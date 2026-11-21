//! CineMosaic window: choose master + cut list, grab the stills, set up the sheet with a live preview, save it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

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
use cinemosaic::timeline::{Pick, Rate, Shot, Timeline, format_tc};
use cinemosaic::report::{self, Outcome, Report};
use cinemosaic::{Gate, UPDATE_APP, avb, export, frames, grab_stills, layout, log, master_jobs, media, pick, picks, read_cut_list, selftest, update};

/// Long side of the preview image in pixels (twice the on-screen size for Retina).
const PREVIEW_PX: f64 = 1800.0;
/// Width of the in-memory copies the preview is drawn from.
const THUMB_W: u32 = 480;

#[derive(Default)]
struct State {
    master: Option<(PathBuf, frames::VideoInfo)>,
    list: Option<PathBuf>,
    timeline: Option<Timeline>,
    shots: Vec<Shot>,
    /// full-size stills on disk (export) and small copies (preview), by shot and place in the shot; a sheet change
    /// can need stills not grabbed yet: they stay grey until "Grab stills" fetches just those
    stills: HashMap<Pick, Result<PathBuf, String>>,
    thumbs: Arc<HashMap<Pick, RgbImage>>,
    /// grabs since the source was chosen: each writes into its own folder
    grabs: usize,
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
    /// width / height of the grabbed stills (bin mode has no master to ask)
    still_aspect: Option<f64>,
}

impl State {
    fn frame_aspect(&self) -> f64 {
        self.still_aspect.or(self.master.as_ref().map(|m| m.1.aspect)).unwrap_or(16.0 / 9.0)
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
    static STATE: RefCell<State> = RefCell::new(State::default());
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
    Settings { language, dark, tab, betas, update: Arc::default() }
}

fn save_settings(s: &Settings) {
    // comfort only; if saving fails the defaults apply next time
    if let Some(p) = settings_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, format!("{}\n{}\n{}\n{}\n", s.language, if s.dark { "dark" } else { "light" }, s.tab, if s.betas { "betas" } else { "final" }));
    }
}

fn apply_menus(shared: &Shared, s: &Settings) {
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

/// Menu of a second window (manual, About, settings) forwards to the main window.
fn forward_menu(shared: &Shared, weak: &slint::Weak<AppWindow>) {
    let w = weak.clone();
    shared.on_select_language(move |i| w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_select_language(i)));
    let w = weak.clone();
    shared.on_select_dark(move |d| w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_select_dark(d)));
    let w = weak.clone();
    shared.on_select_betas(move |b| w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_select_betas(b)));
    let w = weak.clone();
    shared.on_show_help(move || w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_show_help()));
    let w = weak.clone();
    shared.on_show_about(move || w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_show_about()));
    let w = weak.clone();
    shared.on_show_log(move || w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_show_log()));
    let w = weak.clone();
    shared.on_check_update(move || w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_check_update()));
    let w = weak.clone();
    shared.on_install_update(move || w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_install_update()));
    let w = weak.clone();
    shared.on_show_release(move || w.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_show_release()));
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
fn apply_all(app: &AppWindow, others: &Others, s: &Settings) {
    apply_menus(&app.global::<Shared>(), s);
    others.settings.borrow().iter().for_each(|w| apply_menus(&w.global::<Shared>(), s));
    others.help.borrow().iter().for_each(|h| update_help(h, s));
    #[cfg(target_os = "macos")]
    {
        // Slint rebuilds the menu after this event; add the item back once that is done
        let title = app.get_txt_settings_menu().to_string();
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

/// Reads the sheet fields; Err = name of the first field that is not a usable number.
fn read_sheet(app: &AppWindow) -> Result<layout::Sheet, &'static str> {
    fn num(s: &str, min: f64, max: f64, field: &'static str) -> Result<f64, &'static str> {
        let v: f64 = s.trim().replace(',', ".").parse().map_err(|_| field)?;
        if (min..=max).contains(&v) { Ok(v) } else { Err(field) }
    }
    let (mut w, mut h) = match app.get_paper() as usize {
        0 => (num(&app.get_custom_w(), 10.0, 5000.0, "width")?, num(&app.get_custom_h(), 10.0, 5000.0, "height")?),
        i => {
            let d = layout::DIN[(i - 1).min(layout::DIN.len() - 1)];
            (d.1, d.2)
        }
    };
    if app.get_landscape() != (w > h) && w != h {
        (w, h) = (h, w);
    }
    let dpi = num(&app.get_dpi(), 10.0, 1200.0, "dpi")?;
    let margin = num(&app.get_margin(), 0.0, w.min(h) / 2.0 - 1.0, "margin")?;
    let gap = num(&app.get_gap(), 0.0, 500.0, "gap")?;
    let columns = if app.get_auto_cols() { None } else { Some(num(&app.get_cols(), 1.0, 1000.0, "columns")? as u32) };
    Ok(layout::Sheet { width_mm: w, height_mm: h, dpi, margin_mm: margin, gap_mm: gap, columns, ..Default::default() })
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

thread_local! {
    /// installed font families (menu order) and the faces already loaded
    static FONTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static FACES: RefCell<std::collections::HashMap<(String, bool, bool), Option<cinemosaic::fonts::Face>>> = RefCell::new(Default::default());
}

/// Trim area, frame line and headline from the Design tab and the crop-marks switch; Err = the field that is not
/// usable. `aspect` is the picture format the trim area takes.
fn read_decor(app: &AppWindow, sheet: &mut layout::Sheet, aspect: f64) -> Result<(), &'static str> {
    let num = |s: &str, min: f64, max: f64, field: &'static str| -> Result<f64, &'static str> {
        let v: f64 = s.trim().replace(',', ".").parse().map_err(|_| field)?;
        if (min..=max).contains(&v) { Ok(v) } else { Err(field) }
    };
    sheet.trim = app.get_trim().then_some(aspect);
    let line_color = hex_color(&app.get_line_color());
    if let Some([r, g, b]) = line_color {
        app.set_line_swatch(slint::Color::from_rgb_u8(r, g, b));
    }
    let inset = num(&app.get_line_inset(), 0.0, 200.0, "line-inset")?;
    sheet.line = if app.get_line_on() {
        Some(layout::Line {
            width_mm: num(&app.get_line_width(), 0.05, 20.0, "line-width")?,
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
        let size = num(&app.get_head_size(), 2.0, 1000.0, "head-size")?;
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
            let dx_mm = num(&app.get_head_dx(), -2000.0, 2000.0, "head-dx")?;
            let dy_mm = num(&app.get_head_dy(), -2000.0, 2000.0, "head-dy")?;
            sheet.headline = Some(layout::Headline { text, face, size_pt: size, color, align, valign, dx_mm, dy_mm });
        }
    }
    Ok(())
}

/// Everything the sheet needs from the fields: sheet with its decoration and the picture format.
fn read_all(app: &AppWindow) -> Result<(layout::Sheet, Option<f64>), &'static str> {
    let mut sheet = read_sheet(app)?;
    let picture = read_picture(app)?;
    let aspect = picture.unwrap_or_else(|| STATE.with(|st| st.borrow().frame_aspect()));
    read_decor(app, &mut sheet, aspect)?;
    Ok((sheet, picture))
}

/// Loads the installed font families in a thread (that takes a moment) and fills the Font menu; Helvetica Neue first
/// choice, like the apps' headlines.
fn load_fonts(app: &AppWindow) {
    let weak = app.as_weak();
    std::thread::spawn(move || {
        let names = cinemosaic::fonts::families();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            let pick = ["Helvetica Neue", "Helvetica", "Arial"].iter().find_map(|w| names.iter().position(|n| n == w)).unwrap_or(0);
            let model: Vec<SharedString> = names.iter().map(|n| n.as_str().into()).collect();
            app.set_fonts(ModelRc::new(VecModel::from(model)));
            app.set_font(pick as i32);
            FONTS.with(|f| *f.borrow_mut() = names);
            refresh(&app);
        });
    });
}

thread_local! {
    /// the presets by name, sorted; loaded at start, written to presets.json after every change
    static PRESETS: RefCell<Vec<(String, serde_json::Value)>> = const { RefCell::new(Vec::new()) };
}

fn presets_path() -> Option<PathBuf> {
    app_dir().map(|d| d.join("presets.json"))
}

/// Reads presets.json (a missing or broken file = no presets) and fills the menu.
fn show_presets(app: &AppWindow) {
    let list: Vec<(String, serde_json::Value)> = presets_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Vec<serde_json::Value>>(&b).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| Some((e.get("name")?.as_str()?.to_string(), e.get("values")?.clone())))
        .collect();
    PRESETS.with(|p| *p.borrow_mut() = list);
    preset_menu(app);
}

fn preset_menu(app: &AppWindow) {
    let model: Vec<SharedString> = PRESETS.with(|p| p.borrow().iter().map(|e| e.0.as_str().into()).collect());
    app.set_presets(ModelRc::new(VecModel::from(model)));
}

/// Writes all presets atomically and refreshes the menu; `status` is the line under it when saving works.
fn store_presets(app: &AppWindow, status: String) {
    preset_menu(app);
    let list: Vec<serde_json::Value> = PRESETS.with(|p| p.borrow().iter().map(|(n, v)| serde_json::json!({"name": n, "values": v})).collect());
    let result = presets_path().ok_or_else(|| std::io::Error::other("no home folder")).and_then(|path| {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let part = path.with_extension("json.part");
        std::fs::write(&part, serde_json::to_vec_pretty(&list).unwrap_or_default())?;
        std::fs::rename(&part, &path)
    });
    match result {
        Ok(()) => app.set_preset_status(status.into()),
        Err(e) => {
            log::write(&format!("presets not saved: {e}"));
            app.set_preset_status(tr(["Could not save the presets: {}", "Presets ließen sich nicht sichern: {}", "Impossible d'enregistrer les préréglages : {}", "No se pudieron guardar los preajustes: {}"]).replacen("{}", &e.to_string(), 1).into());
        }
    }
}

/// Everything about the look of the sheet, as JSON: paper, picture format, line, headline style and place, crop
/// marks, export format. Not the source and not the headline text: they belong to the project. The font is kept by
/// name, so the preset works on another Mac.
fn preset_json(app: &AppWindow) -> serde_json::Value {
    let font = FONTS.with(|f| f.borrow().get(app.get_font() as usize).cloned()).unwrap_or_default();
    serde_json::json!({
        "paper": app.get_paper(), "custom-w": app.get_custom_w().as_str(), "custom-h": app.get_custom_h().as_str(),
        "landscape": app.get_landscape(), "dpi": app.get_dpi().as_str(), "margin": app.get_margin().as_str(),
        "gap": app.get_gap().as_str(), "auto-cols": app.get_auto_cols(), "cols": app.get_cols().as_str(),
        "picture": app.get_picture(), "custom-aspect": app.get_custom_aspect().as_str(), "trim": app.get_trim(),
        "line-on": app.get_line_on(), "line-width": app.get_line_width().as_str(), "line-inset": app.get_line_inset().as_str(),
        "line-color": app.get_line_color().as_str(), "line-pattern": app.get_line_pattern(),
        "font": font, "head-size": app.get_head_size().as_str(), "head-bold": app.get_head_bold(), "head-italic": app.get_head_italic(),
        "head-color": app.get_head_color().as_str(), "head-anchor": app.get_head_anchor(),
        "head-dx": app.get_head_dx().as_str(), "head-dy": app.get_head_dy().as_str(),
        "export-format": app.get_export_format(),
    })
}

/// Sets the fields from a preset. Missing or wrong keys keep the field as it is (older or newer presets).
fn apply_preset(app: &AppWindow, v: &serde_json::Value) {
    let int = |k: &str, max: i64, set: &dyn Fn(i32)| {
        if let Some(n) = v.get(k).and_then(|x| x.as_i64()).filter(|n| (0..=max).contains(n)) {
            set(n as i32)
        }
    };
    let boolean = |k: &str, set: &dyn Fn(bool)| v.get(k).and_then(|x| x.as_bool()).into_iter().for_each(set);
    let text = |k: &str, set: &dyn Fn(SharedString)| v.get(k).and_then(|x| x.as_str()).filter(|s| s.len() < 100).into_iter().for_each(|s| set(s.into()));
    int("paper", layout::DIN.len() as i64, &|n| app.set_paper(n));
    text("custom-w", &|s| app.set_custom_w(s));
    text("custom-h", &|s| app.set_custom_h(s));
    boolean("landscape", &|b| app.set_landscape(b));
    text("dpi", &|s| app.set_dpi(s));
    text("margin", &|s| app.set_margin(s));
    text("gap", &|s| app.set_gap(s));
    boolean("auto-cols", &|b| app.set_auto_cols(b));
    text("cols", &|s| app.set_cols(s));
    int("picture", layout::PICTURE.len() as i64 + 1, &|n| app.set_picture(n));
    text("custom-aspect", &|s| app.set_custom_aspect(s));
    boolean("trim", &|b| app.set_trim(b));
    boolean("line-on", &|b| app.set_line_on(b));
    text("line-width", &|s| app.set_line_width(s));
    text("line-inset", &|s| app.set_line_inset(s));
    text("line-color", &|s| app.set_line_color(s));
    int("line-pattern", layout::Pattern::ALL.len() as i64 - 1, &|n| app.set_line_pattern(n));
    if let Some(name) = v.get("font").and_then(|x| x.as_str())
        && let Some(i) = FONTS.with(|f| f.borrow().iter().position(|n| n == name))
    {
        app.set_font(i as i32);
    }
    text("head-size", &|s| app.set_head_size(s));
    boolean("head-bold", &|b| app.set_head_bold(b));
    boolean("head-italic", &|b| app.set_head_italic(b));
    text("head-color", &|s| app.set_head_color(s));
    int("head-anchor", 8, &|n| app.set_head_anchor(n));
    text("head-dx", &|s| app.set_head_dx(s));
    text("head-dy", &|s| app.set_head_dy(s));
    int("export-format", 3, &|n| app.set_export_format(n));
}

/// Picture format to crop to: None = full frame; Err = the custom field is not a format.
fn read_picture(app: &AppWindow) -> Result<Option<f64>, &'static str> {
    match app.get_picture() as usize {
        0 => Ok(None),
        i if i <= layout::PICTURE.len() => Ok(Some(layout::PICTURE[i - 1].1)),
        _ => layout::parse_aspect(&app.get_custom_aspect()).map(Some).ok_or("aspect"),
    }
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
    app.set_page_aspect((sheet.width_mm / sheet.height_mm) as f32);
    STATE.with(|st| {
        let st = st.borrow();
        let picks = st.picks(&sheet, picture);
        let n = picks.len();
        // grabbed but not complete: the sheet changed and needs stills that were not grabbed yet
        let missing = if st.stills.is_empty() { 0 } else { picks.iter().filter(|p| !st.stills.contains_key(p)).count() };
        app.set_missing(missing as i32);
        let aspect = picture.unwrap_or(st.frame_aspect());
        let (w, h) = sheet.size_px();
        let mut info = format!("{w} × {h} px");
        if n > 0 {
            let g = sheet.grid(n, aspect);
            info = format!("{} × {} · {info} · {} {} × {} px", g.cols, g.rows, tr(["still", "Still", "image", "fotograma"]), g.cell_w.round(), g.cell_h.round());
        }
        let gb = w as f64 * h as f64 * 3.0 / 1e9;
        if gb >= 1.0 {
            info += &tr([" · needs about {} GB of memory to save", " · Speichern braucht etwa {} GB Arbeitsspeicher", " · l’enregistrement demande environ {} Go de mémoire", " · guardar necesita unos {} GB de memoria"]).replace("{}", &format!("{gb:.1}"));
        }
        app.set_sheet_info(info.into());
        // where the headline is, for dragging it in the preview
        app.set_page_w_mm(sheet.width_mm as f32);
        match sheet.plan().text {
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
        let job = RenderJob { sheet: small, picks, aspect, picture, thumbs: st.thumbs.clone(), generation: RENDER_GEN.fetch_add(1, Ordering::Relaxed) + 1 };
        RENDERER.with(|r| {
            let mut r = r.borrow_mut();
            if r.is_none() {
                *r = Some(start_renderer(app.as_weak()));
            }
            if let Some(tx) = r.as_ref() {
                let _ = tx.send(job);
            }
        });
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

static RENDER_GEN: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static RENDERER: RefCell<Option<std::sync::mpsc::Sender<RenderJob>>> = const { RefCell::new(None) };
}

/// The preview is drawn off the UI thread, so sliders and zoom never wait for it; jobs that piled up while it was
/// drawing are skipped up to the newest.
fn start_renderer(weak: slint::Weak<AppWindow>) -> std::sync::mpsc::Sender<RenderJob> {
    let (tx, rx) = std::sync::mpsc::channel::<RenderJob>();
    std::thread::spawn(move || {
        while let Ok(mut job) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                job = newer;
            }
            let page = layout::render(&job.sheet, job.picks.len(), job.aspect, |i| job.thumbs.get(&job.picks[i]).map(|t| cropped(t.clone(), job.picture)));
            if job.generation != RENDER_GEN.load(Ordering::Relaxed) {
                continue; // a newer one is waiting
            }
            let buffer = SharedPixelBuffer::<Rgb8Pixel>::clone_from_slice(page.as_raw(), page.width(), page.height());
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(app) = weak.upgrade()
                    && job.generation == RENDER_GEN.load(Ordering::Relaxed)
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
    st.grabs = 0;
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

/// Master and cut list both chosen: read the list and show what it holds.
fn load_source(app: &AppWindow) {
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        clear_source(app, &mut st);
        let (Some((_, info)), Some(list)) = (st.master.clone(), st.list.clone()) else { return };
        match read_cut_list(&list, info.rate) {
            Ok(t) => {
                let shots = t.shots();
                if shots.is_empty() {
                    app.set_source_error(tr(["The cut list has no video clips.", "Die Schnittliste enthält keine Videoclips.", "La liste de montage ne contient aucun clip vidéo.", "La lista de montaje no tiene clips de vídeo."]).into());
                    return;
                }
                let before = shots.iter().filter(|s| s.middle() < info.start_tc).count();
                let mut text = tr(["{} · master starts at {}", "{} · Master beginnt bei {}", "{} · le master commence à {}", "{} · el máster empieza en {}"])
                    .replacen("{}", &describe(&t, &shots, 1), 1)
                    .replacen("{}", &format_tc(info.start_tc, info.rate), 1);
                if before > 0 {
                    text += &tr([" · {} lie before the master’s start: check the master’s timecode", " · {} liegen vor dem Start des Masters: Timecode des Masters prüfen", " · {} se trouvent avant le début du master : vérifiez son timecode", " · {} quedan antes del inicio del máster: revisa su timecode"]).replace("{}", &before.to_string());
                }
                app.set_source_info(text.into());
                app.set_can_grab(true);
                st.shots = shots;
                st.timeline = Some(t);
            }
            Err(e) => app.set_source_error(e.into()),
        }
    });
    refresh(app);
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
    let (shots, wanted, fresh, master, cut, volumes, rate, gate, dir) = STATE.with(|st| {
        let mut st = st.borrow_mut();
        let all = st.picks(&sheet, picture);
        let missing: Vec<Pick> = all.iter().filter(|p| !st.stills.contains_key(p)).copied().collect();
        let fresh = missing.is_empty() || st.stills.is_empty();
        if fresh {
            st.stills.clear();
            st.thumbs = Arc::default();
            st.still_aspect = None;
        }
        st.gate = Arc::default();
        st.grabs += 1;
        let volumes: Vec<PathBuf> = st.volumes.iter().filter(|v| v.1).map(|v| v.0.clone()).collect();
        let dir = cache_dir().join(format!("grab-{}", st.grabs));
        (st.shots.clone(), if fresh { all } else { missing }, fresh, st.master.clone(), st.cut.clone(), volumes, st.rate(), st.gate.clone(), dir)
    });
    let bin_mode = app.get_mode() == 1;
    if (bin_mode && cut.is_none()) || (!bin_mode && master.is_none()) {
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
        let jobs = match (&cut, &master) {
            (Some(cut), _) if bin_mode => cut.jobs(&shots, &wanted, &volumes, &|| gate.go(), LANGUAGES[LANG.load(Ordering::Relaxed)].0),
            (_, Some((path, info))) => master_jobs(&shots, &wanted, path, info, LANGUAGES[LANG.load(Ordering::Relaxed)].0),
            _ => return,
        };
        if fresh {
            let _ = std::fs::remove_dir_all(cache_dir()); // stills of an earlier run
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
        let stills = if gate.cancel.load(Ordering::Relaxed) { None } else { grab_stills(&jobs, &dir, &gate, &progress) };
        let thumbs = stills.as_ref().map(|s| thumbnails(s, &gate));
        let still_aspect = stills.iter().flatten().find_map(|r| r.as_ref().ok().and_then(|p| image::image_dimensions(p).ok())).map(|(w, h)| w as f64 / h as f64);
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
            match (stills, thumbs) {
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
                    STATE.with(|st| {
                        let mut st = st.borrow_mut();
                        let mut all = (*st.thumbs).clone();
                        all.extend(wanted.iter().zip(thumbs).filter_map(|(p, t)| Some((*p, t?))));
                        st.thumbs = Arc::new(all);
                        st.stills.extend(wanted.iter().copied().zip(stills));
                        st.still_aspect = st.still_aspect.or(still_aspect);
                    });
                    app.set_state(2);
                }
                // cancelled: the stills of earlier grabs stay
                _ => app.set_state(if STATE.with(|st| st.borrow().stills.is_empty()) { 0 } else { 2 }),
            }
            refresh(&app);
        });
    });
}

/// Bin mode: reads sequence `index` of the chosen bin in a thread (the project's other bins may be needed).
fn load_bin(app: &AppWindow, index: usize) {
    let (path, gate) = STATE.with(|st| {
        let mut st = st.borrow_mut();
        clear_source(app, &mut st);
        st.gate = Arc::default();
        (st.bin.clone(), st.gate.clone())
    });
    let Some(path) = path else { return refresh(app) };
    app.set_sequence(index as i32);
    app.set_state(1);
    app.set_paused(false);
    app.set_progress(0.0);
    app.set_work_text(tr(["Reading the bin…", "Lese den Bin…", "Lecture du chutier…", "Leyendo el bin…"]).into());
    app.set_time_left("".into());
    refresh(app);
    let weak = app.as_weak();
    std::thread::spawn(move || {
        // dropdown entry 0 = all sequences of the bin
        let which = index.checked_sub(1);
        let result = avb::Bin::read(&path).and_then(|b| BinCut::open(&path, b, which, &|| gate.go()));
        let bin_name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(app) = weak.upgrade() else { return };
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
                    STATE.with(|st| {
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
            refresh(&app);
        });
    });
}

/// Volumes with Avid MediaFiles: new ones switched on, the user's choice kept for the others.
fn sync_volumes(app: &AppWindow) {
    STATE.with(|st| {
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
    let (stills, aspect, name) = STATE.with(|st| {
        let st = st.borrow();
        let name = st.timeline.as_ref().map(|t| t.name.trim().to_string()).filter(|n| !n.is_empty());
        let name = name.or_else(|| st.list.as_ref().or(st.bin.as_ref()).and_then(|l| l.file_stem()).map(|s| s.to_string_lossy().into_owned()));
        let stills: Vec<Option<PathBuf>> = st.picks(&sheet, picture).iter().map(|p| st.stills.get(p).and_then(|r| r.as_ref().ok()).cloned()).collect();
        (stills, picture.unwrap_or(st.frame_aspect()), name.unwrap_or_else(|| "CineMosaic".into()))
    });
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
                    STATE.with(|st| st.borrow_mut().saved = Some(path));
                }
                Err(e) => app.set_export_status(tr(["Could not save: {}", "Speichern fehlgeschlagen: {}", "Impossible d’enregistrer : {}", "No se pudo guardar: {}"]).replace("{}", &e).into()),
            }
        });
    });
}

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(target_os = "macos")]
    mac_window::srgb_windows();
    let settings = Rc::new(RefCell::new(load_settings()));
    clean_stills(false);
    let app = AppWindow::new()?;
    repaint_on_resize!(app);
    let set_language = |code: &str| {
        let _ = slint::select_bundled_translation(code);
        LANG.store(LANGUAGES.iter().position(|l| l.0 == code).unwrap_or(0), Ordering::Relaxed);
    };
    set_language(&settings.borrow().language);
    apply_menus(&app.global::<Shared>(), &settings.borrow());
    let others = Others::default();
    log::write(&format!("start {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH));
    std::panic::set_hook(Box::new(|info| log::write(&format!("crash: {info}"))));

    // Menu actions run after the click: they rename menu items, which crashed during macOS menu handling (seen in Ferry).
    app.global::<Shared>().on_select_language({
        let (weak, settings, others) = (app.as_weak(), settings.clone(), others.clone());
        move |i| {
            let (weak, settings, others) = (weak.clone(), settings.clone(), others.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let Some(app) = weak.upgrade() else { return };
                let code = LANGUAGES.get(i as usize).map_or("en", |l| l.0);
                set_language(code);
                settings.borrow_mut().language = code.into();
                save_settings(&settings.borrow());
                apply_all(&app, &others, &settings.borrow());
                // texts Rust wrote (source line, sheet line) follow the language
                load_texts_again(&app);
            });
        }
    });
    app.global::<Shared>().on_select_dark({
        let (weak, settings, others) = (app.as_weak(), settings.clone(), others.clone());
        move |dark| {
            let (weak, settings, others) = (weak.clone(), settings.clone(), others.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let Some(app) = weak.upgrade() else { return };
                settings.borrow_mut().dark = dark;
                save_settings(&settings.borrow());
                apply_all(&app, &others, &settings.borrow());
            });
        }
    });
    // "Get test versions": the old result no longer applies – hide "up to date" and check again quietly
    app.global::<Shared>().on_select_betas({
        let (weak, settings, others) = (app.as_weak(), settings.clone(), others.clone());
        move |betas| {
            let (weak, settings, others) = (weak.clone(), settings.clone(), others.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let Some(app) = weak.upgrade() else { return };
                settings.borrow_mut().betas = betas;
                save_settings(&settings.borrow());
                {
                    let s = settings.borrow();
                    let mut u = s.update.lock().unwrap();
                    if u.state == 2 || u.state == 4 {
                        u.state = 0;
                    }
                }
                apply_all(&app, &others, &settings.borrow());
                check_for_update(&app, &settings.borrow(), false);
            });
        }
    });

    // Settings… opens the tab chosen last (never About); About opens the same window on the About tab
    let open_settings = {
        let (weak, settings, win) = (app.as_weak(), settings.clone(), others.settings.clone());
        move |about: bool| {
            let (weak, settings, win) = (weak.clone(), settings.clone(), win.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let mut win = win.borrow_mut();
                if win.is_none() {
                    *win = SettingsWindow::new().ok();
                    win.iter().for_each(|w| {
                        repaint_on_resize!(w);
                        forward_menu(&w.global::<Shared>(), &weak);
                        wire_support(w, &settings);
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
    };
    app.global::<Shared>().on_show_settings({
        let open = open_settings.clone();
        move || open(false)
    });
    app.global::<Shared>().on_show_about(move || open_settings(true));
    app.global::<Shared>().on_refresh_update({
        let (weak, settings, others) = (app.as_weak(), settings.clone(), others.clone());
        move || weak.upgrade().iter().for_each(|a| apply_all(a, &others, &settings.borrow()))
    });
    app.global::<Shared>().on_check_update({
        let (weak, settings) = (app.as_weak(), settings.clone());
        move || weak.upgrade().iter().for_each(|a| check_for_update(a, &settings.borrow(), true))
    });
    app.global::<Shared>().on_install_update({
        let (weak, settings) = (app.as_weak(), settings.clone());
        move || weak.upgrade().iter().for_each(|a| install_update(a, &settings.borrow()))
    });
    app.global::<Shared>().on_show_release({
        let settings = settings.clone();
        move || {
            if let Some(r) = settings.borrow().update.lock().unwrap().available.as_ref() {
                let _ = std::process::Command::new("open").arg(&r.page).spawn();
            }
        }
    });
    // quietly shortly after the start, then every 6 hours
    slint::Timer::single_shot(Duration::from_secs(5), {
        let (weak, settings) = (app.as_weak(), settings.clone());
        move || weak.upgrade().iter().for_each(|a| check_for_update(a, &settings.borrow(), false))
    });
    let update_timer = slint::Timer::default();
    update_timer.start(slint::TimerMode::Repeated, Duration::from_secs(6 * 3600), {
        let (weak, settings) = (app.as_weak(), settings.clone());
        move || weak.upgrade().iter().for_each(|a| check_for_update(a, &settings.borrow(), false))
    });
    app.global::<Shared>().on_show_log(|| {
        log::write("log file shown");
        let _ = std::process::Command::new("open").arg("-R").arg(log::path()).spawn();
    });
    app.global::<Shared>().on_show_help({
        let (weak, settings, help) = (app.as_weak(), settings.clone(), others.help.clone());
        move || {
            let (weak, settings, help) = (weak.clone(), settings.clone(), help.clone());
            slint::Timer::single_shot(Duration::ZERO, move || {
                let mut help = help.borrow_mut();
                if help.is_none() {
                    *help = HelpWindow::new().ok();
                    help.iter().for_each(|h| {
                        repaint_on_resize!(h);
                        forward_menu(&h.global::<Shared>(), &weak)
                    });
                }
                if let Some(h) = help.as_ref() {
                    update_help(h, &settings.borrow());
                    let _ = h.show();
                }
            });
        }
    });

    app.on_pick_master({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(p) = rfd::FileDialog::new().set_title(tr(["Master file", "Masterfile", "Fichier master", "Archivo máster"])).pick_file() else { return };
            match frames::probe(&p) {
                Ok(info) => {
                    app.set_master_name(file_name(&p).into());
                    STATE.with(|st| st.borrow_mut().master = Some((p, info)));
                }
                Err(e) => {
                    app.set_master_name("".into());
                    STATE.with(|st| st.borrow_mut().master = None);
                    app.set_source_error(e.into());
                    return;
                }
            }
            load_source(&app);
        }
    });
    app.on_pick_list({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let Some(p) = rfd::FileDialog::new().set_title(tr(["Cut list", "Schnittliste", "Liste de montage", "Lista de montaje"])).add_filter("EDL, XML, FCPXML, AAF", &["edl", "xml", "fcpxml", "aaf"]).pick_file() else {
                return;
            };
            app.set_list_name(file_name(&p).into());
            STATE.with(|st| st.borrow_mut().list = Some(p));
            load_source(&app);
        }
    });
    app.on_mode_chosen({
        let weak = app.as_weak();
        move |_| {
            let Some(app) = weak.upgrade() else { return };
            if app.get_mode() == 1 {
                sync_volumes(&app);
                let index = app.get_sequence().max(0) as usize;
                load_bin(&app, index);
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
            show_sequence_menu(&app, &names);
            STATE.with(|st| st.borrow_mut().sequence_names = names.clone());
            STATE.with(|st| st.borrow_mut().bin = Some(p));
            if names.is_empty() {
                STATE.with(|st| clear_source(&app, &mut st.borrow_mut()));
                app.set_source_error(tr(["The bin holds no sequence.", "Der Bin enthält keine Sequenz.", "Le chutier ne contient aucune séquence.", "El bin no contiene ninguna secuencia."]).into());
                return refresh(&app);
            }
            sync_volumes(&app);
            load_bin(&app, 0);
        }
    });
    app.on_sequence_chosen({
        let weak = app.as_weak();
        move |i| weak.upgrade().iter().for_each(|a| load_bin(a, i as usize))
    });
    app.on_toggle_volume({
        let weak = app.as_weak();
        move |i| {
            STATE.with(|st| {
                if let Some(v) = st.borrow_mut().volumes.get_mut(i as usize) {
                    v.1 = !v.1;
                }
            });
            weak.upgrade().iter().for_each(sync_volumes);
        }
    });
    // volumes come and go: look every 2 s while the bin mode is shown
    let poll = slint::Timer::default();
    poll.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(2), {
        let weak = app.as_weak();
        move || {
            if let Some(app) = weak.upgrade().filter(|a| a.get_mode() == 1) {
                sync_volumes(&app);
            }
        }
    });
    app.on_grab({
        let weak = app.as_weak();
        move || weak.upgrade().iter().for_each(grab)
    });
    app.on_pause({
        let weak = app.as_weak();
        move |on| {
            STATE.with(|st| st.borrow().gate.pause.store(on, Ordering::Relaxed));
            weak.upgrade().iter().for_each(|a| a.set_paused(on));
        }
    });
    app.on_cancel(|| STATE.with(|st| st.borrow().gate.cancel.store(true, Ordering::Relaxed)));
    // headline anchor: a sensible move for the new point (inside the sheet), then draw
    app.on_anchor_chosen({
        let weak = app.as_weak();
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.set_head_anchor(i);
            app.set_head_dx(["10", "0", "-10"][i as usize % 3].into());
            app.set_head_dy(["8", "0", "-8"][(i as usize / 3).min(2)].into());
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
    show_presets(&app);
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
    app.on_reveal(|| {
        if let Some(p) = STATE.with(|st| st.borrow().saved.clone()) {
            let _ = std::process::Command::new("open").arg("-R").arg(p).spawn();
        }
    });

    #[cfg(target_os = "macos")]
    {
        let (w1, w2) = (app.as_weak(), app.as_weak());
        mac_menu::install(
            Box::new(move || w1.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_show_about())),
            Box::new(move || w2.upgrade().into_iter().for_each(|a| a.global::<Shared>().invoke_show_settings())),
            &app.get_txt_settings_menu(),
        );
    }
    // title bars follow the app's light/dark; the settings tab bar follows the title bar (lighter while active)
    #[cfg(target_os = "macos")]
    let chrome = slint::Timer::default();
    #[cfg(target_os = "macos")]
    chrome.start(slint::TimerMode::Repeated, Duration::from_millis(200), {
        let (win, settings, weak) = (others.settings.clone(), settings.clone(), app.as_weak());
        let fitted = std::cell::Cell::new(false);
        move || {
            mac_window::appearance_all(settings.borrow().dark);
            // the main window can grow, never shrink below what the side column needs (also when dragged)
            if let Some(a) = weak.upgrade() {
                // never taller than the screen minus the title bar (small laptops)
                let (screen_w, screen) = mac_window::screen_size();
                if screen > 0.0 && ((screen - 28.0).min(820.0) - a.get_smallest_height() as f64).abs() > 0.5 {
                    a.set_smallest_height((screen - 28.0).min(820.0) as f32);
                }
                // the standard size is made for a large screen: on a smaller one the window fits it, once
                if screen > 0.0 && !fitted.get() {
                    fitted.set(true);
                    let size = a.window().size().to_logical(a.window().scale_factor());
                    let (w, h) = ((size.width as f64).min(screen_w), (size.height as f64).min(screen - 28.0));
                    if w < size.width as f64 - 0.5 || h < size.height as f64 - 0.5 {
                        a.window().set_size(slint::LogicalSize::new(w as f32, h as f32));
                    }
                }
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
        let win = others.settings.clone();
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

    load_fonts(&app);
    refresh(&app);
    let result = app.run();
    // the stills are only needed while the app is open
    clean_stills(true);
    result
}

/// The Sequence menu: "All sequences", then every sequence of the bin.
fn show_sequence_menu(app: &AppWindow, names: &[String]) {
    let all = tr(["All sequences", "Alle Sequenzen", "Toutes les séquences", "Todas las secuencias"]);
    let model: Vec<SharedString> = std::iter::once(all.as_str()).chain(names.iter().map(|n| n.as_str())).map(Into::into).collect();
    app.set_sequences(ModelRc::new(VecModel::from(model)));
}

/// After a language change: the lines Rust wrote are written again in the new language.
fn load_texts_again(app: &AppWindow) {
    let names = STATE.with(|st| st.borrow().sequence_names.clone());
    if !names.is_empty() {
        show_sequence_menu(app, &names);
    }
    STATE.with(|st| {
        let st = st.borrow();
        if let Some(t) = &st.timeline
            && !st.shots.is_empty()
            && app.get_source_info() != ""
        {
            let mut text = describe(t, &st.shots, st.sequences);
            if let Some((_, info)) = st.master.as_ref().filter(|_| app.get_mode() == 0) {
                text = tr(["{} · master starts at {}", "{} · Master beginnt bei {}", "{} · le master commence à {}", "{} · el máster empieza en {}"])
                    .replacen("{}", &text, 1)
                    .replacen("{}", &format_tc(info.start_tc, info.rate), 1);
            }
            app.set_source_info(text.into());
        }
    });
    refresh(app);
}

#[cfg(target_os = "macos")]
mod mac_menu {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{class, msg_send, sel};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Handler = RefCell<Option<Box<dyn Fn()>>>;
    thread_local! {
        static ABOUT: Handler = RefCell::new(None);
        static SETTINGS: Handler = RefCell::new(None);
        static TITLE: RefCell<String> = RefCell::new("Settings…".into());
    }
    static ORIGINAL_SET_MAIN_MENU: AtomicUsize = AtomicUsize::new(0);
    type ObjFn = extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);

    extern "C-unwind" fn about(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {
        ABOUT.with(|h| h.borrow().iter().for_each(|f| f()));
    }

    extern "C-unwind" fn settings(_: *mut AnyObject, _: Sel, _: *mut AnyObject) {
        SETTINGS.with(|h| h.borrow().iter().for_each(|f| f()));
    }

    extern "C-unwind" fn set_main_menu(app: *mut AnyObject, sel: Sel, menu: *mut AnyObject) {
        let original = ORIGINAL_SET_MAIN_MENU.load(Ordering::Relaxed);
        if original != 0 {
            let original: ObjFn = unsafe { std::mem::transmute(original) };
            original(app, sel, menu);
        }
        ensure();
    }

    fn ns_string(s: &str) -> Retained<AnyObject> {
        let c = std::ffi::CString::new(s).unwrap_or_default();
        unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    /// Adds "Settings…" to the app menu if it is missing and keeps its title in the current language.
    pub fn ensure() {
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let main: *mut AnyObject = msg_send![app, mainMenu];
            if main.is_null() {
                return;
            }
            let count: isize = msg_send![main, numberOfItems];
            if count == 0 {
                return;
            }
            let first: *mut AnyObject = msg_send![main, itemAtIndex: 0isize];
            let app_menu: *mut AnyObject = msg_send![first, submenu];
            if app_menu.is_null() {
                return;
            }
            let title = TITLE.with(|t| ns_string(&t.borrow()));
            let index: isize = msg_send![app_menu, indexOfItemWithTarget: std::ptr::null_mut::<AnyObject>(), andAction: sel!(ssShowSettings:)];
            if index >= 0 {
                let item: *mut AnyObject = msg_send![app_menu, itemAtIndex: index];
                let _: () = msg_send![item, setTitle: &*title];
                return;
            }
            // only into the app menu Slint builds (About, separator, …); macOS sets its own short menu at launch,
            // and inserting past its end throws an Objective-C exception that aborts the app
            let items: isize = msg_send![app_menu, numberOfItems];
            if items < 3 {
                return;
            }
            let second: *mut AnyObject = msg_send![app_menu, itemAtIndex: 1isize];
            let separator_at_1: bool = msg_send![second, isSeparatorItem];
            if !separator_at_1 {
                return;
            }
            let item: Retained<AnyObject> = msg_send![class!(NSMenuItem), new];
            let _: () = msg_send![&*item, setTitle: &*title];
            let _: () = msg_send![&*item, setAction: sel!(ssShowSettings:)];
            let _: () = msg_send![&*item, setKeyEquivalent: &*ns_string(",")];
            let separator: Retained<AnyObject> = msg_send![class!(NSMenuItem), separatorItem];
            // About, separator, here: Settings…, separator, Services …
            let _: () = msg_send![app_menu, insertItem: &*item, atIndex: 2isize];
            let _: () = msg_send![app_menu, insertItem: &*separator, atIndex: 3isize];
        }
    }

    pub fn set_settings_title(title: &str) {
        TITLE.with(|t| *t.borrow_mut() = title.to_string());
        ensure();
    }

    pub fn install(on_about: Box<dyn Fn()>, on_settings: Box<dyn Fn()>, settings_title: &str) {
        ABOUT.with(|h| *h.borrow_mut() = Some(on_about));
        SETTINGS.with(|h| *h.borrow_mut() = Some(on_settings));
        TITLE.with(|t| *t.borrow_mut() = settings_title.to_string());
        let Some(cls) = AnyClass::get(c"NSApplication") else { return };
        let imp = |f: ObjFn| unsafe { std::mem::transmute::<ObjFn, Imp>(f) };
        for sel in [sel!(orderFrontStandardAboutPanel:), sel!(orderFrontStandardAboutPanelWithOptions:)] {
            if let Some(m) = cls.instance_method(sel) {
                // same signature (self, _cmd, one object) as the methods it replaces
                unsafe { m.set_implementation(imp(about)) };
            }
        }
        // the menu item's action; with no target it reaches NSApplication through the responder chain
        unsafe { objc2::ffi::class_addMethod(cls as *const AnyClass as *mut AnyClass, sel!(ssShowSettings:), imp(settings), c"v@:@".as_ptr()) };
        if let Some(m) = cls.instance_method(sel!(setMainMenu:)) {
            let original = unsafe { m.set_implementation(imp(set_main_menu)) };
            ORIGINAL_SET_MAIN_MENU.store(original as usize, Ordering::Relaxed);
        }
        ensure();
    }
}

/// AppKit details Slint does not offer: colours in sRGB like the Swift apps, and the settings window's title bar.
#[cfg(target_os = "macos")]
mod mac_window {
    use objc2::encode::{Encode, Encoding};
    use objc2::runtime::{AnyObject, Imp, Sel};
    use objc2::{class, msg_send, sel};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }
    unsafe impl Encode for Size {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    unsafe impl Encode for Rect {
        const ENCODING: Encoding = Encoding::Struct("CGRect", &[Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]), Size::ENCODING]);
    }

    type Init = extern "C-unwind" fn(*mut AnyObject, Sel, Rect, usize, usize, bool) -> *mut AnyObject;
    static ORIGINAL_INIT: AtomicUsize = AtomicUsize::new(0);

    extern "C-unwind" fn init_srgb(this: *mut AnyObject, sel: Sel, rect: Rect, style: usize, backing: usize, defer: bool) -> *mut AnyObject {
        let original: Init = unsafe { std::mem::transmute(ORIGINAL_INIT.load(Ordering::Relaxed)) };
        let win = original(this, sel, rect, style, backing, defer);
        if !win.is_null() {
            unsafe {
                let srgb: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
                let _: () = msg_send![win, setColorSpace: srgb];
            }
        }
        win
    }

    /// Slint (femtovg, OpenGL) draws without colour management, so #1f2225 came out as #21262a next to Mountpoint's
    /// exact #1f2225. A window tagged as sRGB before its first frame shows the Theme colours exactly like the SwiftUI
    /// apps; set later it only worked now and then – so every NSWindow gets sRGB right in its initializer.
    /// Call once, before the first window is created.
    pub fn srgb_windows() {
        let cls = class!(NSWindow);
        if let Some(m) = cls.instance_method(sel!(initWithContentRect:styleMask:backing:defer:)) {
            let imp: Imp = unsafe { std::mem::transmute(init_srgb as Init) };
            let original = unsafe { m.set_implementation(imp) };
            ORIGINAL_INIT.store(original as usize, Ordering::Relaxed);
        }
    }

    type KeyFn = extern "C-unwind" fn(*mut AnyObject, Sel);
    static ORIGINAL_BECOME_KEY: AtomicUsize = AtomicUsize::new(0);
    static ORIGINAL_RESIGN_KEY: AtomicUsize = AtomicUsize::new(0);
    thread_local! {
        static KEY_CHANGED: std::cell::RefCell<Option<Box<dyn Fn()>>> = std::cell::RefCell::new(None);
    }

    fn key_changed(original: &AtomicUsize, this: *mut AnyObject, sel: Sel) {
        let original: KeyFn = unsafe { std::mem::transmute(original.load(Ordering::Relaxed)) };
        original(this, sel);
        KEY_CHANGED.with(|f| f.borrow().iter().for_each(|f| f()));
    }
    extern "C-unwind" fn become_key(this: *mut AnyObject, sel: Sel) {
        key_changed(&ORIGINAL_BECOME_KEY, this, sel);
    }
    extern "C-unwind" fn resign_key(this: *mut AnyObject, sel: Sel) {
        key_changed(&ORIGINAL_RESIGN_KEY, this, sel);
    }

    /// Calls `f` whenever any window becomes or stops being the key window – the tab bar then changes colour at once
    /// together with the title bar (polling made it lag behind).
    pub fn on_key_change(f: Box<dyn Fn()>) {
        KEY_CHANGED.with(|k| *k.borrow_mut() = Some(f));
        let cls = class!(NSWindow);
        for (name, imp, original) in [
            (sel!(becomeKeyWindow), become_key as KeyFn, &ORIGINAL_BECOME_KEY),
            (sel!(resignKeyWindow), resign_key as KeyFn, &ORIGINAL_RESIGN_KEY),
        ] {
            if original.load(Ordering::Relaxed) != 0 {
                continue;
            }
            if let Some(m) = cls.instance_method(name) {
                let imp: Imp = unsafe { std::mem::transmute(imp) };
                original.store(unsafe { m.set_implementation(imp) } as usize, Ordering::Relaxed);
            }
        }
    }

    fn ns_window(w: &slint::Window) -> Option<*mut AnyObject> {
        let handle = w.window_handle();
        let RawWindowHandle::AppKit(h) = handle.window_handle().ok()?.as_raw() else { return None };
        let view = h.ns_view.as_ptr() as *mut AnyObject;
        let win: *mut AnyObject = unsafe { msg_send![view, window] };
        (!win.is_null()).then_some(win)
    }

    /// Content up to the top edge under a transparent title bar (like an NSToolbar window), so the tab bar and the
    /// title bar are one surface without the separator line. Keeps the content size (`height` = whole window).
    pub fn unify_titlebar(w: &slint::Window, width: f64, height: f64) {
        let Some(win) = ns_window(w) else { return };
        unsafe {
            let mask: usize = msg_send![win, styleMask];
            const FULL_SIZE_CONTENT_VIEW: usize = 1 << 15;
            const MINIATURIZABLE: usize = 1 << 2;
            if mask & FULL_SIZE_CONTENT_VIEW == 0 {
                // HIG settings: minimize and zoom dimmed (zoom already is: the size is fixed)
                let _: () = msg_send![win, setStyleMask: (mask | FULL_SIZE_CONTENT_VIEW) & !MINIATURIZABLE];
                let _: () = msg_send![win, setTitlebarAppearsTransparent: true];
                let _: () = msg_send![win, setContentSize: Size { width, height }];
            }
        }
    }

    /// Usable size of the main screen (without menu bar and Dock), in points; (0, 0) if unknown.
    pub fn screen_size() -> (f64, f64) {
        unsafe {
            let screen: *mut AnyObject = msg_send![class!(NSScreen), mainScreen];
            if screen.is_null() {
                return (0.0, 0.0);
            }
            let frame: Rect = msg_send![screen, visibleFrame];
            (frame.width, frame.height)
        }
    }

    /// Smallest size the user can drag the window to (content size in points); returns the value AppKit now holds.
    pub fn set_min_size(w: &slint::Window, width: f64, height: f64) -> Option<(f64, f64)> {
        let win = ns_window(w)?;
        unsafe {
            let _: () = msg_send![win, setContentMinSize: Size { width, height }];
            let got: Size = msg_send![win, contentMinSize];
            Some((got.width, got.height))
        }
    }

    /// Title bar, traffic lights and title text in the app's own light/dark (not the system's): every window.
    pub fn appearance_all(dark: bool) {
        let name: &std::ffi::CStr = if dark { c"NSAppearanceNameDarkAqua" } else { c"NSAppearanceNameAqua" };
        unsafe {
            let name: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
            let wanted: *mut AnyObject = msg_send![class!(NSAppearance), appearanceNamed: name];
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let windows: *mut AnyObject = msg_send![app, windows];
            let count: usize = msg_send![windows, count];
            for i in 0..count {
                let w: *mut AnyObject = msg_send![windows, objectAtIndex: i];
                let current: *mut AnyObject = msg_send![w, appearance];
                if current != wanted {
                    let _: () = msg_send![w, setAppearance: wanted];
                }
            }
        }
    }

    /// Whether the window is the key window (the title bar is lighter then; the tab bar follows).
    pub fn is_key(w: &slint::Window) -> bool {
        ns_window(w).is_some_and(|win| unsafe { msg_send![win, isKeyWindow] })
    }
}

#[cfg(test)]
mod tests {
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
