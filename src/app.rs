//! CineMosaic window: choose master + cut list, grab the stills, set up the sheet with a live preview, save it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();
mod mac;
mod presets;
mod tc;
use presets::{PRESETS, apply_preset, load_presets, preset_json, preset_menu, store_presets};
use tc::{clipboard_text, tc_edit, tc_parts, tc_pasted};
use mac::color_panel;
#[cfg(target_os = "macos")]
use mac::{mac_app, mac_menu, mac_window};

use image::RgbImage;
use slint::{ComponentHandle, Image, Rgb8Pixel, SharedPixelBuffer};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use slint::{ModelRc, SharedString, VecModel};
use cinemosaic::bin::{BinCut, sequences};
use cinemosaic::timeline::{Item, Part, Pick, Rate, Shot, Timeline, Track, format_tc};
use cinemosaic::report::{self, Outcome, Report};
use cinemosaic::{Gate, Job, UPDATE_APP, avb, export, frames, grab_stills, layout, log, master_jobs, media, pick, picks, project, read_cut_list, selftest, update};

/// Long side of the preview image in pixels (twice the on-screen size for Retina).
const PREVIEW_PX: f64 = 1800.0;
/// Width of the in-memory copies the preview is drawn from.
const THUMB_W: u32 = 480;

#[derive(Default)]
struct State {
    /// master mode: master files and cut lists; a master and the list with the same file name belong together
    masters: Vec<(PathBuf, frames::VideoInfo)>,
    lists: Vec<PathBuf>,
    /// masters without a cut list whose cuts were found in the picture: first frame of every shot, then the frame count
    detected: Vec<(PathBuf, Vec<i64>)>,
    timeline: Option<Timeline>,
    shots: Vec<Shot>,
    /// full-size stills on disk (export) and small copies (preview), by shot and place in the shot; a sheet change
    /// can need stills not grabbed yet: they stay grey until "Grab stills" fetches just those
    stills: HashMap<Pick, Result<PathBuf, String>>,
    thumbs: Arc<HashMap<Pick, RgbImage>>,
    /// grabs since the window opened (never reset): each writes into its own folder
    grabs: usize,
    /// frames grabbed so far by media file and frame: a changed cut reuses them instead of grabbing again
    done: HashMap<(PathBuf, i64), PathBuf>,
    /// the open project and what it held when last saved or opened (to ask before changes get lost)
    project: Option<PathBuf>,
    project_saved: Option<serde_json::Value>,
    /// an opened project's shots and stills, waiting for its source to be read again
    carry: Option<Carry>,
    /// headline font a project or preset asked for, by name, until the font list is loaded
    font_wanted: Option<String>,
    /// this window's preview renderer and the generation of its newest job
    renderer: std::cell::OnceCell<(std::sync::mpsc::Sender<RenderJob>, Arc<AtomicUsize>)>,
    gate: Arc<Gate>,
    saved: Option<PathBuf>,
    /// bin mode: the bin, the chosen sequence read from it, volumes (switched on or off)
    bin: Option<PathBuf>,
    cut: Option<Arc<BinCut>>,
    volumes: Vec<(PathBuf, bool)>,
    /// how many sequences the shots come from (bin mode, "All sequences")
    sequences: usize,
    /// names of the bin's sequences (the menu adds "All sequences" in front)
    sequence_names: Vec<String>,
    /// which of them are shown (all on = all sequences, also those added to the bin later)
    seq_on: Vec<bool>,
    /// readings of the bin started; only the newest one's result is shown
    bin_loads: usize,
    /// width / height of the grabbed stills (bin mode has no master to ask)
    still_aspect: Option<f64>,
}

/// What an opened project showed: taken over for the shots that are still the same once the source is read again,
/// or shown as saved when the source cannot be read.
struct Carry {
    shots: Vec<Shot>,
    stills: HashMap<Pick, Result<PathBuf, String>>,
    thumbs: HashMap<Pick, RgbImage>,
    still_aspect: Option<f64>,
    timeline: Timeline,
    sequences: usize,
    info: String,
}

impl State {
    fn frame_aspect(&self) -> f64 {
        self.still_aspect.or(self.masters.first().map(|m| m.1.aspect)).unwrap_or(16.0 / 9.0)
    }
    fn rate(&self) -> Rate {
        self.timeline.as_ref().map_or(Rate { num: 25, den: 1 }, |t| t.rate)
    }
    /// The stills that fill the sheet as it is set now.
    fn picks(&self, sheet: &layout::Sheet, picture: Option<f64>) -> Vec<Pick> {
        picks(&self.shots, sheet, picture.unwrap_or(self.frame_aspect()))
    }
}

thread_local! {
    /// the project windows, oldest first; each has its own State under its `win-id`
    static WINDOWS: RefCell<Vec<AppWindow>> = const { RefCell::new(Vec::new()) };
    static STATES: RefCell<HashMap<i32, Rc<RefCell<State>>>> = RefCell::new(HashMap::new());
    /// the project window that was in front last (menus of the other windows act on it)
    static FRONT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

/// The state of `app`'s window, like `RefCell::with` (a closed window gets a fresh, unused one).
fn with_state<R>(app: &AppWindow, f: impl FnOnce(&RefCell<State>) -> R) -> R {
    let rc = STATES.with(|s| s.borrow_mut().entry(app.get_win_id()).or_default().clone());
    f(&rc)
}

/// The project window in front, else any.
fn front_window() -> Option<AppWindow> {
    let id = FRONT.with(|f| f.get());
    WINDOWS.with(|w| {
        let w = w.borrow();
        w.iter().find(|a| a.get_win_id() == id).or(w.last()).map(|a| a.clone_strong())
    })
}

fn all_windows() -> Vec<AppWindow> {
    WINDOWS.with(|w| w.borrow().iter().map(|a| a.clone_strong()).collect())
}

/// (code, name in its own language, manual). New language: add it here, in scripts/translations.py and docs/help/<code>.md.
const LANGUAGES: [(&str, &str, &str); 4] = [
    ("en", "English", include_str!("../docs/help/en.md")),
    ("de", "Deutsch", include_str!("../docs/help/de.md")),
    ("fr", "Français", include_str!("../docs/help/fr.md")),
    ("es", "Español", include_str!("../docs/help/es.md")),
];

/// Index into LANGUAGES of the current language; read by worker threads too.
static LANG: AtomicUsize = AtomicUsize::new(0);

/// Text in the current language: [English, German, French, Spanish].
fn tr(t: [&str; 4]) -> String {
    cinemosaic::pick(LANGUAGES[LANG.load(Ordering::Relaxed)].0, t).to_string()
}

macro_rules! repaint_on_resize {
    ($w:expr) => {{
        let weak = $w.as_weak();
        $w.global::<Shared>().on_resized(move || weak.upgrade().iter().for_each(repaint_soon));
    }};
}

#[derive(Clone)]
struct Settings {
    language: String,
    dark: bool,
    /// settings tab opened by Settings… (0 General, 1 Updates, 2 Support; About is never remembered)
    tab: i32,
    /// "Get test versions": also offer GitHub prereleases (always while a test version runs)
    betas: bool,
    /// not saved; shared with the threads that check and install
    update: Arc<Mutex<Update>>,
    /// the five colour presets of the frame line [0], the headline [1] and the sheet background [2], "#rrggbb"
    palettes: [Vec<String>; 3],
}

/// Colour presets before the user changes them: black, dark grey, grey, white, red.
const PALETTE: [&str; 5] = ["#000000", "#3c4146", "#8a8f94", "#ffffff", "#b3261e"];
/// Background presets before the user changes them: white, warm paper, light grey, dark grey, black.
const BG_PALETTE: [&str; 5] = ["#ffffff", "#f4f0e6", "#d9dadb", "#3c4146", "#000000"];

/// [r, g, b] → "#rrggbb"
fn hex_text(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Update state as the windows show it (Shared.update-*).
#[derive(Default)]
struct Update {
    available: Option<update::Release>,
    /// 0 nothing, 1 checking, 2 up to date, 3 installing, 4 failed
    state: i32,
    /// with state 4: 0 check, 1 download, 2 damaged, 3 replace, 4 restart
    error: i32,
    details: String,
}

impl Update {
    fn failed(&mut self, e: update::Error) {
        self.state = 4;
        (self.error, self.details) = match e {
            update::Error::Check => (0, String::new()),
            update::Error::Download => (1, String::new()),
            update::Error::Damaged => (2, String::new()),
            update::Error::Replace(d) => (3, d),
            update::Error::Restart => (4, String::new()),
        };
    }
}

fn app_dir() -> Option<PathBuf> {
    let (var, sub) = if cfg!(windows) { ("APPDATA", "CineMosaic") } else { ("HOME", "Library/Application Support/CineMosaic") };
    std::env::var_os(var).map(|b| PathBuf::from(b).join(sub))
}

fn settings_path() -> Option<PathBuf> {
    app_dir().map(|d| d.join("settings.txt"))
}

fn load_settings() -> Settings {
    let text = settings_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut lines = text.lines();
    let language = lines.next().filter(|l| LANGUAGES.iter().any(|x| x.0 == *l)).unwrap_or("en").to_string();
    let dark = lines.next() != Some("light");
    let tab = lines.next().and_then(|l| l.parse().ok()).filter(|t| matches!(t, 0..=2)).unwrap_or(0);
    let betas = lines.next() == Some("betas");
    // five valid colours, else the defaults (settings from before 2026-11-22 have none)
    let mut palette = |default: [&str; 5]| {
        let p: Vec<String> = lines.next().unwrap_or("").split(',').filter_map(hex_color).map(hex_text).collect();
        if p.len() == 5 { p } else { default.map(String::from).to_vec() }
    };
    let palettes = [palette(PALETTE), palette(PALETTE), palette(BG_PALETTE)];
    Settings { language, dark, tab, betas, update: Arc::default(), palettes }
}

fn save_settings(s: &Settings) {
    // comfort only; if saving fails the defaults apply next time
    if let Some(p) = settings_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(
            p,
            format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
                s.language,
                if s.dark { "dark" } else { "light" },
                s.tab,
                if s.betas { "betas" } else { "final" },
                s.palettes[0].join(","),
                s.palettes[1].join(","),
                s.palettes[2].join(",")
            ),
        );
    }
}

fn apply_menus(shared: &Shared, s: &Settings) {
    show_recent(shared);
    let languages: Vec<MenuChoice> = LANGUAGES.iter().map(|l| MenuChoice { title: l.1.into(), checked: l.0 == s.language }).collect();
    shared.set_languages(ModelRc::new(VecModel::from(languages)));
    shared.set_appearance(ModelRc::new(VecModel::from(vec![s.dark, !s.dark])));
    let names: Vec<slint::SharedString> = LANGUAGES.iter().map(|l| l.1.into()).collect();
    shared.set_language_names(ModelRc::new(VecModel::from(names)));
    shared.set_language_index(LANGUAGES.iter().position(|l| l.0 == s.language).unwrap_or(0) as i32);
    shared.set_dark(s.dark);
    shared.set_betas(s.betas);
    let u = s.update.lock().unwrap();
    shared.set_version(env!("CARGO_PKG_VERSION").into());
    shared.set_update_version(u.available.as_ref().map(|r| r.tag.trim_start_matches('v').to_string()).unwrap_or_default().into());
    shared.set_update_state(u.state);
    shared.set_update_error(u.error);
    shared.set_update_details(u.details.clone().into());
}

/// Asks GitHub for a newer version; `manual` (Settings button) also reports "up to date" and errors.
fn check_for_update(app: &AppWindow, s: &Settings, manual: bool) {
    {
        let mut u = s.update.lock().unwrap();
        if u.state == 1 || u.state == 3 {
            return;
        }
        if manual {
            u.state = 1;
        }
    }
    app.global::<Shared>().invoke_refresh_update();
    let (weak, update, betas) = (app.as_weak(), s.update.clone(), s.betas);
    std::thread::spawn(move || {
        let result = update::check(&UPDATE_APP, env!("CARGO_PKG_VERSION"), betas);
        match &result {
            Ok(Some(r)) => log::write(&format!("update available: {}", r.tag)),
            Ok(None) => {}
            Err(_) => log::write("update check failed"),
        }
        {
            let mut u = update.lock().unwrap();
            if u.state == 3 {
                return;
            }
            match result {
                Ok(r) => {
                    let found = r.is_some();
                    u.available = r;
                    if manual {
                        u.state = if found { 0 } else { 2 };
                    }
                }
                Err(e) if manual => u.failed(e),
                Err(_) => {}
            }
        }
        let _ = slint::invoke_from_event_loop(move || weak.upgrade().iter().for_each(|a| a.global::<Shared>().invoke_refresh_update()));
    });
}

/// Downloads, verifies and swaps in the new version, then quits; the new version starts by itself.
fn install_update(app: &AppWindow, s: &Settings) {
    let release = {
        let mut u = s.update.lock().unwrap();
        let Some(r) = u.available.clone().filter(|_| u.state != 3) else { return };
        u.state = 3;
        r
    };
    log::write(&format!("installing update {}", release.tag));
    app.global::<Shared>().invoke_refresh_update();
    let (weak, update) = (app.as_weak(), s.update.clone());
    std::thread::spawn(move || {
        let result = update::download(&release, &UPDATE_APP).and_then(|new| update::swap_and_restart(&new));
        if let Err(e) = result {
            log::write(&format!("update failed: {e:?}"));
            update.lock().unwrap().failed(e);
            let _ = slint::invoke_from_event_loop(move || weak.upgrade().iter().for_each(|a| a.global::<Shared>().invoke_refresh_update()));
            return;
        }
        log::write("update installed, restarting");
        let _ = slint::invoke_from_event_loop(|| {
            let _ = slint::quit_event_loop();
        });
    });
}

/// A picture of the manual (`![…](images/<lang>/x.png)`, path relative to docs/help): in the app bundle under
/// Resources/help, when run from the repo in docs/help.
fn help_picture(path: &str) -> slint::Image {
    let bundled = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.parent()?.join("Resources/help").join(path)));
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/help").join(path);
    bundled.filter(|p| p.is_file()).or(Some(repo)).and_then(|p| slint::Image::load_from_path(&p).ok()).unwrap_or_default()
}

/// Splits the manual into blocks: "# " = subtitle, "## " = heading, every paragraph its own block
/// (StyledText knows neither headings nor paragraph spacing), a line "![…](path)" a picture. Backticks go, so
/// everything stays in the app font.
fn help_blocks(markdown: &str) -> (String, Vec<HelpBlock>) {
    let mut subtitle = String::new();
    let mut blocks = vec![];
    let mut paragraph = String::new();
    let flush = |paragraph: &mut String, blocks: &mut Vec<HelpBlock>| {
        // "<folder>" would otherwise be an HTML tag
        let text = paragraph.trim().replace('`', "").replace('<', "‹").replace('>', "›");
        if !text.is_empty() {
            let body = slint::StyledText::from_markdown(&text).unwrap_or_else(|_| slint::StyledText::from_plain_text(&text));
            blocks.push(HelpBlock { heading: "".into(), body, picture: Default::default() });
        }
        paragraph.clear();
    };
    for line in markdown.lines() {
        if let Some(t) = line.strip_prefix("# ") {
            subtitle = t.to_uppercase();
        } else if let Some(h) = line.strip_prefix("## ") {
            flush(&mut paragraph, &mut blocks);
            blocks.push(HelpBlock { heading: h.into(), body: Default::default(), picture: Default::default() });
        } else if let Some(path) = line.trim().strip_prefix("![").and_then(|l| l.split_once("](")).and_then(|(_, p)| p.strip_suffix(')')) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(HelpBlock { heading: "".into(), body: Default::default(), picture: help_picture(path) });
        } else if line.trim().is_empty() {
            flush(&mut paragraph, &mut blocks);
        } else {
            paragraph.push_str(line);
            paragraph.push('\n');
        }
    }
    flush(&mut paragraph, &mut blocks);
    (subtitle, blocks)
}

fn update_help(help: &HelpWindow, s: &Settings) {
    let markdown = LANGUAGES.iter().find(|l| l.0 == s.language).unwrap_or(&LANGUAGES[0]).2;
    let (subtitle, blocks) = help_blocks(markdown);
    help.set_subtitle(subtitle.into());
    help.set_blocks(ModelRc::new(VecModel::from(blocks)));
    apply_menus(&help.global::<Shared>(), s);
}

/// Menu of a second window (manual, About, settings) forwards to the project window in front.
fn forward_menu(shared: &Shared) {
    shared.on_select_language(move |i| front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_select_language(i)));
    shared.on_select_dark(move |d| front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_select_dark(d)));
    shared.on_select_betas(move |b| front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_select_betas(b)));
    shared.on_show_help(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_show_help()));
    shared.on_show_about(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_show_about()));
    shared.on_show_log(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_show_log()));
    shared.on_check_update(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_check_update()));
    shared.on_install_update(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_install_update()));
    shared.on_show_release(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_show_release()));
    shared.on_new_project(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_new_project()));
    shared.on_open_project(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_open_project()));
    shared.on_open_recent(move |i| front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_open_recent(i)));
    shared.on_clear_recent(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_clear_recent()));
    shared.on_save_project(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_save_project()));
    shared.on_save_project_as(move || front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_save_project_as()));
    show_recent(shared);
}


/// Short message over the settings after sending (success 3 s, failure 8 s – the file name stays readable).
fn toast(w: &slint::Weak<SettingsWindow>, outcome: &Outcome, lang: &str) {
    let (text, ok) = match outcome {
        Outcome::Sent => (pick(lang, ["Sent – thank you!", "Gesendet – danke!", "Envoyé – merci !", "Enviado, ¡gracias!"]).to_string(), true),
        Outcome::Failed(path) if !path.is_empty() => {
            let file = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (pick(lang, [
                "The report could not be sent. It is saved in Downloads as “{}” – please try again later or send us the file.",
                "Der Bericht konnte nicht gesendet werden. Er liegt in „Downloads“ als „{}“ – bitte später noch einmal versuchen oder uns die Datei schicken.",
                "Le rapport n’a pas pu être envoyé. Il est enregistré dans Téléchargements sous « {} » – réessayez plus tard ou envoyez-nous le fichier.",
                "No se pudo enviar el informe. Está guardado en Descargas como «{}»: inténtalo más tarde o envíanos el archivo.",
            ]).replace("{}", &file), false)
        }
        Outcome::Failed(_) => (pick(lang, ["The report could not be sent or saved.", "Der Bericht konnte weder gesendet noch gesichert werden.", "Le rapport n’a pu être ni envoyé ni enregistré.", "No se pudo enviar ni guardar el informe."]).to_string(), false),
    };
    let Some(win) = w.upgrade() else { return };
    win.set_toast_ok(ok);
    win.set_toast_text(text.clone().into());
    win.set_toast_visible(true);
    let w = w.clone();
    slint::Timer::single_shot(Duration::from_secs(if ok { 3 } else { 8 }), move || {
        // slides out; the text stays until the next toast so the box keeps its size while fading
        w.upgrade().into_iter().filter(|w| w.get_toast_text() == text.as_str()).for_each(|w| w.set_toast_visible(false));
    });
}

/// Settings → Support: self-test and problem report (same flow as Mountpoint). After a successful send the page
/// returns to the settings and shows the toast; on failure it stays, so nothing typed is lost.
fn wire_support(w: &SettingsWindow, settings: &Rc<RefCell<Settings>>) {
    let last: Rc<RefCell<Option<Report>>> = Rc::default();
    w.on_open_coffee(|| {
        let _ = std::process::Command::new("open").arg("https://buymeacoffee.com/moritzpoth").spawn();
    });
    w.on_back({
        let weak = w.as_weak();
        move || {
            let Some(w) = weak.upgrade() else { return };
            if w.get_test_phase() == 2 {
                w.set_test_phase(0);
                w.set_test_rows(ModelRc::default());
            }
            w.set_page(0);
        }
    });
    w.on_start_selftest({
        let (weak, settings, last) = (w.as_weak(), settings.clone(), last.clone());
        move || {
            let Some(w) = weak.upgrade() else { return };
            let lang = settings.borrow().language.clone();
            w.set_test_phase(1);
            w.set_test_note("".into());
            w.set_test_rows(ModelRc::default());
            let (weak, last) = (weak.clone(), last.clone());
            let (tx, rx) = std::sync::mpsc::channel::<Report>();
            let l = lang.clone();
            let progress_weak = weak.clone();
            std::thread::spawn(move || {
                let r = selftest::run(&l, &mut |done, next| {
                    let rows: Vec<TestRow> = done.iter().map(|c| TestRow {
                        title: selftest::title(&c.id, &l).into(),
                        detail: c.detail.clone().into(),
                        status: match c.status { "pass" => 0, "fail" => 1, _ => 2 },
                    }).collect();
                    let next = if next.is_empty() { String::new() } else { selftest::title(selftest::CHECKS.iter().find(|c| c.1 == next).map_or(next, |c| c.0), &l) + "…" };
                    let pw = progress_weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = pw.upgrade() {
                            w.set_test_rows(ModelRc::new(VecModel::from(rows)));
                            w.set_test_current(next.into());
                        }
                    });
                });
                let _ = tx.send(r);
            });
            // the report itself is not Send through slint's closure in every version: poll the channel
            let timer = Rc::new(slint::Timer::default());
            let t = timer.clone();
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(200), move || {
                let Ok(r) = rx.try_recv() else { return };
                t.stop();
                if let Some(w) = weak.upgrade() {
                    let n = |s: &str| r.results.iter().filter(|c| c.status == s).count();
                    w.set_test_summary(pick(&lang, ["{p} passed, {f} failed, {s} skipped", "{p} bestanden, {f} fehlgeschlagen, {s} übersprungen", "{p} réussis, {f} échoués, {s} ignorés", "{p} superadas, {f} fallidas, {s} omitidas"])
                        .replace("{p}", &n("pass").to_string()).replace("{f}", &n("fail").to_string()).replace("{s}", &n("skip").to_string()).into());
                    w.set_test_failed(n("fail") > 0);
                    w.set_test_phase(2);
                }
                *last.borrow_mut() = Some(r);
            });
        }
    });
    w.on_save_selftest({
        let (weak, settings, last) = (w.as_weak(), settings.clone(), last.clone());
        move || {
            let Some(r) = last.borrow().clone() else { return };
            let lang = settings.borrow().language.clone();
            let note = match report::save(&r) {
                Some(p) => {
                    let _ = std::process::Command::new("open").arg("-R").arg(&p).spawn();
                    pick(&lang, ["Saved as “{}” in Downloads.", "Gesichert als „{}“ in „Downloads“.", "Enregistré sous « {} » dans Téléchargements.", "Guardado como «{}» en Descargas."])
                        .replace("{}", &p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                }
                None => pick(&lang, ["The report could not be saved.", "Der Bericht konnte nicht gesichert werden.", "Le rapport n’a pas pu être enregistré.", "No se pudo guardar el informe."]).into(),
            };
            weak.upgrade().into_iter().for_each(|w| w.set_test_note(note.clone().into()));
        }
    });
    let send = move |weak: slint::Weak<SettingsWindow>, lang: String, report: Report, on_sent: Box<dyn Fn(&SettingsWindow)>| {
        let Some(w) = weak.upgrade() else { return };
        w.set_sending(true);
        let (tx, rx) = std::sync::mpsc::channel::<Outcome>();
        std::thread::spawn(move || {
            let _ = tx.send(report::send(&report));
        });
        let timer = Rc::new(slint::Timer::default());
        let t = timer.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(200), move || {
            let Ok(outcome) = rx.try_recv() else { return };
            t.stop();
            if let Some(w) = weak.upgrade() {
                w.set_sending(false);
                if outcome == Outcome::Sent {
                    on_sent(&w);
                    w.set_page(0);
                }
                toast(&weak, &outcome, &lang);
            }
        });
    };
    w.on_send_selftest({
        let (weak, settings, last) = (w.as_weak(), settings.clone(), last.clone());
        move || {
            let Some(r) = last.borrow().clone() else { return };
            send(weak.clone(), settings.borrow().language.clone(), r, Box::new(|w: &SettingsWindow| {
                w.set_test_phase(0);
                w.set_test_rows(ModelRc::default());
            }));
        }
    });
    w.on_send_problem({
        let (weak, settings) = (w.as_weak(), settings.clone());
        move || {
            let Some(w) = weak.upgrade() else { return };
            let lang = settings.borrow().language.clone();
            let r = report::problem(&w.get_report_text(), &w.get_report_contact(), w.get_report_attach(), &lang);
            send(weak.clone(), lang, r, Box::new(|w: &SettingsWindow| {
                w.set_report_text("".into());
                w.set_report_contact("".into());
            }));
        }
    });
}

type Win<T> = Rc<RefCell<Option<T>>>;

/// The windows besides the main one, created when first opened.
#[derive(Clone, Default)]
struct Others {
    help: Win<HelpWindow>,
    settings: Win<SettingsWindow>,
}


fn repaint_soon<T: ComponentHandle + 'static>(c: &T) {
    for ms in [50, 200, 600, 1500] {
        let weak = c.as_weak();
        slint::Timer::single_shot(Duration::from_millis(ms), move || weak.upgrade().into_iter().for_each(|c| c.window().request_redraw()));
    }
}

/// Language or appearance changed: every open window follows.
fn apply_all(others: &Others, s: &Settings) {
    all_windows().iter().for_each(|a| apply_menus(&a.global::<Shared>(), s));
    others.settings.borrow().iter().for_each(|w| apply_menus(&w.global::<Shared>(), s));
    others.help.borrow().iter().for_each(|h| update_help(h, s));
    #[cfg(target_os = "macos")]
    {
        // Slint rebuilds the menu after this event; add the item back once that is done
        let title = front_window().map(|a| a.get_txt_settings_menu().to_string()).unwrap_or_default();
        slint::Timer::single_shot(Duration::from_millis(150), move || mac_menu::set_settings_title(&title));
    }
}

fn cache_root() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Library/Caches/CineMosaic")
}

/// This run's stills: one folder per process, so two open copies of the app never delete each other's stills.
fn cache_dir() -> PathBuf {
    cache_root().join(format!("stills-{}", std::process::id()))
}

/// Removes the stills this app left behind: at the start the folders of runs that are over (a crash or a forced
/// quit leaves them), at the end this run's own. Only folders named "stills…" inside the app's cache folder.
fn clean_stills(own_too: bool) {
    let running = |pid: &str| std::process::Command::new("/bin/ps").args(["-p", pid]).output().is_ok_and(|o| o.status.success());
    for e in std::fs::read_dir(cache_root()).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let stale = match name.strip_prefix("stills-") {
            Some(pid) if pid == std::process::id().to_string() => own_too,
            Some(pid) => !running(pid),
            None => name == "stills", // folder of the first versions
        };
        if stale && e.path().is_dir() {
            match std::fs::remove_dir_all(e.path()) {
                Ok(()) => log::write(&format!("removed stills {name}")),
                Err(err) => log::write(&format!("could not remove stills {name}: {err}")),
            }
        }
    }
}

/// Long names shortened in the middle, so start and end stay visible.
fn shorten(s: &str, max: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= max {
        return s.to_string();
    }
    let head = (max - 1) / 2;
    c[..head].iter().chain(['…'].iter()).chain(c[c.len() - (max - 1 - head)..].iter()).collect()
}

fn file_name(p: &Path) -> String {
    shorten(&p.file_name().unwrap_or_default().to_string_lossy(), 44)
}

/// "25", "23.976", "29.97"
fn fps_text(fps: f64) -> String {
    let s = format!("{fps:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A decimal number field in `min..=max`; Err = `field`.
fn num(s: &str, min: f64, max: f64, field: &'static str) -> Result<f64, &'static str> {
    let v: f64 = s.trim().replace(',', ".").parse().map_err(|_| field)?;
    if (min..=max).contains(&v) { Ok(v) } else { Err(field) }
}

/// A whole-number field (lengths, margin, sizes); Err = `field`.
fn whole(s: &str, field: &'static str) -> Result<f64, &'static str> {
    s.trim().parse::<i64>().map(|v| v as f64).map_err(|_| field)
}

/// A length field in mm, from mm or px (`per` = mm per unit), in `min..=max` mm.
fn length(s: &str, per: f64, min: f64, max: f64, field: &'static str) -> Result<f64, &'static str> {
    let v = whole(s, field)? * per;
    if (min..=max).contains(&v) { Ok(v) } else { Err(field) }
}

/// mm per unit of the length fields: 1 in mm, 25.4 / dpi in px (an unusable dpi counts as 300).
fn mm_per_unit(app: &AppWindow) -> f64 {
    if app.get_px_units() { 25.4 / num(&app.get_dpi(), 10.0, 1200.0, "dpi").unwrap_or(300.0) } else { 1.0 }
}

/// mm per pixel of the sheet: frame line and headline place are always in px (an unusable dpi counts as 300).
fn mm_per_px(app: &AppWindow) -> f64 {
    25.4 / num(&app.get_dpi(), 10.0, 1200.0, "dpi").unwrap_or(300.0)
}

/// `mm` as a whole number in the unit of the length fields.
fn in_unit(mm: f64, per: f64) -> SharedString {
    format!("{}", (mm / per).round() as i64).into()
}

/// Reads the sheet fields; Err = name of the first field that is not a usable number.
fn read_sheet(app: &AppWindow) -> Result<layout::Sheet, &'static str> {
    let dpi = num(&app.get_dpi(), 10.0, 1200.0, "dpi")?;
    let per = if app.get_px_units() { 25.4 / dpi } else { 1.0 };
    let w = length(&app.get_custom_w(), per, 10.0, 5000.0, "width")?;
    let h = length(&app.get_custom_h(), per, 10.0, 5000.0, "height")?;
    // margin and gap always in px of the sheet (user 2026-11-22)
    // negative: the stills run off the page (they fill it to the edges, the overhang is cut off)
    let margin = length(&app.get_margin(), 25.4 / dpi, -w.min(h), w.min(h) / 2.0 - 1.0, "margin")?;
    let gap = length(&app.get_gap(), 25.4 / dpi, 0.0, 500.0, "gap")?;
    let columns = if app.get_auto_cols() { None } else { Some(num(&app.get_cols(), 1.0, 1000.0, "columns")? as u32) };
    Ok(layout::Sheet { width_mm: w, height_mm: h, dpi, margin_mm: margin, gap_mm: gap, columns, ..Default::default() })
}

/// Paper from the list (1..=7 DIN A0..A6, 8.. social media sizes): its size into width × height, a DIN size in the
/// orientation and unit chosen, a social media size in pixels as it is (unit switches to px).
fn show_paper(app: &AppWindow) {
    let i = app.get_paper() as usize;
    if let Some(&(w, h, _)) = i.checked_sub(layout::DIN.len() + 1).and_then(|k| layout::SOCIAL.get(k)) {
        switch_unit(app, true); // would make the paper Custom
        app.set_paper(i as i32);
        app.set_custom_w(w.to_string().into());
        app.set_custom_h(h.to_string().into());
        if w != h {
            app.set_landscape(w > h);
        }
        return;
    }
    let Some(d) = i.checked_sub(1).and_then(|i| layout::DIN.get(i)) else { return };
    let (short, long) = (d.1.min(d.2), d.1.max(d.2));
    let (w, h) = if app.get_landscape() { (long, short) } else { (short, long) };
    let per = mm_per_unit(app);
    app.set_custom_w(in_unit(w, per));
    app.set_custom_h(in_unit(h, per));
}

/// Width × height typed or dragged: orientation follows, and the paper list shows the A size it matches or Custom.
fn size_edited(app: &AppWindow) {
    let per = mm_per_unit(app);
    if let (Ok(w), Ok(h)) = (whole(&app.get_custom_w(), ""), whole(&app.get_custom_h(), "")) {
        let typed = (w, h);
        let (w, h) = (w * per, h * per);
        if w != h {
            app.set_landscape(w > h);
        }
        // within 0.6 mm: px sizes are rounded
        let near = |a: f64, b: f64| (a - b).abs() < 0.6;
        let a_size = layout::DIN.iter().position(|d| near(d.1.min(d.2), w.min(h)) && near(d.1.max(d.2), w.max(h)));
        // social media sizes: exactly these pixels, in this orientation
        let social = app.get_px_units().then(|| layout::SOCIAL.iter().position(|s| (s.0 as f64, s.1 as f64) == typed)).flatten();
        let i = a_size.map(|i| i + 1).or(social.map(|k| k + layout::DIN.len() + 1)).unwrap_or(0);
        app.set_paper(i as i32);
    }
    refresh(app);
}

type Getter = fn(&AppWindow) -> SharedString;
type Setter = fn(&AppWindow, SharedString);

/// Every length field into the other unit, rounded to whole numbers.
fn switch_unit(app: &AppWindow, px: bool) {
    if app.get_px_units() == px {
        return;
    }
    let old = mm_per_unit(app);
    app.set_px_units(px);
    let new = mm_per_unit(app);
    let fields: [(Getter, Setter, f64); 2] = [
        (AppWindow::get_custom_w, AppWindow::set_custom_w, 1.0),
        (AppWindow::get_custom_h, AppWindow::set_custom_h, 1.0),
    ];
    for (get, set, least) in fields {
        // an unusable value stays as typed (and red)
        if let Ok(v) = num(&get(app), f64::MIN, f64::MAX, "") {
            set(app, format!("{}", (v * old / new).round().max(least) as i64).into());
        }
    }
    // a DIN size exactly, not a rounded conversion; a social media size is pixels, in mm it is Custom
    if app.get_paper() as usize > layout::DIN.len() {
        app.set_paper(0);
    } else {
        show_paper(app);
    }
}

/// "#rrggbb", "rrggbb" or "#rgb" → colour.
fn hex_color(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(&h, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// The colour presets into a window.
fn show_palettes(app: &AppWindow, s: &Settings) {
    let swatches = |k: usize| {
        let v: Vec<Swatch> = s.palettes[k]
            .iter()
            .map(|h| {
                let [r, g, b] = hex_color(h).unwrap_or([0, 0, 0]);
                Swatch { hex: h.into(), c: slint::Color::from_rgb_u8(r, g, b) }
            })
            .collect();
        ModelRc::new(VecModel::from(v))
    };
    app.set_line_palette(swatches(0));
    app.set_head_palette(swatches(1));
    app.set_bg_palette(swatches(2));
}

/// Colour of palette `k` (0 frame line, 1 headline, 2 background) as typed.
fn color_of(app: &AppWindow, k: i32) -> SharedString {
    [AppWindow::get_line_color, AppWindow::get_head_color, AppWindow::get_bg_color][k.clamp(0, 2) as usize](app)
}

fn set_color(app: &AppWindow, k: i32, v: SharedString) {
    [AppWindow::set_line_color, AppWindow::set_head_color, AppWindow::set_bg_color][k.clamp(0, 2) as usize](app, v)
}

/// The chosen preset of palette `k` (-1: none).
fn slot_of(app: &AppWindow, k: i32) -> i32 {
    [AppWindow::get_line_slot, AppWindow::get_head_slot, AppWindow::get_bg_slot][k.clamp(0, 2) as usize](app)
}

/// The chosen preset follows the colour (a preset or project brings its own): the preset with this colour, or none.
fn match_slots(app: &AppWindow) {
    use slint::Model;
    let slot = |pal: ModelRc<Swatch>, value: SharedString| {
        let c = hex_color(&value);
        c.and_then(|c| pal.iter().position(|sw| hex_color(&sw.hex) == Some(c))).map_or(-1, |i| i as i32)
    };
    app.set_line_slot(slot(app.get_line_palette(), app.get_line_color()));
    app.set_head_slot(slot(app.get_head_palette(), app.get_head_color()));
    app.set_bg_slot(slot(app.get_bg_palette(), app.get_bg_color()));
}

/// Preset `slot` of palette `k` gets colour `c`; every window shows it. `save`: write the settings now (the colour
/// panel saves once when it closes, not with every colour it passes through).
fn set_palette_entry(settings: &RefCell<Settings>, k: usize, slot: usize, c: [u8; 3], save: bool) {
    let mut s = settings.borrow_mut();
    let Some(e) = s.palettes[k].get_mut(slot) else { return };
    *e = hex_text(c);
    if save {
        save_settings(&s);
    }
    all_windows().iter().for_each(|w| show_palettes(w, &s));
}

thread_local! {
    /// installed font families (menu order) and the faces already loaded
    static FONTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static FACES: RefCell<std::collections::HashMap<(String, bool, bool), Option<cinemosaic::fonts::Face>>> = RefCell::new(Default::default());
}

/// Trim area, frame line and headline from the Design tab and the crop-marks switch; Err = the field that is not
/// usable. `aspect` is the picture format the trim area takes.
fn read_decor(app: &AppWindow, sheet: &mut layout::Sheet, aspect: f64) -> Result<(), &'static str> {
    // frame line and headline place in px of the sheet, whatever the unit of the Sheet tab
    let per = 25.4 / sheet.dpi;
    sheet.trim = app.get_trim().then_some(aspect);
    let bg = hex_color(&app.get_bg_color()).ok_or("bg-color")?;
    app.set_bg_swatch(slint::Color::from_rgb_u8(bg[0], bg[1], bg[2]));
    sheet.background = bg;
    let line_color = hex_color(&app.get_line_color());
    if let Some([r, g, b]) = line_color {
        app.set_line_swatch(slint::Color::from_rgb_u8(r, g, b));
    }
    let inset = length(&app.get_line_inset(), per, 0.0, 200.0, "line-inset")?;
    sheet.line = if app.get_line_on() {
        Some(layout::Line {
            width_mm: length(&app.get_line_width(), per, 0.05, 20.0, "line-width")?,
            inset_mm: inset,
            color: line_color.ok_or("line-color")?,
            pattern: layout::Pattern::ALL[(app.get_line_pattern().max(0) as usize).min(layout::Pattern::ALL.len() - 1)],
        })
    } else {
        None
    };
    let text = app.get_headline().to_string();
    let head_color = hex_color(&app.get_head_color());
    if let Some([r, g, b]) = head_color {
        app.set_head_swatch(slint::Color::from_rgb_u8(r, g, b));
    }
    sheet.headline = None;
    if !text.trim().is_empty() {
        let size = whole(&app.get_head_size(), "head-size").and_then(|v| if (2.0..=1000.0).contains(&v) { Ok(v) } else { Err("head-size") })?;
        let color = head_color.ok_or("head-color")?;
        let family = FONTS.with(|f| f.borrow().get(app.get_font() as usize).cloned());
        let (bold, italic) = (app.get_head_bold(), app.get_head_italic());
        let face = family.and_then(|fam| {
            FACES.with(|c| c.borrow_mut().entry((fam.clone(), bold, italic)).or_insert_with(|| cinemosaic::fonts::face(&fam, bold, italic)).clone())
        });
        if let Some(face) = face {
            let anchor = app.get_head_anchor().clamp(0, 8);
            let align = [layout::Align::Left, layout::Align::Center, layout::Align::Right][anchor as usize % 3];
            let valign = [layout::VAlign::Top, layout::VAlign::Middle, layout::VAlign::Bottom][anchor as usize / 3];
            let dx_mm = length(&app.get_head_dx(), per, -2000.0, 2000.0, "head-dx")?;
            let dy_mm = length(&app.get_head_dy(), per, -2000.0, 2000.0, "head-dy")?;
            let pad = |s: SharedString, field| length(&s, per, 0.0, 500.0, field);
            let pad_mm = [
                pad(app.get_head_pad_t(), "head-pad-t")?,
                pad(app.get_head_pad_r(), "head-pad-r")?,
                pad(app.get_head_pad_b(), "head-pad-b")?,
                pad(app.get_head_pad_l(), "head-pad-l")?,
            ];
            sheet.headline = Some(layout::Headline { text, face, size_pt: size, color, align, valign, dx_mm, dy_mm, pad_mm, wrap: app.get_head_wrap() });
        }
    }
    Ok(())
}

/// Everything the sheet needs from the fields: sheet with its decoration and the picture format.
fn read_all(app: &AppWindow) -> Result<(layout::Sheet, Option<f64>), &'static str> {
    let mut sheet = read_sheet(app)?;
    let picture = read_picture(app)?;
    let aspect = picture.unwrap_or_else(|| with_state(app, |st| st.borrow().frame_aspect()));
    read_decor(app, &mut sheet, aspect)?;
    Ok((sheet, picture))
}

/// Loads the installed font families in a thread (that takes a moment) and fills the Font menu; Helvetica Neue first
/// choice, like the apps' headlines.
fn load_fonts() {
    std::thread::spawn(move || {
        let names = cinemosaic::fonts::families();
        let _ = slint::invoke_from_event_loop(move || {
            FONTS.with(|f| *f.borrow_mut() = names);
            for app in all_windows() {
                show_fonts(&app);
                refresh(&app);
            }
        });
    });
}

/// The font list of a window, Helvetica Neue chosen (nothing while the fonts are still loading).
fn show_fonts(app: &AppWindow) {
    FONTS.with(|f| {
        let names = f.borrow();
        let model: Vec<SharedString> = names.iter().map(|n| n.as_str().into()).collect();
        app.set_fonts(ModelRc::new(VecModel::from(model)));
        app.set_font(default_font(&names) as i32);
    });
    // a project or preset opened while the fonts were still loading gets its font now
    apply_font(app);
}

/// The standard headline font: Helvetica Neue, else Helvetica, else Arial, else the first.
fn default_font(names: &[String]) -> usize {
    ["Helvetica Neue", "Helvetica", "Arial"].iter().find_map(|w| names.iter().position(|n| n == w)).unwrap_or(0)
}

/// Sets the font a project or preset asked for, by name, once the font list is loaded. A font this Mac does not
/// have: first say so (with its name), then take the standard font.
fn apply_font(app: &AppWindow) {
    if FONTS.with(|f| f.borrow().is_empty()) {
        return; // still loading: show_fonts comes back here
    }
    let Some(name) = with_state(app, |st| st.borrow_mut().font_wanted.take()) else { return };
    let (found, fallback) = FONTS.with(|f| {
        let names = f.borrow();
        let d = default_font(&names);
        (names.iter().position(|n| *n == name), (d, names.get(d).cloned().unwrap_or_default()))
    });
    match found {
        Some(i) => app.set_font(i as i32),
        None => {
            log::write(&format!("font not installed: {name}"));
            rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title(tr(["Font not installed", "Schrift nicht installiert", "Police non installée", "Fuente no instalada"]))
                .set_description(
                    tr(["The font “{f}” cannot be loaded – it is not installed on this Mac. The headline is set in the standard font, {d}, instead.", "Die Schrift „{f}“ lässt sich nicht laden – sie ist auf diesem Mac nicht installiert. Die Headline wird deshalb in der Standardschrift {d} gesetzt.", "La police « {f} » ne peut pas être chargée – elle n’est pas installée sur ce Mac. Le titre utilise donc la police standard, {d}.", "No se puede cargar la fuente «{f}»: no está instalada en este Mac. Por eso el título usa la fuente estándar, {d}."])
                        .replace("{f}", &name)
                        .replace("{d}", &fallback.1),
                )
                .show();
            app.set_font(fallback.0 as i32);
        }
    }
    refresh(app);
}

/// Picture format to crop to: None = full frame; Err = width : height is not a format.
fn read_picture(app: &AppWindow) -> Result<Option<f64>, &'static str> {
    if app.get_picture() == 0 {
        return Ok(None);
    }
    layout::parse_aspect(&app.get_aspect()).map(Some).ok_or("aspect")
}

/// The picture-format menu: Full frame, the film formats, Custom (menu index = `picture`).
fn picture_menu(app: &AppWindow) {
    let entry = |title: String| MenuChoice { title: title.into(), checked: false };
    let mut items = vec![entry(tr(["Full frame", "Vollbild", "Image entière", "Imagen completa"]))];
    items.extend(layout::PICTURE.iter().map(|&(w, h)| entry(format!("{}:{}", layout::ratio_text(w), layout::ratio_text(h)))));
    items.push(entry(tr(["Custom", "Eigenes", "Personnalisé", "Personalizado"])));
    app.set_picture_menu(ModelRc::new(VecModel::from(items)));
}

/// The paper menu: Custom, DIN A0–A6, social media sizes in pixels (with their platforms); the two section titles
/// cannot be chosen. Menu index ↔ `paper`: see the paper row in app.slint.
fn paper_menu(app: &AppWindow) {
    let entry = |title: String, header: bool| MenuChoice { title: title.into(), checked: header };
    let mut items = vec![entry(tr(["Custom", "Eigenes", "Personnalisé", "Personalizado"]), false)];
    items.push(entry("DIN".into(), true));
    items.extend(layout::DIN.iter().map(|d| entry(format!("DIN {}", d.0), false)));
    items.push(entry(tr(["Social media", "Social Media", "Réseaux sociaux", "Redes sociales"]), true));
    items.extend(layout::SOCIAL.iter().map(|&(w, h, p)| {
        let p = if p.is_empty() { tr(["square", "quadratisch", "carré", "cuadrado"]) } else { p.to_string() };
        entry(format!("{w} × {h} px – {p}"), false)
    }));
    app.set_paper_menu(ModelRc::new(VecModel::from(items)));
}

/// The button text of the paper menu: "DIN A4", "1080 × 1920 px" or Custom.
fn paper_label(app: &AppWindow) {
    let i = app.get_paper() as usize;
    let label = match (i.checked_sub(1).and_then(|k| layout::DIN.get(k)), i.checked_sub(layout::DIN.len() + 1).and_then(|k| layout::SOCIAL.get(k))) {
        (Some(d), _) => format!("DIN {}", d.0),
        (_, Some(s)) => format!("{} × {} px", s.0, s.1),
        _ => tr(["Custom", "Eigenes", "Personnalisé", "Personalizado"]),
    };
    app.set_paper_label(label.into());
}

/// A format from the menu: its width : height into the field (Full frame shows the stills' own format).
fn show_picture(app: &AppWindow) {
    let i = app.get_picture() as usize;
    let (w, h) = match i {
        0 => {
            let a = with_state(app, |st| st.borrow().frame_aspect());
            // the name the format goes by (2048 × 858 is 2.39:1, not 2.387:1), anything else as x:1
            layout::nominal(a).unwrap_or((a, 1.0))
        }
        i if i <= layout::PICTURE.len() => layout::PICTURE[i - 1],
        _ => return, // Custom keeps what is typed
    };
    app.set_aspect(format!("{}:{}", layout::ratio_text(w), layout::ratio_text(h)).into());
}

/// The button text of the picture-format menu.
fn picture_label(app: &AppWindow) {
    let i = app.get_picture() as usize;
    let label = match i {
        0 => tr(["Full frame", "Vollbild", "Image entière", "Imagen completa"]),
        i if i <= layout::PICTURE.len() => format!("{}:{}", layout::ratio_text(layout::PICTURE[i - 1].0), layout::ratio_text(layout::PICTURE[i - 1].1)),
        _ => tr(["Custom", "Eigenes", "Personnalisé", "Personalizado"]),
    };
    app.set_picture_label(label.into());
}

/// Width : height typed or dragged: the menu shows the format it matches, else Custom.
fn picture_edited(app: &AppWindow) {
    if let Some(a) = layout::parse_aspect(&app.get_aspect()) {
        let found = layout::PICTURE.iter().position(|(w, h)| (w / h - a).abs() < 0.0005);
        app.set_picture(found.map_or(layout::PICTURE.len() + 1, |i| i + 1) as i32);
    } else if app.get_picture() == 0 {
        app.set_picture(layout::PICTURE.len() as i32 + 1);
    }
    refresh(app);
}

/// Still `img` cut to the chosen format.
fn cropped(img: RgbImage, picture: Option<f64>) -> RgbImage {
    match picture {
        Some(a) => layout::crop(&img, a),
        None => img,
    }
}


/// Draws the preview and the grid line from the current fields and stills.
fn refresh(app: &AppWindow) {
    let sheet_picture = read_all(app);
    let (sheet, picture) = match sheet_picture {
        Ok(s) => s,
        Err(field) => {
            app.set_invalid(field.into());
            return; // keep the last good preview
        }
    };
    app.set_invalid("".into());
    if app.get_picture() == 0 {
        show_picture(app);
    }
    picture_label(app);
    paper_label(app);
    app.set_page_aspect((sheet.width_mm / sheet.height_mm) as f32);
    with_state(app, |st| {
        let st = st.borrow();
        let picks = st.picks(&sheet, picture);
        let n = picks.len();
        // grabbed but not complete: the sheet changed and needs stills that were not grabbed yet
        let missing = if st.stills.is_empty() { 0 } else { picks.iter().filter(|p| !st.stills.contains_key(p)).count() };
        app.set_missing(missing as i32);
        let aspect = picture.unwrap_or(st.frame_aspect());
        let (w, h) = sheet.size_px();
        // the sheet's own size stands in width × height
        let mut info = String::new();
        if n > 0 {
            let g = sheet.grid(n, aspect);
            info = format!("{} × {} · {} {} × {} px", g.cols, g.rows, tr(["still", "Still", "image", "fotograma"]), g.cell_w.round(), g.cell_h.round());
        }
        let gb = w as f64 * h as f64 * 3.0 / 1e9;
        if gb >= 1.0 {
            info += &tr([" · needs about {} GB of memory to save", " · Speichern braucht etwa {} GB Arbeitsspeicher", " · l’enregistrement demande environ {} Go de mémoire", " · guardar necesita unos {} GB de memoria"]).replace("{}", &format!("{gb:.1}"));
        }
        app.set_sheet_info(info.trim_start_matches(" · ").into());
        // where the headline is, for dragging it in the preview
        app.set_page_px(w as f32);
        let plan = sheet.plan();
        let t = &plan.trim;
        app.set_trim_x((t.x / w as f64) as f32);
        app.set_trim_y((t.y / h as f64) as f32);
        app.set_trim_w((t.w / w as f64) as f32);
        app.set_trim_h((t.h / h as f64) as f32);
        match plan.text {
            Some((r, _, _)) => {
                app.set_head_bx((r.x / w as f64) as f32);
                app.set_head_by((r.y / h as f64) as f32);
                app.set_head_bw((r.w / w as f64) as f32);
                app.set_head_bh((r.h / h as f64) as f32);
            }
            None => app.set_head_bw(0.0),
        }
        // zoomed in, the preview is drawn sharper – never finer than the sheet itself, at most 8000 px
        let real = w.max(h) as f64;
        let long = (PREVIEW_PX * app.get_zoom() as f64).min(real).clamp(PREVIEW_PX, 8000.0);
        let mut small = sheet.clone();
        small.dpi = long / (sheet.width_mm.max(sheet.height_mm) / 25.4);
        let (tx, latest) = st.renderer.get_or_init(|| {
            let latest = Arc::new(AtomicUsize::new(0));
            (start_renderer(app.as_weak(), latest.clone()), latest)
        });
        let job = RenderJob { sheet: small, picks, aspect, picture, thumbs: st.thumbs.clone(), generation: latest.fetch_add(1, Ordering::Relaxed) + 1 };
        let _ = tx.send(job);
    });
}

/// One preview to draw; only the newest one (`generation`) is shown.
struct RenderJob {
    sheet: layout::Sheet,
    picks: Vec<Pick>,
    aspect: f64,
    picture: Option<f64>,
    thumbs: Arc<HashMap<Pick, RgbImage>>,
    generation: usize,
}



/// The preview is drawn off the UI thread, so sliders and zoom never wait for it; jobs that piled up while it was
/// drawing are skipped up to the newest.
fn start_renderer(weak: slint::Weak<AppWindow>, latest: Arc<AtomicUsize>) -> std::sync::mpsc::Sender<RenderJob> {
    let (tx, rx) = std::sync::mpsc::channel::<RenderJob>();
    std::thread::spawn(move || {
        // stills already scaled to their cell: a change of font, colour or pattern (cells unchanged) needs none
        // scaled again. Only the tiles of the last picture are kept (zooming would otherwise pile up sets of
        // ~130 MB); forgotten when the stills change
        type Key = (Pick, u32, u32, Option<u64>);
        // the stills these tiles were made from, and the tiles
        let mut from: Option<Arc<HashMap<Pick, RgbImage>>> = None;
        let mut cache: HashMap<Key, RgbImage> = HashMap::new();
        while let Ok(mut job) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }
            if !from.as_ref().is_some_and(|t| Arc::ptr_eq(t, &job.thumbs)) {
                from = Some(job.thumbs.clone());
                cache.clear();
            }
            let picture = job.picture.map(f64::to_bits);
            let (done, used) = (&cache, std::sync::Mutex::new(Vec::new()));
            let page = layout::render_tiles(&job.sheet, job.picks.len(), job.aspect, |i, cw, ch| {
                let key = (job.picks[i], cw, ch, picture);
                let t = match done.get(&key) {
                    Some(t) => t.clone(),
                    None => layout::cover(&cropped(job.thumbs.get(&job.picks[i])?.clone(), job.picture), cw, ch),
                };
                used.lock().unwrap().push((key, t.clone()));
                Some(t)
            });
            cache = used.into_inner().unwrap().into_iter().collect();
            if job.generation != latest.load(Ordering::Relaxed) {
                continue; // a newer one is waiting
            }
            let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(page.as_raw(), page.width(), page.height());
            let (weak, latest) = (weak.clone(), latest.clone());
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade()
                    && job.generation == latest.load(Ordering::Relaxed)
                {
                    app.set_preview(Image::from_rgb8(buffer));
                }
            });
        }
    });
    tx
}

/// Forgets the shots and stills of the previous source.
fn clear_source(app: &AppWindow, st: &mut State) {
    st.stills.clear();
    st.thumbs = Arc::default();
    // a grab or cut detection still running belongs to the old source
    st.gate.cancel.store(true, Ordering::Relaxed);
    st.shots.clear();
    st.timeline = None;
    st.cut = None;
    st.sequences = 0;
    st.still_aspect = None;
    app.set_state(0);
    app.set_source_error("".into());
    app.set_source_info("".into());
    app.set_export_status("".into());
    app.set_can_reveal(false);
    app.set_can_grab(false);
}

/// "12 shots · 25 fps · V1–V3 · starts at 01:00:00:00"
fn describe(t: &Timeline, shots: &[Shot], sequences: usize) -> String {
    let first = shots.iter().map(|s| s.start).min().unwrap_or(0);
    let tracks = shots.iter().map(|s| s.track).max().unwrap_or(0) + 1;
    let v = if tracks > 1 { format!("V1–V{tracks}") } else { "V1".into() };
    if sequences > 1 {
        return tr(["{n} shots from {s} sequences · {r} fps · {v}", "{n} Shots aus {s} Sequenzen · {r} fps · {v}", "{n} plans de {s} séquences · {r} i/s · {v}", "{n} planos de {s} secuencias · {r} fps · {v}"])
            .replace("{n}", &shots.len().to_string())
            .replace("{s}", &sequences.to_string())
            .replace("{r}", &fps_text(t.rate.fps()))
            .replace("{v}", &v);
    }
    tr(["{n} shots · {r} fps · {v} · starts at {tc}", "{n} Shots · {r} fps · {v} · beginnt bei {tc}", "{n} plans · {r} i/s · {v} · commence à {tc}", "{n} planos · {r} fps · {v} · empieza en {tc}"])
        .replace("{n}", &shots.len().to_string())
        .replace("{r}", &fps_text(t.rate.fps()))
        .replace("{v}", &v)
        .replace("{tc}", &format_tc(first, t.rate))
}

/// File name without extension, for pairing a master with its cut list (case ignored).
fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Where a master's cuts come from: its cut list (same file name) or the cuts found in its picture.
enum CutSource {
    List(PathBuf),
    Detected(Vec<i64>),
}

/// Master mode: each master (by index) with its cuts, in the masters' order; a cut list wins over detected cuts.
fn pairs(st: &State) -> Vec<(usize, CutSource)> {
    st.masters
        .iter()
        .enumerate()
        .filter_map(|(i, (m, _))| match st.lists.iter().find(|l| stem(l) == stem(m)) {
            Some(l) => Some((i, CutSource::List(l.clone()))),
            None => st.detected.iter().find(|d| &d.0 == m).map(|d| (i, CutSource::Detected(d.1.clone()))),
        })
        .collect()
}

/// The shots found in a master's picture as a one-track timeline in record frames (the master's timecode).
/// The shots of detected `cuts` (first frame of every shot, then the frame count) at the master's timecode.
fn detected_timeline(master: &Path, cuts: &[i64], info: &frames::VideoInfo) -> Timeline {
    let items = cuts
        .windows(2)
        .enumerate()
        .map(|(k, w)| Item { name: format!("{} {}", tr(["Shot", "Shot", "Plan", "Plano"]), k + 1), start: info.start_tc + w[0], end: info.start_tc + w[1], key: 0 })
        .collect();
    Timeline { name: master.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), rate: info.rate, tracks: vec![Track { items }] }
}

/// Only the film between `from` and `to` (timecode frames; None = no limit): clips the start or end cuts through
/// keep their part inside, the others go.
fn within(mut t: Timeline, from: Option<i64>, to: Option<i64>) -> Timeline {
    let (lo, hi) = (from.unwrap_or(i64::MIN), to.unwrap_or(i64::MAX));
    for track in &mut t.tracks {
        track.items.retain_mut(|it| {
            (it.start, it.end) = (it.start.max(lo), it.end.min(hi));
            it.start < it.end
        });
    }
    t
}

/// The source line in master mode: shots, rate, tracks; one master: where it starts, several: how many.
fn master_text(st: &State) -> String {
    let (Some(t), Some(first)) = (&st.timeline, st.shots.first()) else { return String::new() };
    let pairs = st.sequences.max(1);
    let mut text = if pairs > 1 {
        tr(["{n} shots from {m} master files · {r} fps", "{n} Shots aus {m} Masterfiles · {r} fps", "{n} plans de {m} fichiers master · {r} i/s", "{n} planos de {m} archivos máster · {r} fps"])
            .replace("{n}", &st.shots.len().to_string())
            .replace("{m}", &pairs.to_string())
            .replace("{r}", &fps_text(t.rate.fps()))
    } else {
        let info = &st.masters[first.key.min(st.masters.len() - 1)].1;
        tr(["{} · master starts at {}", "{} · Master beginnt bei {}", "{} · le master commence à {}", "{} · el máster empieza en {}"])
            .replacen("{}", &describe(t, &st.shots, 1), 1)
            .replacen("{}", &format_tc(info.start_tc, info.rate), 1)
    };
    let before = st.shots.iter().filter(|s| st.masters.get(s.key).is_some_and(|m| s.middle() < m.1.start_tc)).count();
    if before > 0 {
        text += &tr([" · {} lie before the master’s start: check the master’s timecode", " · {} liegen vor dem Start des Masters: Timecode des Masters prüfen", " · {} se trouvent avant le début du master : vérifiez son timecode", " · {} quedan antes del inicio del máster: revisa su timecode"]).replace("{}", &before.to_string());
    }
    text
}

/// Master mode: reads the cut list of every master (same file name) and shows what they hold, one after the other.
/// In master mode a shot's `key` is the index of its master.
fn load_source(app: &AppWindow) {
    with_state(app, |st| {
        let mut st = st.borrow_mut();
        clear_source(app, &mut st);
        let pairs = pairs(&st);
        let mut problems: Vec<String> = vec![];
        let names = |v: Vec<&Path>| v.into_iter().map(file_name).collect::<Vec<_>>().join(", ");
        let lone_masters: Vec<&Path> = st.masters.iter().enumerate().filter(|(i, _)| !pairs.iter().any(|p| p.0 == *i)).map(|(_, m)| m.0.as_path()).collect();
        let lone_lists: Vec<&Path> = st.lists.iter().filter(|l| !st.masters.iter().any(|m| stem(&m.0) == stem(l))).map(|l| l.as_path()).collect();
        // only once both kinds were chosen: before that the empty side is simply still to come
        if !st.masters.is_empty() && (!st.lists.is_empty() || !st.detected.is_empty()) {
            if !lone_masters.is_empty() {
                problems.push(tr(["No cut list with the same name for {}", "Keine Schnittliste mit gleichem Namen für {}", "Aucune liste de montage du même nom pour {}", "No hay lista de montaje con el mismo nombre para {}"]).replace("{}", &names(lone_masters)));
            }
            if !lone_lists.is_empty() {
                problems.push(tr(["No master file with the same name for {}", "Kein Masterfile mit gleichem Namen für {}", "Aucun fichier master du même nom pour {}", "No hay archivo máster con el mismo nombre para {}"]).replace("{}", &names(lone_lists)));
            }
        }
        let mut shots = vec![];
        let mut first: Option<Timeline> = None;
        // only the film between start and end (titles stay out), for cut lists and detected cuts alike; changing
        // them needs no new analysis
        let (from, to) = (app.get_detect_from(), app.get_detect_to());
        for (i, source) in &pairs {
            let info = &st.masters[*i].1;
            let read = match source {
                CutSource::List(list) => read_cut_list(list, info.rate).map_err(|e| format!("{}: {e}", file_name(list))),
                CutSource::Detected(cuts) => Ok(detected_timeline(&st.masters[*i].0, cuts, info)),
            };
            let tc = |s: &str| (!s.is_empty()).then(|| cinemosaic::timeline::parse_tc(s, info.rate, false)).flatten();
            let read = read.map(|t| within(t, tc(&from), tc(&to))).and_then(|t| {
                if t.tracks.iter().any(|k| !k.items.is_empty()) {
                    return Ok(t);
                }
                Err(tr(["{}: no picture between start and end (the master starts at {})", "{}: kein Bild zwischen Beginn und Ende (der Master beginnt bei {})", "{} : aucune image entre début et fin (le master commence à {})", "{}: no hay imagen entre inicio y fin (el máster empieza en {})"])
                    .replacen("{}", &file_name(&st.masters[*i].0), 1)
                    .replacen("{}", &format_tc(info.start_tc, info.rate), 1))
            });
            match read {
                Ok(t) => {
                    shots.extend(t.shots().into_iter().map(|mut s| {
                        s.key = *i;
                        s
                    }));
                    first.get_or_insert(t);
                }
                Err(e) => problems.push(e),
            }
        }
        if !pairs.is_empty() && shots.is_empty() && problems.is_empty() {
            problems.push(tr(["The cut list has no video clips.", "Die Schnittliste enthält keine Videoclips.", "La liste de montage ne contient aucun clip vidéo.", "La lista de montaje no tiene clips de vídeo."]));
        }
        app.set_source_error(problems.join(" · ").into());
        if let Some(t) = first.filter(|_| !shots.is_empty()) {
            st.shots = shots;
            st.timeline = Some(t);
            st.sequences = pairs.len();
            app.set_source_info(master_text(&st).into());
            app.set_can_grab(true);
        }
        adopt_carry(app, &mut st);
    });
    refresh(app);
}

/// The master and cut-list lists of the Source tab.
fn show_files(app: &AppWindow) {
    let (m, l): (Vec<SharedString>, Vec<SharedString>) = with_state(app, |st| {
        let st = st.borrow();
        let detected = st.detected.iter().map(|d| {
            tr(["{} – {n} shots detected", "{} – {n} Shots erkannt", "{} – {n} plans détectés", "{} – {n} planos detectados"]).replace("{}", &file_name(&d.0)).replace("{n}", &(d.1.len() - 1).to_string()).into()
        });
        (st.masters.iter().map(|m| file_name(&m.0).into()).collect(), st.lists.iter().map(|l| file_name(l).into()).chain(detected).collect())
    });
    // a master with neither a cut list nor detected cuts
    app.set_can_detect(with_state(app, |st| {
        let st = st.borrow();
        st.masters.iter().any(|(m, _)| !st.lists.iter().any(|l| stem(l) == stem(m)) && !st.detected.iter().any(|d| &d.0 == m))
    }));
    app.set_master_names(ModelRc::new(VecModel::from(m)));
    app.set_list_names(ModelRc::new(VecModel::from(l)));
}

fn time_left(start: Instant, done: usize, total: usize) -> String {
    if done < 3 {
        return tr(["Estimating time left…", "Restzeit wird geschätzt…", "Estimation du temps restant…", "Calculando el tiempo restante…"]);
    }
    let left = start.elapsed().as_secs_f64() / done as f64 * (total - done) as f64;
    if left < 60.0 {
        tr(["Less than a minute left", "Weniger als eine Minute", "Moins d’une minute restante", "Menos de un minuto"])
    } else {
        tr(["About {} min left", "Noch etwa {} min", "Environ {} min restantes", "Quedan unos {} min"]).replace("{}", &(left / 60.0).ceil().to_string())
    }
}

/// Grabs the stills the sheet needs that are not there yet; when none are missing, all of them again.
fn grab(app: &AppWindow) {
    let Ok((sheet, picture)) = read_all(app) else { return };
    let (shots, wanted, fresh, masters, cut, volumes, rate, gate, dir, done) = with_state(app, |st| {
        let mut st = st.borrow_mut();
        let all = st.picks(&sheet, picture);
        let missing: Vec<Pick> = all.iter().filter(|p| !st.stills.contains_key(p)).copied().collect();
        let fresh = missing.is_empty() || st.stills.is_empty();
        if fresh {
            st.stills.clear();
            st.thumbs = Arc::default();
            st.still_aspect = None;
            st.done.clear();
        }
        st.gate.cancel.store(true, Ordering::Relaxed);
        st.gate = Arc::default();
        st.grabs += 1;
        let volumes: Vec<PathBuf> = st.volumes.iter().filter(|v| v.1).map(|v| v.0.clone()).collect();
        // each window its own folder: grabbing everything again empties only its own
        let base = cache_dir().join(format!("window-{}", app.get_win_id()));
        let dir = base.join(format!("grab-{}", st.grabs));
        (st.shots.clone(), if fresh { all } else { missing }, fresh, st.masters.clone(), st.cut.clone(), volumes, st.rate(), st.gate.clone(), (base, dir), st.done.clone())
    });
    let bin_mode = app.get_mode() == 1;
    if (bin_mode && cut.is_none()) || (!bin_mode && masters.is_empty()) {
        return;
    }
    app.set_state(1);
    app.set_paused(false);
    app.set_progress(0.0);
    app.set_work_text(tr(["Looking for the media files…", "Suche die Mediendateien…", "Recherche des fichiers média…", "Buscando los archivos de medios…"]).into());
    app.set_time_left("".into());
    app.set_source_error("".into());
    app.set_export_status("".into());
    app.set_can_reveal(false);
    refresh(app);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let lang = LANGUAGES[LANG.load(Ordering::Relaxed)].0;
        let jobs = match &cut {
            Some(cut) if bin_mode => cut.jobs(&shots, &wanted, &volumes, &|| gate.go(), lang),
            _ if bin_mode => return,
            // every still from its own master (the shot's key)
            _ => {
                let mut jobs: Vec<Option<Result<Job, String>>> = vec![None; wanted.len()];
                for (m, (path, info)) in masters.iter().enumerate() {
                    let mine: Vec<usize> = (0..wanted.len()).filter(|&k| shots[wanted[k].0].key == m).collect();
                    let picks: Vec<Pick> = mine.iter().map(|&k| wanted[k]).collect();
                    for (k, j) in mine.into_iter().zip(master_jobs(&shots, &picks, path, info, lang)) {
                        jobs[k] = Some(j);
                    }
                }
                jobs.into_iter().map(|j| j.unwrap_or_else(|| Err("no master file".into()))).collect()
            }
        };
        if fresh {
            // stills of earlier runs; never this run's folder (an older, cancelled run may still be writing to its own)
            for e in std::fs::read_dir(&dir.0).into_iter().flatten().flatten() {
                if e.path() != dir.1 {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        let start = Instant::now();
        let progress = |done: usize, total: usize| {
            let (weak, left) = (weak.clone(), time_left(start, done, total));
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade() {
                    app.set_progress(done as f32 / total as f32);
                    app.set_work_text(tr(["Still {d} of {t}", "Still {d} von {t}", "Image {d} sur {t}", "Fotograma {d} de {t}"]).replace("{d}", &done.to_string()).replace("{t}", &total.to_string()).into());
                    app.set_time_left(left.into());
                }
            });
        };
        // a frame grabbed before (same media file and frame, e.g. a shot that only moved in the cut) is not grabbed again
        let reuse: Vec<Option<PathBuf>> = jobs
            .iter()
            .map(|j| j.as_ref().ok().and_then(|j| done.get(&(j.file.clone(), j.frame))).filter(|p| p.is_file()).cloned())
            .collect();
        let to_grab: Vec<Result<Job, String>> = jobs.iter().zip(&reuse).filter(|(_, r)| r.is_none()).map(|(j, _)| j.clone()).collect();
        let grabbed = if gate.cancel.load(Ordering::Relaxed) { None } else { grab_stills(&to_grab, &dir.1, &gate, &progress) };
        let stills: Option<Vec<Result<PathBuf, String>>> = grabbed.map(|g| {
            let mut g = g.into_iter();
            reuse.into_iter().map(|r| r.map(Ok).unwrap_or_else(|| g.next().unwrap_or_else(|| Err("not read".into())))).collect()
        });
        let new_done: Vec<((PathBuf, i64), PathBuf)> = stills
            .iter()
            .flatten()
            .zip(&jobs)
            .filter_map(|(r, j)| Some(((j.as_ref().ok()?.file.clone(), j.as_ref().ok()?.frame), r.as_ref().ok()?.clone())))
            .collect();
        let thumbs = stills.as_ref().map(|s| thumbnails(s, &gate));
        let still_aspect = stills.iter().flatten().find_map(|r| r.as_ref().ok().and_then(|p| image::image_dimensions(p).ok())).map(|(w, h)| w as f64 / h as f64);
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            // a newer run took over (new gate): it owns the window, this result is stale
            if !with_state(&app, |st| Arc::ptr_eq(&gate, &st.borrow().gate)) {
                return;
            }
            // cancelled, also by a new source, even if the stills were all there just before
            let result = if gate.cancel.load(Ordering::Relaxed) { (None, None) } else { (stills, thumbs) };
            match result {
                (Some(stills), Some(Some(thumbs))) => {
                    let failed: Vec<String> = wanted
                        .iter()
                        .zip(&stills)
                        .filter_map(|(&(i, part), r)| r.as_ref().err().map(|e| format!("{} {}: {e}", format_tc(shots[i].at(part), rate), shots[i].name)))
                        .collect();
                    if !failed.is_empty() {
                        app.set_source_error(
                            tr(["{n} stills could not be read (grey on the sheet). First: {f}", "{n} Stills konnten nicht gelesen werden (grau auf dem Bogen). Erstes: {f}", "{n} images n’ont pas pu être lues (grises sur la planche). Première : {f}", "No se pudieron leer {n} fotogramas (grises en la hoja). Primero: {f}"])
                                .replace("{n}", &failed.len().to_string())
                                .replace("{f}", &failed[0])
                                .into(),
                        );
                    }
                    with_state(&app, |st| {
                        let mut st = st.borrow_mut();
                        let mut all = (*st.thumbs).clone();
                        all.extend(wanted.iter().zip(thumbs).filter_map(|(p, t)| Some((*p, t?))));
                        st.thumbs = Arc::new(all);
                        st.stills.extend(wanted.iter().copied().zip(stills));
                        st.done.extend(new_done);
                        st.still_aspect = st.still_aspect.or(still_aspect);
                    });
                    app.set_state(2);
                }
                // cancelled: the stills of earlier grabs stay
                _ => app.set_state(if with_state(&app, |st| st.borrow().stills.is_empty()) { 0 } else { 2 }),
            }
            refresh(&app);
        });
    });
}

/// The highest frame number of a timecode in the first master (24 without one).
fn last_frame(app: &AppWindow) -> u32 {
    with_state(app, |st| st.borrow().masters.first().map_or(25, |m| m.1.rate.nominal()) as u32 - 1)
}

/// Finds the cuts of every master that has neither a cut list nor detected cuts (in a thread; pause and cancel work).
fn detect_cuts(app: &AppWindow) {
    let (todo, gate) = with_state(app, |st| {
        let mut st = st.borrow_mut();
        let todo: Vec<(PathBuf, frames::VideoInfo)> = st
            .masters
            .iter()
            .filter(|(m, _)| !st.lists.iter().any(|l| stem(l) == stem(m)) && !st.detected.iter().any(|d| &d.0 == m))
            .cloned()
            .collect();
        st.gate.cancel.store(true, Ordering::Relaxed);
        st.gate = Arc::default();
        (todo, st.gate.clone())
    });
    if todo.is_empty() {
        return;
    }
    app.set_state(1);
    app.set_paused(false);
    app.set_progress(0.0);
    app.set_work_text(tr(["Looking for the cuts…", "Suche die Schnitte…", "Recherche des coupes…", "Buscando los cortes…"]).into());
    app.set_time_left("".into());
    app.set_source_error("".into());
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let start = Instant::now();
        let n = todo.len();
        let mut found = vec![];
        let mut errors = vec![];
        // always the whole film: start and end only choose the shots afterwards (load_source), so they can change
        for (k, (path, info)) in todo.iter().enumerate() {
            let progress = |done: i64, total: i64| {
                let part = if total > 0 { done as f64 / total as f64 } else { 0.0 };
                let share = (k as f64 + part) / n as f64;
                let text = tr(["Looking for the cuts in {} – {p} %", "Suche die Schnitte in {} – {p} %", "Recherche des coupes dans {} – {p} %", "Buscando los cortes en {} – {p} %"])
                    .replace("{}", &file_name(path))
                    .replace("{p}", &((part * 100.0) as i64).to_string());
                let left = time_left(start, (share * 10000.0) as usize, 10000);
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = weak.upgrade() {
                        app.set_progress(share as f32);
                        app.set_work_text(text.into());
                        app.set_time_left(left.into());
                    }
                });
            };
            match frames::detect_cuts(path, info.rate, &|| gate.go(), &progress) {
                Ok(Some(cuts)) if cuts.len() > 1 => found.push((path.clone(), cuts)),
                Ok(Some(_)) => errors.push(format!("{}: {}", file_name(path), tr(["no picture found", "kein Bild gefunden", "aucune image trouvée", "no se encontró imagen"]))),
                Ok(None) => break,
                Err(e) => errors.push(format!("{}: {e}", file_name(path))),
            }
        }
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            // what was found before a cancel is kept
            with_state(&app, |st| st.borrow_mut().detected.extend(found));
            app.set_state(if with_state(&app, |st| st.borrow().stills.is_empty()) { 0 } else { 2 });
            show_files(&app);
            load_source(&app);
            if !errors.is_empty() {
                let shown = app.get_source_error();
                app.set_source_error(errors.into_iter().chain((!shown.is_empty()).then(|| shown.to_string())).collect::<Vec<_>>().join(" · ").into());
            }
        });
    });
}

/// Bin mode: reads the chosen sequences of the bin in a thread (the project's other bins may be needed).
fn load_bin(app: &AppWindow) {
    let (path, gate, chosen, load) = with_state(app, |st| {
        let mut st = st.borrow_mut();
        clear_source(app, &mut st);
        // a reading still running (sequences switched quickly one after the other) stops
        st.gate.cancel.store(true, Ordering::Relaxed);
        st.gate = Arc::default();
        st.bin_loads += 1;
        let chosen: Option<Vec<usize>> =
            if st.seq_on.iter().all(|&on| on) { None } else { Some(st.seq_on.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| i).collect()) };
        (st.bin.clone(), st.gate.clone(), chosen, st.bin_loads)
    });
    let Some(path) = path.filter(|_| chosen.as_ref().is_none_or(|c| !c.is_empty())) else {
        if chosen.is_some_and(|c| c.is_empty()) {
            app.set_source_error(tr(["Choose at least one sequence.", "Wähle mindestens eine Sequenz.", "Choisissez au moins une séquence.", "Elige al menos una secuencia."]).into());
        }
        with_state(app, |st| adopt_carry(app, &mut st.borrow_mut()));
        return refresh(app);
    };
    app.set_state(1);
    app.set_paused(false);
    app.set_progress(0.0);
    app.set_work_text(tr(["Reading the bin…", "Lese den Bin…", "Lecture du chutier…", "Leyendo el bin…"]).into());
    app.set_time_left("".into());
    refresh(app);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let result = avb::Bin::read(&path).and_then(|b| BinCut::open(&path, b, chosen.as_deref(), &|| gate.go()));
        let bin_name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            if with_state(&app, |st| st.borrow().bin_loads) != load {
                return; // a newer reading was started meanwhile
            }
            app.set_state(0);
            match result {
                Ok(cut) => {
                    let shots = cut.shots();
                    // all sequences: one timeline stands for them (rate of the first, the bin's name for the file)
                    let mut timeline = cut.timelines.first().cloned().unwrap_or(Timeline { name: String::new(), rate: cut.rate(), tracks: vec![] });
                    if cut.timelines.len() > 1 {
                        timeline.name = bin_name;
                    }
                    let sequences = cut.timelines.len();
                    if shots.is_empty() {
                        app.set_source_error(tr(["The sequence has no video clips.", "Die Sequenz enthält keine Videoclips.", "La séquence ne contient aucun clip vidéo.", "La secuencia no tiene clips de vídeo."]).into());
                    } else {
                        app.set_source_info(describe(&timeline, &shots, sequences).into());
                        app.set_can_grab(true);
                    }
                    with_state(&app, |st| {
                        let mut st = st.borrow_mut();
                        st.shots = shots;
                        st.timeline = Some(timeline);
                        st.sequences = sequences;
                        st.cut = Some(Arc::new(cut));
                    });
                }
                Err(e) if e == "cancelled" => {}
                Err(e) => app.set_source_error(e.into()),
            }
            with_state(&app, |st| adopt_carry(&app, &mut st.borrow_mut()));
            refresh(&app);
        });
    });
}

/// Volumes with Avid MediaFiles: new ones switched on, the user's choice kept for the others.
fn sync_volumes(app: &AppWindow) {
    with_state(app, |st| {
        let mut st = st.borrow_mut();
        let now = media::volumes();
        let old = std::mem::take(&mut st.volumes);
        st.volumes = now.into_iter().map(|v| {
            let on = old.iter().find(|o| o.0 == v).is_none_or(|o| o.1);
            (v, on)
        }).collect();
        let rows: Vec<VolumeRow> = st
            .volumes
            .iter()
            .map(|(p, on)| VolumeRow { name: file_name(p).into(), on: *on })
            .collect();
        app.set_volumes(ModelRc::new(VecModel::from(rows)));
    });
}

/// Small copies for the preview, 4 at a time; None when cancelled.
fn thumbnails(stills: &[Result<PathBuf, String>], gate: &Gate) -> Option<Vec<Option<RgbImage>>> {
    let chunk = stills.len().div_ceil(4).max(1);
    let parts: Vec<Vec<Option<RgbImage>>> = std::thread::scope(|s| {
        let handles: Vec<_> = stills
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .map(|r| {
                            if !gate.go() {
                                return None;
                            }
                            let img = image::open(r.as_ref().ok()?).ok()?.to_rgb8();
                            let h = (img.height() as f64 * THUMB_W as f64 / img.width() as f64).round().max(1.0) as u32;
                            Some(image::imageops::thumbnail(&img, THUMB_W.min(img.width()), h.min(img.height())))
                        })
                        .collect()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    (!gate.cancel.load(Ordering::Relaxed)).then(|| parts.concat())
}

fn save(app: &AppWindow) {
    let Ok((sheet, picture)) = read_all(app) else { return };
    let format = export::Format::ALL[app.get_export_format() as usize % 4];
    let (stills, aspect, name) = with_state(app, |st| {
        let st = st.borrow();
        let name = st.timeline.as_ref().map(|t| t.name.trim().to_string()).filter(|n| !n.is_empty());
        let name = name.or_else(|| st.lists.first().or(st.bin.as_ref()).and_then(|l| l.file_stem()).map(|s| s.to_string_lossy().into_owned()));
        let stills: Vec<Option<PathBuf>> = st.picks(&sheet, picture).iter().map(|p| st.stills.get(p).and_then(|r| r.as_ref().ok()).cloned()).collect();
        (stills, picture.unwrap_or(st.frame_aspect()), name.unwrap_or_else(|| "CineMosaic".into()))
    });
    // stills the sheet needs but nobody grabbed yet (the sheet or format changed): say so before saving grey cells
    let missing = app.get_missing();
    if missing > 0 {
        let go = tr(["Save Anyway", "Trotzdem speichern", "Enregistrer quand même", "Guardar de todos modos"]);
        let answer = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title(tr(["Not all stills are grabbed", "Noch nicht alle Stills geholt", "Toutes les images ne sont pas récupérées", "Faltan fotogramas por capturar"]))
            .set_description(
                tr(["{} stills of this sheet have not been grabbed yet; they stay grey. “Grab {} missing stills” fetches them.", "{} Stills dieses Bogens sind noch nicht geholt; sie bleiben grau. „{} fehlende Stills holen“ holt sie.", "{} images de cette planche ne sont pas encore récupérées ; elles restent grises. « Récupérer {} images manquantes » les récupère.", "Aún no se han capturado {} fotogramas de esta hoja; quedan en gris. «Capturar {} fotogramas que faltan» los captura."])
                    .replace("{}", &missing.to_string()),
            )
            .set_buttons(rfd::MessageButtons::OkCancelCustom(go.clone(), tr(["Cancel", "Abbrechen", "Annuler", "Cancelar"])))
            .show();
        if !matches!(answer, rfd::MessageDialogResult::Custom(ref s) if *s == go) && answer != rfd::MessageDialogResult::Ok {
            return;
        }
    }
    // "/" and ":" are not allowed in a file name
    let name = name.replace(['/', ':'], "-");
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{name}.{}", format.ext()))
        .add_filter(format.ext().to_uppercase(), &[format.ext()])
        .save_file()
    else {
        return;
    };
    app.set_exporting(true);
    app.set_can_reveal(false);
    app.set_export_status(tr(["Drawing the sheet…", "Zeichne den Bogen…", "Dessin de la planche…", "Dibujando la hoja…"]).into());
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let page = layout::render(&sheet, stills.len(), aspect, |i| stills[i].as_ref().and_then(|p| image::open(p).ok()).map(|i| cropped(i.to_rgb8(), picture)));
        let result = export::save(&page, &path, sheet.dpi, format);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            app.set_exporting(false);
            match result {
                Ok(()) => {
                    app.set_export_status(tr(["Saved: {}", "Gespeichert: {}", "Enregistré : {}", "Guardado: {}"]).replace("{}", &file_name(&path)).into());
                    app.set_can_reveal(true);
                    with_state(&app, |st| st.borrow_mut().saved = Some(path));
                }
                Err(e) => app.set_export_status(tr(["Could not save: {}", "Speichern fehlgeschlagen: {}", "Impossible d’enregistrer : {}", "No se pudo guardar: {}"]).replace("{}", &e).into()),
            }
        });
    });
}

/// After the source of an opened project was read (or could not be): shots that are still the same keep their
/// stills – the others show grey until "Grab N missing stills"; unreadable source = the project as it was saved.
fn adopt_carry(app: &AppWindow, st: &mut State) {
    let Some(c) = st.carry.take() else { return };
    if st.shots.is_empty() {
        let msg = app.get_source_error();
        st.shots = c.shots;
        st.stills = c.stills;
        st.thumbs = Arc::new(c.thumbs);
        st.timeline = Some(c.timeline);
        st.sequences = c.sequences;
        app.set_source_info(c.info.into());
        app.set_can_grab(false);
        let why = tr(["The source could not be read again – the project shows its saved stills; grabbing needs the source.", "Die Quelle ließ sich nicht wieder lesen – das Projekt zeigt seine gesicherten Stills; zum Holen braucht es die Quelle.", "La source n’a pas pu être relue – le projet montre ses images enregistrées ; la récupération demande la source.", "No se pudo volver a leer el origen: el proyecto muestra sus fotogramas guardados; para capturar hace falta el origen."]);
        app.set_source_error(if msg.is_empty() { why } else { format!("{why} ({msg})") }.into());
    } else {
        let mut stills = HashMap::new();
        let mut thumbs = HashMap::new();
        for (j, shot) in st.shots.iter().enumerate() {
            let Some(i) = c.shots.iter().position(|o| o == shot) else { continue };
            for part in [Part::Middle, Part::FirstThird, Part::SecondThird] {
                if let Some(r) = c.stills.get(&(i, part)) {
                    stills.insert((j, part), r.clone());
                }
                if let Some(t) = c.thumbs.get(&(i, part)) {
                    thumbs.insert((j, part), t.clone());
                }
            }
        }
        st.stills = stills;
        st.thumbs = Arc::new(thumbs);
    }
    st.still_aspect = st.still_aspect.or(c.still_aspect);
    if !st.stills.is_empty() {
        app.set_state(2);
    }
    // what is shown now counts as the saved state
    st.project_saved = Some(snapshot(app, st));
}

/// What a project would save, without file places: compared to tell whether there are unsaved changes.
fn snapshot(app: &AppWindow, st: &State) -> serde_json::Value {
    serde_json::json!({
        "look": preset_json(app), "headline": app.get_headline().as_str(), "mode": app.get_mode(),
        "film": [app.get_detect_from().as_str(), app.get_detect_to().as_str()],
        "masters": st.masters.iter().map(|m| m.0.clone()).collect::<Vec<_>>(), "lists": st.lists, "detected": st.detected, "bin": st.bin, "sequence": st.seq_on,
        "shots": st.shots.len(), "stills": st.stills.len(), "read": st.stills.values().filter(|r| r.is_ok()).count(),
    })
}

/// Unsaved work: grabbed stills without a project, or a project changed since saving.
fn unsaved(app: &AppWindow) -> bool {
    with_state(app, |st| {
        let st = st.borrow();
        match &st.project {
            None => !st.stills.is_empty(),
            Some(_) => st.project_saved.as_ref() != Some(&snapshot(app, &st)),
        }
    })
}

/// Before the sheet is replaced or the app quits: Save / Don't Save / Cancel. false = stop.
fn may_discard(app: &AppWindow) -> bool {
    if !unsaved(app) {
        return true;
    }
    let save = tr(["Save", "Sichern", "Enregistrer", "Guardar"]);
    let answer = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(tr(["Unsaved changes", "Ungesicherte Änderungen", "Modifications non enregistrées", "Cambios sin guardar"]))
        .set_description(tr(["Do you want to save the changes to this project?", "Möchtest du die Änderungen an diesem Projekt sichern?", "Voulez-vous enregistrer les modifications de ce projet ?", "¿Quieres guardar los cambios de este proyecto?"]))
        .set_buttons(rfd::MessageButtons::YesNoCancelCustom(save.clone(), tr(["Don’t Save", "Nicht sichern", "Ne pas enregistrer", "No guardar"]), tr(["Cancel", "Abbrechen", "Annuler", "Cancelar"])))
        .show();
    match answer {
        rfd::MessageDialogResult::Custom(s) if s == save => save_project(app, false),
        rfd::MessageDialogResult::Yes => save_project(app, false),
        rfd::MessageDialogResult::Custom(s) if s == tr(["Don’t Save", "Nicht sichern", "Ne pas enregistrer", "No guardar"]) => true,
        rfd::MessageDialogResult::No => true,
        _ => false,
    }
}

fn error_dialog(text: &str) {
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("CineMosaic").set_description(text).show();
}

fn part_order(p: Part) -> u8 {
    match p {
        Part::Middle => 0,
        Part::FirstThird => 1,
        Part::SecondThird => 2,
    }
}

/// Saves the project (asks where for a new one or with `ask`); false = not saved.
// shortcut: copies the stills on the UI thread (instant on the same APFS volume, slower elsewhere); move to a thread
// with progress if saving to other volumes turns out slow
fn save_project(app: &AppWindow, ask: bool) -> bool {
    let current = with_state(app, |st| st.borrow().project.clone());
    let path = match current.filter(|_| !ask) {
        Some(p) => p,
        None => {
            let name = with_state(app, |st| st.borrow().timeline.as_ref().map(|t| t.name.trim().to_string()).filter(|n| !n.is_empty()));
            let name = name.unwrap_or_else(|| "CineMosaic".into()).replace(['/', ':'], "-");
            let Some(p) = rfd::FileDialog::new()
                .set_title(tr(["Save project", "Projekt sichern", "Enregistrer le projet", "Guardar el proyecto"]))
                .set_file_name(format!("{name}.{}", project::EXT))
                .add_filter("CineMosaic", &[project::EXT])
                .save_file()
            else {
                return false;
            };
            if p.extension().is_some_and(|e| e == project::EXT) { p } else { p.with_extension(project::EXT) }
        }
    };
    let p = with_state(app, |st| {
        let st = st.borrow();
        let by_file: HashMap<&PathBuf, &(PathBuf, i64)> = st.done.iter().map(|(job, file)| (file, job)).collect();
        let mut stills: Vec<project::Still> = st
            .stills
            .iter()
            .map(|(&(shot, part), r)| project::Still { shot, part, file: r.clone(), job: r.as_ref().ok().and_then(|f| by_file.get(f)).map(|j| (*j).clone()) })
            .collect();
        stills.sort_by_key(|s| (s.shot, part_order(s.part)));
        let t = st.timeline.clone().unwrap_or(Timeline { name: String::new(), rate: st.rate(), tracks: vec![] });
        project::Project {
            mode: app.get_mode(),
            masters: st.masters.iter().map(|m| m.0.clone()).collect(),
            lists: st.lists.clone(),
            detected: st.detected.clone(),
            bin: st.bin.clone(),
            chosen: (!st.seq_on.iter().all(|&on| on)).then(|| st.sequence_names.iter().zip(&st.seq_on).filter(|(_, on)| **on).map(|(n, _)| n.clone()).collect()),
            volumes_off: st.volumes.iter().filter(|v| !v.1).map(|v| v.0.clone()).collect(),
            look: preset_json(app),
            headline: app.get_headline().to_string(),
            film: (app.get_detect_from().to_string(), app.get_detect_to().to_string()),
            name: t.name,
            rate: t.rate,
            sequences: st.sequences,
            source_info: app.get_source_info().to_string(),
            shots: st.shots.clone(),
            still_aspect: st.still_aspect,
            stills,
        }
    });
    match project::save(&p, &path) {
        Ok(saved) => {
            with_state(app, |st| {
                let mut st = st.borrow_mut();
                // the stills now live in the package (an overwritten package's old files are gone)
                for s in saved {
                    if let (Ok(f), Some(job)) = (&s.file, s.job) {
                        st.done.insert(job, f.clone());
                    }
                    st.stills.insert((s.shot, s.part), s.file);
                }
                st.project = Some(path.clone());
                st.project_saved = Some(snapshot(app, &st));
            });
            log::write(&format!("project saved: {}", file_name(&path)));
            remember_recent(&path);
            show_project_name(app);
            true
        }
        Err(e) => {
            log::write(&format!("project not saved: {e}"));
            error_dialog(&tr(["The project could not be saved: {}", "Das Projekt ließ sich nicht sichern: {}", "Impossible d’enregistrer le projet : {}", "No se pudo guardar el proyecto: {}"]).replace("{}", &e));
            false
        }
    }
}

fn show_project_name(app: &AppWindow) {
    let name = with_state(app, |st| st.borrow().project.as_ref().and_then(|p| p.file_stem()).map(|n| n.to_string_lossy().into_owned()));
    app.set_project_name(name.unwrap_or_default().into());
}

/// Opens a project: its look and source at once, its stills after the small copies are made, then the source is read
/// again (in the background) and the stills of unchanged shots are taken over.
fn open_project(app: &AppWindow, path: PathBuf) {
    if !may_discard(app) {
        return;
    }
    let p = match project::load(&path) {
        Ok(p) => p,
        Err(e) => {
            log::write(&format!("project not opened: {e}"));
            return error_dialog(&tr(["The project could not be opened: {}", "Das Projekt ließ sich nicht öffnen: {}", "Impossible d’ouvrir le projet : {}", "No se pudo abrir el proyecto: {}"]).replace("{}", &e));
        }
    };
    log::write(&format!("project opened: {}", file_name(&path)));
    apply_preset(app, &p.look);
    app.set_headline(p.headline.clone().into());
    app.set_mode(p.mode.clamp(0, 1));
    let name = |o: &Option<PathBuf>| o.as_deref().map(file_name).unwrap_or_default();

    app.set_bin_name(name(&p.bin).into());
    let gate = with_state(app, |st| {
        let mut st = st.borrow_mut();
        clear_source(app, &mut st);
        st.masters.clear();
        st.lists = p.lists.clone();
        st.detected = p.detected.clone();
        app.set_detect_from(p.film.0.clone().into());
        app.set_detect_to(p.film.1.clone().into());
        st.bin = p.bin.clone();
        st.volumes = p.volumes_off.iter().map(|v| (v.clone(), false)).collect();
        st.done = p.stills.iter().filter_map(|s| Some((s.job.clone()?, s.file.clone().ok()?))).collect();
        st.carry = None;
        st.project = Some(path.clone());
        st.project_saved = None;
        st.gate.cancel.store(true, Ordering::Relaxed);
        st.gate = Arc::default();
        st.gate.clone()
    });
    remember_recent(&path);
    show_project_name(app);
    app.set_state(1);
    app.set_progress(0.0);
    app.set_work_text(tr(["Opening the project…", "Öffne das Projekt…", "Ouverture du projet…", "Abriendo el proyecto…"]).into());
    app.set_time_left("".into());
    refresh(app);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let files: Vec<Result<PathBuf, String>> = p.stills.iter().map(|s| s.file.clone()).collect();
        let thumbs = thumbnails(&files, &gate).unwrap_or_default();
        // masters not found drop out (with their shots); their names are said
        let probed: Vec<(PathBuf, Result<frames::VideoInfo, String>)> =
            p.masters.iter().filter(|_| p.mode == 0).map(|m| (m.clone(), frames::probe(m))).collect();
        let names = p.bin.as_ref().filter(|_| p.mode == 1).and_then(|b| avb::Bin::read(b).ok()).map(|b| sequences(&b));
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            app.set_state(0);
            let carry = Carry {
                stills: p.stills.iter().map(|s| ((s.shot, s.part), s.file.clone())).collect(),
                thumbs: p.stills.iter().zip(thumbs).filter_map(|(s, t)| Some(((s.shot, s.part), t?))).collect(),
                shots: p.shots,
                still_aspect: p.still_aspect,
                timeline: Timeline { name: p.name, rate: p.rate, tracks: vec![] },
                sequences: p.sequences,
                info: p.source_info,
            };
            with_state(&app, |st| {
                let mut st = st.borrow_mut();
                st.carry = Some(carry);
                st.masters = probed.iter().filter_map(|(m, i)| Some((m.clone(), i.clone().ok()?))).collect();
            });
            show_files(&app);
            let gone: Vec<String> = probed.iter().filter(|(_, i)| i.is_err()).map(|(m, _)| file_name(m)).collect();
            if let Some(names) = &names {
                // the sequences chosen by name (None = all)
                let on: Vec<bool> = names.iter().map(|n| p.chosen.as_ref().is_none_or(|list| list.contains(n))).collect();
                with_state(&app, |st| {
                    let mut st = st.borrow_mut();
                    st.sequence_names = names.clone();
                    st.seq_on = on;
                });
                show_sequences(&app);
            }
            if p.mode == 1 {
                sync_volumes(&app);
                load_bin(&app);
            } else {
                load_source(&app);
                if !gone.is_empty() {
                    let shown = app.get_source_error();
                    let msg = tr(["Master files not found: {}", "Masterfiles nicht gefunden: {}", "Fichiers master introuvables : {}", "Archivos máster no encontrados: {}"]).replace("{}", &gone.join(", "));
                    app.set_source_error(if shown.is_empty() { msg } else { format!("{msg} · {shown}") }.into());
                }
            }
        });
    });
}

fn recent_path() -> Option<PathBuf> {
    app_dir().map(|d| d.join("recent.txt"))
}

/// Projects opened or saved last, newest first, only those still there.
fn recent() -> Vec<PathBuf> {
    let text = recent_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    text.lines().map(PathBuf::from).filter(|p| p.join("project.json").is_file()).take(10).collect()
}

fn remember_recent(path: &Path) {
    let mut list = recent();
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(10);
    write_recent(&list);
}

fn write_recent(list: &[PathBuf]) {
    if let Some(p) = recent_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, list.iter().map(|p| format!("{}\n", p.display())).collect::<String>());
    }
    all_windows().iter().for_each(|a| show_recent(&a.global::<Shared>()));
}

/// The Open Recent menu of a window.
fn show_recent(shared: &Shared) {
    let names: Vec<SharedString> = recent().iter().map(|p| p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into()).collect();
    shared.set_recent(ModelRc::new(VecModel::from(names)));
}

/// What all windows share: the settings and the settings and manual windows.
struct Ctx {
    settings: Rc<RefCell<Settings>>,
    others: Others,
}

fn set_language(code: &str) {
    let _ = slint::select_bundled_translation(code);
    LANG.store(LANGUAGES.iter().position(|l| l.0 == code).unwrap_or(0), Ordering::Relaxed);
}

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(target_os = "macos")]
    mac_window::srgb_windows();
    let settings = Rc::new(RefCell::new(load_settings()));
    clean_stills(false);
    set_language(&settings.borrow().language);
    log::write(&format!("start {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH));
    std::panic::set_hook(Box::new(|info| log::write(&format!("crash: {info}"))));
    load_presets();
    let ctx = Rc::new(Ctx { settings: settings.clone(), others: Others::default() });
    let app = new_window(&ctx)?;

    // updates: quietly shortly after the start, then every 6 hours
    slint::Timer::single_shot(Duration::from_secs(5), {
        let settings = settings.clone();
        move || front_window().iter().for_each(|a| check_for_update(a, &settings.borrow(), false))
    });
    let update_timer = slint::Timer::default();
    update_timer.start(slint::TimerMode::Repeated, Duration::from_secs(6 * 3600), {
        let settings = settings.clone();
        move || front_window().iter().for_each(|a| check_for_update(a, &settings.borrow(), false))
    });
    // volumes come and go: look every 2 s in the windows that show the bin mode
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, Duration::from_secs(2), || {
        for a in all_windows().iter().filter(|a| a.get_mode() == 1) {
            sync_volumes(a);
        }
    });
    #[cfg(target_os = "macos")]
    {
        let c = ctx.clone();
        mac_app::install(
            Box::new(move |p| {
                let c = c.clone();
                slint::Timer::single_shot(Duration::ZERO, move || open_project_in(&c, front_window(), p.clone()));
            }),
            // Quit: every window with unsaved changes asks; Cancel in one stops quitting
            Box::new(|| all_windows().iter().all(|a| {
                if unsaved(a) {
                    let _ = a.show();
                }
                may_discard(a)
            })),
        );
        mac_menu::install(
            Box::new(|| front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_show_about())),
            Box::new(|| front_window().into_iter().for_each(|a| a.global::<Shared>().invoke_show_settings())),
            &app.get_txt_settings_menu(),
        );
    }
    drop(app);
    // title bars follow the app's light/dark; the settings tab bar follows the title bar (lighter while active)
    #[cfg(target_os = "macos")]
    let chrome = slint::Timer::default();
    #[cfg(target_os = "macos")]
    chrome.start(slint::TimerMode::Repeated, Duration::from_millis(200), {
        let (win, settings) = (ctx.others.settings.clone(), settings.clone());
        let fitted = RefCell::new(std::collections::HashSet::new());
        move || {
            mac_window::appearance_all(settings.borrow().dark);
            for a in all_windows() {
                if mac_window::is_key(a.window()) {
                    FRONT.with(|f| f.set(a.get_win_id()));
                }
                // never taller than the screen minus the title bar (small laptops)
                let (screen_w, screen) = mac_window::screen_size();
                if screen > 0.0 && ((screen - 28.0).min(820.0) - a.get_smallest_height() as f64).abs() > 0.5 {
                    a.set_smallest_height((screen - 28.0).min(820.0) as f32);
                }
                // the standard size is made for a large screen: on a smaller one the window fits it, once
                if screen > 0.0 && fitted.borrow_mut().insert(a.get_win_id()) {
                    let size = a.window().size().to_logical(a.window().scale_factor());
                    let (w, h) = ((size.width as f64).min(screen_w), (size.height as f64).min(screen - 28.0));
                    if w < size.width as f64 - 0.5 || h < size.height as f64 - 0.5 {
                        a.window().set_size(slint::LogicalSize::new(w as f32, h as f32));
                    }
                }
                // a window can grow, never shrink below what the side column needs (also when dragged)
                mac_window::set_min_size(a.window(), a.get_smallest_width() as f64, a.get_smallest_height() as f64);
            }
            if let Some(w) = win.borrow().as_ref() {
                // the NSWindow exists only once the event loop has run after show(); only changes it once
                mac_window::unify_titlebar(w.window(), 800.0, 450.0);
            }
        }
    });
    #[cfg(target_os = "macos")]
    mac_window::on_key_change({
        let win = ctx.others.settings.clone();
        Box::new(move || {
            // called from inside AppKit (also while show() holds the window's RefCell): update on the next turn
            let win = win.clone();
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let Some(w) = win.borrow().as_ref() {
                    let key = mac_window::is_key(w.window());
                    if w.get_active() != key {
                        w.set_active(key);
                    }
                }
            });
        })
    });

    load_fonts();
    let result = slint::run_event_loop();
    // the stills are only needed while the app is open
    clean_stills(true);
    result
}

/// A new project window, a little below and right of the one in front.
fn new_window(ctx: &Rc<Ctx>) -> Result<AppWindow, slint::PlatformError> {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let app = AppWindow::new()?;
    // Slint takes the language only once a window exists (before the first one it is ignored)
    set_language(&ctx.settings.borrow().language);
    app.set_win_id(NEXT.fetch_add(1, Ordering::Relaxed) as i32);
    repaint_on_resize!(app);
    let from = front_window();
    WINDOWS.with(|w| w.borrow_mut().push(app.clone_strong()));
    FRONT.with(|f| f.set(app.get_win_id()));
    wire_shared(&app, ctx);
    wire_window(&app, ctx);
    show_palettes(&app, &ctx.settings.borrow());
    match_slots(&app);
    show_fonts(&app);
    preset_menu(&app);
    picture_menu(&app);
    paper_menu(&app);
    show_picture(&app);
    refresh(&app);
    app.show()?;
    if let Some(f) = from {
        let (p, k) = (f.window().position(), f.window().scale_factor());
        app.window().set_position(slint::PhysicalPosition::new(p.x + (28.0 * k) as i32, p.y + (28.0 * k) as i32));
    }
    Ok(app)
}

/// Closes a project window after asking about unsaved changes; its state and its stills folder go with it.
fn close_window(app: &AppWindow) {
    if !may_discard(app) {
        return;
    }
    let id = app.get_win_id();
    log::write("window closed");
    let _ = app.hide();
    WINDOWS.with(|w| w.borrow_mut().retain(|a| a.get_win_id() != id));
    STATES.with(|s| s.borrow_mut().remove(&id));
    let _ = std::fs::remove_dir_all(cache_dir().join(format!("window-{id}")));
}

/// A window shows nothing yet: no project, no source, no stills – an opened project can take it over.
fn pristine(app: &AppWindow) -> bool {
    with_state(app, |st| {
        let st = st.borrow();
        st.project.is_none() && st.masters.is_empty() && st.lists.is_empty() && st.detected.is_empty() && st.bin.is_none() && st.stills.is_empty()
    })
}

/// Opens a project in its own window: the window it is already open in comes to the front; an empty window `from`
/// takes it, else a new window opens.
fn open_project_in(ctx: &Rc<Ctx>, from: Option<AppWindow>, path: PathBuf) {
    let same = |p: &Path| p.canonicalize().ok() == path.canonicalize().ok();
    if let Some(w) = all_windows().into_iter().find(|w| with_state(w, |st| st.borrow().project.as_deref().is_some_and(same))) {
        let _ = w.show();
        FRONT.with(|f| f.set(w.get_win_id()));
        return;
    }
    let target = match from.filter(pristine) {
        Some(w) => Some(w),
        None => new_window(ctx).map_err(|e| log::write(&format!("no new window: {e}"))).ok(),
    };
    if let Some(t) = target {
        open_project(&t, path);
    }
}

/// Settings… opens the tab chosen last (never About); About opens the same window on the About tab.
fn open_settings(ctx: &Rc<Ctx>, about: bool) {
    let ctx = ctx.clone();
    slint::Timer::single_shot(Duration::ZERO, move || {
        let settings = &ctx.settings;
        let mut win = ctx.others.settings.borrow_mut();
        if win.is_none() {
            *win = SettingsWindow::new().ok();
            win.iter().for_each(|w| {
                repaint_on_resize!(w);
                forward_menu(&w.global::<Shared>());
                wire_support(w, settings);
                w.set_version(env!("CARGO_PKG_VERSION").into());
                w.set_last_tab(settings.borrow().tab);
                w.on_tab_chosen({
                    let settings = settings.clone();
                    move |tab| {
                        settings.borrow_mut().tab = tab;
                        save_settings(&settings.borrow());
                    }
                });
                w.on_open_source(|| {
                    let _ = std::process::Command::new("open").arg(format!("https://github.com/{}", UPDATE_APP.repo)).spawn();
                });
                w.on_open_license(|| {
                    // bundled license text, otherwise the FSF page
                    let bundled = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.parent()?.join("Resources/LICENSE.txt")));
                    let target = bundled.filter(|p| p.exists()).map_or("https://www.gnu.org/licenses/gpl-3.0.html".into(), |p| p.display().to_string());
                    let _ = std::process::Command::new("open").arg(target).spawn();
                });
            });
        }
        if let Some(w) = win.as_ref() {
            apply_menus(&w.global::<Shared>(), &settings.borrow());
            w.set_tab(if about { 3 } else { w.get_last_tab() });
            let _ = w.show();
        }
    });
}

/// The app's menus in a project window: language, appearance, updates, settings, manual, log.
// Menu actions run after the click: they rename menu items, which crashed during macOS menu handling (seen in Ferry).
fn wire_shared(app: &AppWindow, ctx: &Rc<Ctx>) {
    let shared = app.global::<Shared>();
    apply_menus(&shared, &ctx.settings.borrow());
    let c = ctx.clone();
    shared.on_select_language(move |i| {
        let c = c.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            let code = LANGUAGES.get(i as usize).map_or("en", |l| l.0);
            set_language(code);
            c.settings.borrow_mut().language = code.into();
            save_settings(&c.settings.borrow());
            apply_all(&c.others, &c.settings.borrow());
            // texts Rust wrote (source line, sheet line) follow the language
            all_windows().iter().for_each(load_texts_again);
        });
    });
    let c = ctx.clone();
    shared.on_select_dark(move |dark| {
        let c = c.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            c.settings.borrow_mut().dark = dark;
            save_settings(&c.settings.borrow());
            apply_all(&c.others, &c.settings.borrow());
        });
    });
    // "Get test versions": the old result no longer applies – hide "up to date" and check again quietly
    let c = ctx.clone();
    shared.on_select_betas(move |betas| {
        let c = c.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            c.settings.borrow_mut().betas = betas;
            save_settings(&c.settings.borrow());
            {
                let s = c.settings.borrow();
                let mut u = s.update.lock().unwrap();
                if u.state == 2 || u.state == 4 {
                    u.state = 0;
                }
            }
            apply_all(&c.others, &c.settings.borrow());
            front_window().iter().for_each(|a| check_for_update(a, &c.settings.borrow(), false));
        });
    });
    let c = ctx.clone();
    shared.on_show_settings(move || open_settings(&c, false));
    let c = ctx.clone();
    shared.on_show_about(move || open_settings(&c, true));
    let c = ctx.clone();
    shared.on_refresh_update(move || apply_all(&c.others, &c.settings.borrow()));
    let (weak, c) = (app.as_weak(), ctx.clone());
    shared.on_check_update(move || weak.upgrade().iter().for_each(|a| check_for_update(a, &c.settings.borrow(), true)));
    let (weak, c) = (app.as_weak(), ctx.clone());
    shared.on_install_update(move || weak.upgrade().iter().for_each(|a| install_update(a, &c.settings.borrow())));
    let c = ctx.clone();
    shared.on_show_release(move || {
        if let Some(r) = c.settings.borrow().update.lock().unwrap().available.as_ref() {
            let _ = std::process::Command::new("open").arg(&r.page).spawn();
        }
    });
    shared.on_show_log(|| {
        log::write("log file shown");
        let _ = std::process::Command::new("open").arg("-R").arg(log::path()).spawn();
    });
    let c = ctx.clone();
    shared.on_show_help(move || {
        let c = c.clone();
        slint::Timer::single_shot(Duration::ZERO, move || {
            let mut help = c.others.help.borrow_mut();
            if help.is_none() {
                *help = HelpWindow::new().ok();
                help.iter().for_each(|h| {
                    repaint_on_resize!(h);
                    forward_menu(&h.global::<Shared>())
                });
            }
            if let Some(h) = help.as_ref() {
                update_help(h, &c.settings.borrow());
                let _ = h.show();
            }
        });
    });
}

/// Source tab: master files, cut lists, cut detection and its film range, bin, sequences, volumes; grabbing.
fn wire_source(app: &AppWindow) {
    // master files and cut lists are added (several at a time); the × removes one
    app.on_pick_master({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(files) = rfd::FileDialog::new().set_title(tr(["Master files", "Masterfiles", "Fichiers master", "Archivos máster"])).pick_files() else { return };
            let mut errors = vec![];
            for p in files {
                if with_state(&app, |st| st.borrow().masters.iter().any(|m| m.0 == p)) {
                    continue;
                }
                match frames::probe(&p) {
                    Ok(info) => with_state(&app, |st| st.borrow_mut().masters.push((p, info))),
                    Err(e) => errors.push(format!("{}: {e}", file_name(&p))),
                }
            }
            show_files(&app);
            load_source(&app);
            if !errors.is_empty() {
                let shown = app.get_source_error();
                app.set_source_error(errors.into_iter().chain((!shown.is_empty()).then(|| shown.to_string())).collect::<Vec<_>>().join(" · ").into());
            }
        }
    });
    app.on_pick_list({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(files) = rfd::FileDialog::new().set_title(tr(["Cut lists", "Schnittlisten", "Listes de montage", "Listas de montaje"])).add_filter("EDL, XML, FCPXML, AAF", &["edl", "xml", "fcpxml", "aaf"]).pick_files() else {
                return;
            };
            with_state(&app, |st| {
                let mut st = st.borrow_mut();
                for p in files {
                    if !st.lists.contains(&p) {
                        st.lists.push(p);
                    }
                }
            });
            show_files(&app);
            load_source(&app);
        }
    });
    app.on_tc_edit({
        let weak = app.as_weak();
        move |st, key, meta| match weak.upgrade() {
            Some(app) => tc_edit(st, &key, meta, last_frame(&app)),
            None => st,
        }
    });
    app.on_tc_paste({
        let weak = app.as_weak();
        move |st| match (weak.upgrade(), clipboard_text()) {
            (Some(app), Some(clip)) => tc_pasted(st, &clip, last_frame(&app)),
            _ => st,
        }
    });
    app.on_tc_parts(|text| {
        let v: Vec<SharedString> = tc_parts(&text).iter().map(|v| format!("{v:02}").into()).collect();
        ModelRc::new(VecModel::from(v))
    });
    // start or end of the film changed: the detected shots follow at once
    app.on_film_changed({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            if app.get_mode() == 0 && with_state(&app, |st| !st.borrow().masters.is_empty()) {
                load_source(&app);
            }
        }
    });
    // masters without a cut list: the cuts are looked for in the picture, one master after the other (whole files)
    app.on_detect_cuts({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(detect_cuts)
    });
    app.on_remove_master({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            with_state(&app, |st| {
                let mut st = st.borrow_mut();
                if (i as usize) < st.masters.len() {
                    let (m, _) = st.masters.remove(i as usize);
                    st.detected.retain(|d| d.0 != m);
                }
            });
            show_files(&app);
            load_source(&app);
        }
    });
    app.on_remove_list({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            with_state(&app, |st| {
                let mut st = st.borrow_mut();
                let i = i as usize;
                if i < st.lists.len() {
                    st.lists.remove(i);
                } else if i - st.lists.len() < st.detected.len() {
                    let k = i - st.lists.len();
                    st.detected.remove(k);
                }
            });
            show_files(&app);
            load_source(&app);
        }
    });
    app.on_mode_chosen({
        let weak = app.as_weak();
        move |_| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_mode() == 1 {
                sync_volumes(&app);
                load_bin(&app);
            } else {
                load_source(&app);
            }
        }
    });
    app.on_pick_bin({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(p) = rfd::FileDialog::new().set_title(tr(["Media Composer bin", "Media-Composer-Bin", "Chutier Media Composer", "Bin de Media Composer"])).add_filter("Media Composer bin", &["avb"]).pick_file() else { return };
            let names = match avb::Bin::read(&p) {
                Ok(b) => sequences(&b),
                Err(e) => {
                    app.set_source_error(e.into());
                    return;
                }
            };
            app.set_bin_name(file_name(&p).into());
            with_state(&app, |st| {
                let mut st = st.borrow_mut();
                st.seq_on = vec![true; names.len()];
                st.sequence_names = names.clone();
                st.bin = Some(p);
            });
            show_sequences(&app);
            if names.is_empty() {
                with_state(&app, |st| clear_source(&app, &mut st.borrow_mut()));
                app.set_source_error(tr(["The bin holds no sequence.", "Der Bin enthält keine Sequenz.", "Le chutier ne contient aucune séquence.", "El bin no contiene ninguna secuencia."]).into());
                return refresh(&app);
            }
            sync_volumes(&app);
            load_bin(&app);
        }
    });
    // 0 = All sequences; 1.. a sequence: while all are shown it becomes the only one, else it is ticked or unticked
    // (unticking the last one goes back to All sequences)
    app.on_toggle_sequence({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            with_state(&app, |st| {
                let mut st = st.borrow_mut();
                let every = st.seq_on.iter().all(|&on| on);
                match (i as usize).checked_sub(1) {
                    None => st.seq_on.iter_mut().for_each(|on| *on = true),
                    Some(k) if every => st.seq_on.iter_mut().enumerate().for_each(|(j, on)| *on = j == k),
                    Some(k) => {
                        if let Some(on) = st.seq_on.get_mut(k) {
                            *on = !*on;
                        }
                        if st.seq_on.iter().all(|&on| !on) {
                            st.seq_on.iter_mut().for_each(|on| *on = true);
                        }
                    }
                }
            });
            show_sequences(&app);
            load_bin(&app);
        }
    });
    app.on_toggle_volume({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            with_state(&app, |st| {
                if let Some(v) = st.borrow_mut().volumes.get_mut(i as usize) {
                    v.1 = !v.1;
                }
            });
            sync_volumes(&app);
        }
    });
    app.on_grab({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(grab)
    });
    app.on_pause({
        let weak = app.as_weak();
        move |on| {
            let Some(app) = weak.upgrade() else { return };
            with_state(&app, |st| st.borrow().gate.pause.store(on, Ordering::Relaxed));
            app.set_paused(on);
        }
    });
    app.on_cancel({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            with_state(&app, |st| st.borrow().gate.cancel.store(true, Ordering::Relaxed));
        }
    });
}

/// Sheet tab and preview: picture format, paper, unit, size, zoom, redraw; export and Show in Finder.
fn wire_sheet(app: &AppWindow) {
    app.on_picture_chosen({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.set_picture(i);
            show_picture(&app);
            refresh(&app);
        }
    });
    app.on_picture_edited({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(picture_edited)
    });
    app.on_paper_chosen({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.set_paper(i);
            show_paper(&app);
            refresh(&app);
        }
    });
    app.on_orientation_chosen({
        let weak = app.as_weak();
        move |landscape| {
            let Some(app) = weak.upgrade() else { return };
            app.set_landscape(landscape);
            let (w, h) = (app.get_custom_w(), app.get_custom_h());
            if let (Ok(a), Ok(b)) = (whole(&w, ""), whole(&h, ""))
                && a != b
                && (a > b) != landscape
            {
                app.set_custom_w(h);
                app.set_custom_h(w);
            }
            // a turned social media size is another one or Custom
            size_edited(&app);
        }
    });
    app.on_unit_chosen({
        let weak = app.as_weak();
        move |px| {
            let Some(app) = weak.upgrade() else { return };
            switch_unit(&app, px);
            refresh(&app);
        }
    });
    app.on_size_edited({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(size_edited)
    });
    // zoom: draw a sharper preview once the zoom has rested for a moment
    let zoom_timer = Rc::new(slint::Timer::default());
    app.on_zoom_changed({
        let (weak, timer) = (app.as_weak(), zoom_timer.clone());
        move |_| {
            let weak = weak.clone();
            timer.start(slint::TimerMode::SingleShot, Duration::from_millis(180), move || weak.upgrade().iter().for_each(refresh));
        }
    });
    app.on_sheet_changed({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(refresh)
    });
    app.on_export({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(save)
    });
    app.on_reveal({
        let weak = app.as_weak();
        move || {
        let Some(app) = weak.upgrade() else { return };
        if let Some(p) = with_state(&app, |st| st.borrow().saved.clone()) {
            let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
        }
        }
    });
}

/// Frame line and Headline tabs: colour presets and the colour panel, anchor, font list.
fn wire_design(app: &AppWindow, ctx: &Rc<Ctx>) {
    // a colour preset clicked: its colour
    app.on_color_swatch({
        let weak = app.as_weak();
        move |k, i| {
            let Some(app) = weak.upgrade() else { return };
            let pal = [app.get_line_palette(), app.get_head_palette(), app.get_bg_palette()][k.clamp(0, 2) as usize].clone();
            let Some(sw) = slint::Model::row_data(&pal, i as usize) else { return };
            [AppWindow::set_line_slot, AppWindow::set_head_slot, AppWindow::set_bg_slot][k.clamp(0, 2) as usize](&app, i);
            set_color(&app, k, sw.hex);
            refresh(&app);
        }
    });
    // a click into the hex field: the colour panel; what is picked there becomes the chosen preset (none chosen:
    // only the colour)
    app.on_color_panel({
        let (weak, ctx) = (app.as_weak(), ctx.clone());
        move |k| {
            let Some(app) = weak.upgrade() else { return };
            let value = color_of(&app, k);
            let (weak, ctx, saver) = (weak.clone(), ctx.clone(), ctx.clone());
            color_panel::open(
                hex_color(&value).unwrap_or([0, 0, 0]),
                Rc::new(move |c| {
                    let Some(app) = weak.upgrade() else { return };
                    if let Ok(slot) = usize::try_from(slot_of(&app, k)) {
                        set_palette_entry(&ctx.settings, k as usize, slot, c, false);
                    }
                    set_color(&app, k, hex_text(c).into());
                    refresh(&app);
                }),
                Rc::new(move || save_settings(&saver.settings.borrow())),
            );
        }
    });
    // a hex value typed: the chosen preset takes it
    app.on_color_typed({
        let (weak, ctx) = (app.as_weak(), ctx.clone());
        move |k| {
            let Some(app) = weak.upgrade() else { return };
            if let (Some(c), Ok(slot)) = (hex_color(&color_of(&app, k)), usize::try_from(slot_of(&app, k))) {
                set_palette_entry(&ctx.settings, k as usize, slot, c, true);
            }
        }
    });
    // headline anchor: a sensible move for the new point (inside the sheet), then draw
    app.on_anchor_chosen({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.set_head_anchor(i);
            let per = mm_per_px(&app);
            app.set_head_dx(in_unit([10.0, 0.0, -10.0][i as usize % 3], per));
            app.set_head_dy(in_unit([8.0, 0.0, -8.0][(i as usize / 3).min(2)], per));
            refresh(&app);
        }
    });
    // a letter in the font list: the next family starting with it (after the current one, wrapping round)
    app.on_jump_font({
        let weak = app.as_weak();
        move |text| {
            let Some(app) = weak.upgrade() else { return -1 };
            let key = text.to_lowercase();
            FONTS.with(|f| {
                let f = f.borrow();
                let start = app.get_font().max(0) as usize + 1;
                (0..f.len()).map(|k| (start + k) % f.len()).find(|&k| f[k].to_lowercase().starts_with(&key)).map_or(-1, |k| k as i32)
            })
        }
    });
}

/// Presets: choose, new, update, rename, delete.
fn wire_presets(app: &AppWindow) {
    app.on_preset_chosen({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            let Some((name, values)) = PRESETS.with(|p| p.borrow().get(i as usize).cloned()) else { return };
            apply_preset(&app, &values);
            app.set_preset(i);
            app.set_preset_status(SharedString::new());
            log::write(&format!("preset chosen: {name}"));
            refresh(&app);
        }
    });
    app.on_preset_update({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let values = preset_json(&app);
            let i = app.get_preset() as usize;
            let Some(name) = PRESETS.with(|p| p.borrow_mut().get_mut(i).map(|e| {
                e.1 = values;
                e.0.clone()
            })) else { return };
            store_presets(&app, tr(["“{}” now has the current settings.", "„{}“ hat jetzt die aktuellen Einstellungen.", "« {} » a maintenant les réglages actuels.", "«{}» tiene ahora los ajustes actuales."]).replacen("{}", &name, 1));
            log::write(&format!("preset updated: {name}"));
        }
    });
    app.on_preset_commit({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let name: String = app.get_preset_draft().trim().chars().take(80).collect();
            if name.is_empty() {
                return;
            }
            let renaming = app.get_preset_edit() == 2;
            let current = app.get_preset() as usize;
            let taken = PRESETS.with(|p| p.borrow().iter().enumerate().any(|(i, (n, _))| n.eq_ignore_ascii_case(&name) && !(renaming && i == current)));
            if taken {
                app.set_preset_status(tr(["A preset with this name already exists.", "Ein Preset mit diesem Namen gibt es schon.", "Un préréglage porte déjà ce nom.", "Ya existe un preajuste con este nombre."]).into());
                return;
            }
            let values = preset_json(&app);
            PRESETS.with(|p| {
                let mut p = p.borrow_mut();
                if renaming {
                    if let Some(e) = p.get_mut(current) {
                        log::write(&format!("preset renamed: {} -> {name}", e.0));
                        e.0 = name.clone();
                    }
                } else {
                    log::write(&format!("preset saved: {name}"));
                    p.push((name.clone(), values));
                }
                p.sort_by_key(|e| e.0.to_lowercase());
            });
            let i = PRESETS.with(|p| p.borrow().iter().position(|e| e.0 == name)).map_or(-1, |i| i as i32);
            app.set_preset(i);
            app.set_preset_edit(0);
            store_presets(&app, String::new());
        }
    });
    app.on_preset_delete({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let i = app.get_preset() as usize;
            if let Some((name, _)) = PRESETS.with(|p| {
                let mut p = p.borrow_mut();
                (i < p.len()).then(|| p.remove(i))
            }) {
                log::write(&format!("preset deleted: {name}"));
            }
            app.set_preset(-1);
            app.set_preset_edit(0);
            store_presets(&app, String::new());
        }
    });
}

/// One project window: its tabs, presets and the File menu.
fn wire_window(app: &AppWindow, ctx: &Rc<Ctx>) {
    wire_source(app);
    wire_sheet(app);
    wire_design(app, ctx);
    wire_presets(app);
    // File menu: run after the click (dialogs during macOS menu handling are asking for trouble, see above)
    let shared = app.global::<Shared>();
    let later = |f: Rc<dyn Fn(&AppWindow)>, weak: slint::Weak<AppWindow>| {
        move || {
            let (weak, f) = (weak.clone(), f.clone());
            slint::Timer::single_shot(Duration::ZERO, move || weak.upgrade().iter().for_each(|a| f(a)));
        }
    };
    let c = ctx.clone();
    shared.on_new_project(later(
        Rc::new(move |_| {
            if let Err(e) = new_window(&c) {
                log::write(&format!("no new window: {e}"));
            }
        }),
        app.as_weak(),
    ));
    shared.on_save_project(later(Rc::new(|a| { save_project(a, false); }), app.as_weak()));
    shared.on_save_project_as(later(Rc::new(|a| { save_project(a, true); }), app.as_weak()));
    let c = ctx.clone();
    shared.on_open_project(later(
        Rc::new(move |a| {
            if let Some(p) = rfd::FileDialog::new().set_title(tr(["Open project", "Projekt öffnen", "Ouvrir un projet", "Abrir proyecto"])).add_filter("CineMosaic", &[project::EXT]).pick_file() {
                open_project_in(&c, Some(a.clone_strong()), p);
            }
        }),
        app.as_weak(),
    ));
    let (weak, c) = (app.as_weak(), ctx.clone());
    shared.on_open_recent(move |i| {
        let (weak, c) = (weak.clone(), c.clone());
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(p) = recent().get(i as usize).cloned() {
                open_project_in(&c, weak.upgrade(), p);
            }
        });
    });
    shared.on_clear_recent(|| write_recent(&[]));
    shared.on_close_window(later(Rc::new(close_window), app.as_weak()));
    // the red button: same as Close Window, but the window must stay until the answer
    app.window().on_close_requested({
        let weak = app.as_weak();
        move || {
            let weak = weak.clone();
            slint::Timer::single_shot(Duration::ZERO, move || weak.upgrade().iter().for_each(close_window));
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
}

/// The sequence menu: "All sequences" ticked, or the chosen sequences ticked; the button names the choice.
fn show_sequences(app: &AppWindow) {
    let all = tr(["All sequences", "Alle Sequenzen", "Toutes les séquences", "Todas las secuencias"]);
    let (rows, label) = with_state(app, |st| {
        let st = st.borrow();
        let every = st.seq_on.iter().all(|&on| on);
        let rows: Vec<MenuChoice> = std::iter::once(MenuChoice { title: all.as_str().into(), checked: every })
            .chain(st.sequence_names.iter().zip(&st.seq_on).map(|(n, on)| MenuChoice { title: n.into(), checked: *on && !every }))
            .collect();
        let chosen: Vec<&String> = st.sequence_names.iter().zip(&st.seq_on).filter(|(_, on)| **on).map(|(n, _)| n).collect();
        let label = match chosen.len() {
            _ if every => all.clone(),
            1 => chosen[0].clone(),
            n => tr(["{} sequences", "{} Sequenzen", "{} séquences", "{} secuencias"]).replace("{}", &n.to_string()),
        };
        (rows, label)
    });
    app.set_sequences(ModelRc::new(VecModel::from(if rows.len() > 1 { rows } else { vec![] })));
    app.set_sequence_label(label.into());
}

/// After a language change: the lines Rust wrote are written again in the new language.
fn load_texts_again(app: &AppWindow) {
    picture_menu(app);
    paper_menu(app);
    show_sequences(app);
    with_state(app, |st| {
        let st = st.borrow();
        if let Some(t) = &st.timeline
            && !st.shots.is_empty()
            && app.get_source_info() != ""
        {
            let text = if app.get_mode() == 0 { master_text(&st) } else { describe(t, &st.shots, st.sequences) };
            app.set_source_info(text.into());
        }
    });
    refresh(app);
}

#[cfg(test)]
mod tests {
    #[test]
    fn film_start_and_end_cut_the_clips() {
        use super::{Item, Rate, Timeline, Track, within};
        let it = |start, end| Item { name: String::new(), start, end, key: 0 };
        let t = Timeline { name: String::new(), rate: Rate { num: 25, den: 1 }, tracks: vec![Track { items: vec![it(0, 100), it(100, 200), it(200, 300)] }] };
        let cut = within(t.clone(), Some(150), Some(250));
        assert_eq!(cut.tracks[0].items.iter().map(|i| (i.start, i.end)).collect::<Vec<_>>(), vec![(150, 200), (200, 250)]);
        assert_eq!(within(t.clone(), None, None).tracks[0].items.len(), 3);
        assert!(within(t, Some(400), None).tracks[0].items.is_empty());
    }

    #[test]
    fn long_names_are_shortened_in_the_middle() {
        assert_eq!(super::shorten("Cut.edl", 10), "Cut.edl");
        assert_eq!(super::shorten("/Volumes/Archive/Projects/Avid Projects", 21), "/Volumes/A…d Projects");
    }

    #[test]
    fn help_is_split_at_headings_in_every_language() {
        for l in super::LANGUAGES {
            let (subtitle, blocks) = super::help_blocks(l.2);
            assert!(!subtitle.is_empty(), "{}", l.0);
            assert!(blocks.iter().filter(|b| !b.heading.is_empty()).count() >= 8, "{}", l.0);
            assert!(!blocks[0].heading.is_empty(), "{}: starts with a heading", l.0);
        }
        // every language has the same sections
        let count = |m: &str| m.lines().filter(|l| l.starts_with("## ")).count();
        assert!(super::LANGUAGES.iter().all(|l| count(l.2) == count(super::LANGUAGES[0].2)));
    }

    #[test]
    fn rust_texts_follow_the_language() {
        use std::sync::atomic::Ordering;
        super::LANG.store(1, Ordering::Relaxed);
        assert_eq!(super::tr(["Paper", "Papier", "Papier", "Papel"]), "Papier");
        super::LANG.store(0, Ordering::Relaxed);
    }
}
