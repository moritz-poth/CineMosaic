//! Self-test for testers without developer tools (Settings → Support → "Run self-test…"): checks the app's core
//! functions on its own test data in a temporary folder – never on the user's media or projects – and builds an
//! anonymous report (src/report.rs). The only things it looks at outside its folder: whether the volumes with
//! "Avid MediaFiles" can be read (listing only) and GitHub (update download + signature).
use crate::report::{self, CheckResult, Report};
use crate::timeline::Rate;
use crate::{Gate, export, frames, layout, master_jobs, media, read_cut_list};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub enum Failed {
    Skip(String),
    Fail(String),
}

fn fail<T>(text: impl Into<String>) -> Result<T, Failed> {
    Err(Failed::Fail(text.into()))
}

fn io<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, Failed> {
    r.map_err(|e| Failed::Fail(e.to_string()))
}

/// ffmpeg and ffprobe start (bundled with the app, else installed).
fn tools(_: &Path) -> Result<String, Failed> {
    let mut found = vec![];
    for t in ["ffmpeg", "ffprobe"] {
        let path = frames::tool(t);
        let out = Command::new(&path).arg("-version").output().map_err(|e| Failed::Fail(format!("{t} could not start: {e}")))?;
        let first = String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or_default().split(" Copyright").next().unwrap_or_default().to_string();
        found.push(format!("{first} ({})", if path.starts_with(std::env::current_exe().unwrap_or_default().parent().unwrap_or(Path::new("/"))) { "bundled" } else { "installed" }));
    }
    Ok(found.join(", "))
}

/// A generated test video whose frames carry their number as colour: the grabbed middle frame must be exact.
fn middle_frame(base: &Path) -> Result<String, Failed> {
    let master = base.join("master.mov");
    let ok = Command::new(frames::tool("ffmpeg"))
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=black:s=64x36:r=25:d=8,format=rgb24,geq=r='mod(N,256)':g=0:b=128"])
        .args(["-c:v", "png", "-timecode", "01:00:00:00"])
        .arg(&master)
        .status();
    if !ok.is_ok_and(|s| s.success()) {
        return fail("the test video could not be made");
    }
    let info = io(frames::probe(&master))?;
    let edl = base.join("cut.edl");
    io(fs::write(&edl, "TITLE: T\nFCM: NON-DROP FRAME\n001  AX V C 00:00:00:00 00:00:03:00 01:00:02:00 01:00:05:00\n"))?;
    let shots = io(read_cut_list(&edl, info.rate))?.shots();
    let stills = crate::grab_stills(&master_jobs(&shots, &master, &info, "en"), &base.join("stills"), &Gate::default(), &|_, _| {}).ok_or(Failed::Fail("cancelled".into()))?;
    let still = stills.first().and_then(|s| s.as_ref().ok()).ok_or(Failed::Fail("no still".into()))?;
    let got = io(image::open(still))?.to_rgb8().get_pixel(5, 5)[0] as i64;
    // 01:00:02:00 + 75 / 2 frames, 50 + 37 into the video
    if got == 87 { Ok("frame 87 of 200 – exact".into()) } else { fail(format!("frame {got} instead of 87")) }
}

/// The four cut-list readers on small samples.
fn cut_lists(base: &Path) -> Result<String, Failed> {
    let rate = Rate { num: 25, den: 1 };
    let samples = [
        ("cut.edl", "TITLE: T\n001  AX V C 00:00:00:00 00:00:01:00 01:00:00:00 01:00:01:00\n002  AX V C 00:00:00:00 00:00:01:00 01:00:01:00 01:00:02:00\n"),
        ("cut.xml", r#"<xmeml><sequence><rate><timebase>25</timebase></rate><media><video><track><clipitem><name>A</name><start>0</start><end>25</end></clipitem><clipitem><name>B</name><start>25</start><end>50</end></clipitem></track></video></media></sequence></xmeml>"#),
        ("cut.fcpxml", r#"<fcpxml><resources><format id="r1" frameDuration="1/25s"/></resources><library><event><project name="P"><sequence format="r1"><spine><asset-clip name="A" offset="0s" duration="1s"/><asset-clip name="B" offset="1s" duration="1s"/></spine></sequence></project></event></library></fcpxml>"#),
    ];
    for (name, text) in samples {
        let p = base.join(name);
        io(fs::write(&p, text))?;
        let n = io(read_cut_list(&p, rate))?.shots().len();
        if n != 2 {
            return fail(format!("{name}: {n} shots instead of 2"));
        }
    }
    Ok("EDL, FCP7 XML, FCPXML read".into())
}

/// A small sheet saved in every format, read back.
fn sheets(base: &Path) -> Result<String, Failed> {
    let sheet = layout::Sheet { width_mm: 50.0, height_mm: 30.0, dpi: 150.0, margin_mm: 2.0, gap_mm: 1.0, columns: None, ..Default::default() };
    let page = layout::render(&sheet, 6, 16.0 / 9.0, |_| Some(image::RgbImage::from_pixel(32, 18, image::Rgb([200, 30, 30]))));
    for f in export::Format::ALL {
        let p = base.join(format!("sheet.{}", f.ext()));
        io(export::save(&page, &p, sheet.dpi, f))?;
        if f != export::Format::Pdf {
            let back = io(image::open(&p))?;
            if (back.width(), back.height()) != (page.width(), page.height()) {
                return fail(format!("{}: wrong size", f.ext()));
            }
        } else if !io(fs::read(&p))?.starts_with(b"%PDF") {
            return fail("pdf: not a PDF");
        }
    }
    Ok("PNG, TIFF, JPEG, PDF".into())
}

/// The volumes with Avid MediaFiles answer (listing only, 30 s each).
fn volumes(_: &Path) -> Result<String, Failed> {
    let found = media::volumes();
    if found.is_empty() {
        return Err(Failed::Skip("no volume with “Avid MediaFiles” connected".into()));
    }
    let mut slow = vec![];
    for v in &found {
        let dir = v.join("Avid MediaFiles/MXF");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fs::read_dir(&dir).map(|d| d.count()));
        });
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => slow.push(format!("{}: {e}", report::anonymize(&v.display().to_string()))),
            Err(_) => slow.push(format!("{}: no answer within 30 s", report::anonymize(&v.display().to_string()))),
        }
    }
    if slow.is_empty() { Ok(format!("{} volume(s) readable", found.len())) } else { fail(slow.join("; ")) }
}

fn log_file(_: &Path) -> Result<String, Failed> {
    if cfg!(test) {
        return Err(Failed::Skip("not in unit tests (they never write the log)".into()));
    }
    let before = fs::metadata(crate::log::path()).map_or(0, |m| m.len());
    crate::log::write("self-test: log check");
    let after = fs::metadata(crate::log::path()).map_or(0, |m| m.len());
    if after > before || after > 0 && before > 900_000 { Ok("log file is written".into()) } else { fail("the log file could not be written") }
}

/// The newest release on GitHub downloads, its signature is valid and the app in it is intact.
fn update(_: &Path) -> Result<String, Failed> {
    let release = match crate::update::check(&crate::UPDATE_APP, "0.0.1", true) {
        Ok(Some(r)) => r,
        _ => return Err(Failed::Skip("GitHub not reachable, or no signed release yet".into())),
    };
    match crate::update::download(&release, &crate::UPDATE_APP) {
        Ok(app) => {
            let _ = fs::remove_dir_all(app.parent().unwrap_or(&app));
            Ok(format!("{}: signature valid, app intact", release.tag))
        }
        Err(crate::update::Error::Download) => Err(Failed::Skip("download failed (network)".into())),
        Err(e) => fail(format!("{}: {e:?}", release.tag)),
    }
}

/// Title of a check in the app's language (the report keeps the English title).
pub fn title(id: &str, lang: &str) -> String {
    let t: [&str; 4] = match id {
        "tools" => ["ffmpeg and ffprobe", "ffmpeg und ffprobe", "ffmpeg et ffprobe", "ffmpeg y ffprobe"],
        "frame" => ["Exact middle frame", "Genaue mittlere Frame", "Image du milieu exacte", "Fotograma central exacto"],
        "cutlists" => ["Read cut lists", "Schnittlisten lesen", "Lire les listes de montage", "Leer listas de montaje"],
        "sheets" => ["Save sheets", "Bögen speichern", "Enregistrer les planches", "Guardar hojas"],
        "volumes" => ["Volumes with Avid MediaFiles readable", "Volumes mit Avid MediaFiles lesbar", "Volumes avec Avid MediaFiles lisibles", "Volúmenes con Avid MediaFiles legibles"],
        "log" => ["Log file", "Logdatei", "Fichier journal", "Archivo de registro"],
        "update" => ["Update download and signature", "Update-Download und Signatur", "Téléchargement et signature de la mise à jour", "Descarga y firma de la actualización"],
        _ => return id.into(),
    };
    crate::pick(lang, t).into()
}

type Check = (&'static str, &'static str, fn(&Path) -> Result<String, Failed>);

/// The checks: id (stays English in the report), title (shown translated), function.
pub const CHECKS: [Check; 7] = [
    ("tools", "ffmpeg and ffprobe", tools),
    ("frame", "Exact middle frame", middle_frame),
    ("cutlists", "Read cut lists", cut_lists),
    ("sheets", "Save sheets", sheets),
    ("volumes", "Volumes with Avid MediaFiles readable", volumes),
    ("log", "Log file", log_file),
    ("update", "Update download and signature", update),
];

const PREFIX: &str = "CineMosaicSelftest";

/// Runs every check (call it off the UI thread); `progress(done, title of the next check)` after each step.
pub fn run(lang: &str, progress: &mut dyn FnMut(&[CheckResult], &str)) -> Report {
    let started = Instant::now();
    crate::log::write("self-test started");
    let base = std::env::temp_dir().join(format!("{PREFIX}-{}-{}", std::process::id(), chrono::Local::now().format("%H%M%S")));
    let mut results = vec![];
    let ready = fs::create_dir_all(&base);
    for (id, title, check) in CHECKS {
        progress(&results, title);
        let t = Instant::now();
        let outcome = match &ready {
            Ok(()) => std::panic::catch_unwind(|| check(&base)).unwrap_or_else(|_| fail("the check crashed")),
            Err(e) => fail(format!("test folder could not be created: {e}")),
        };
        let (status, detail) = match outcome {
            Ok(d) => ("pass", d),
            Err(Failed::Skip(d)) => ("skip", d),
            Err(Failed::Fail(d)) => ("fail", d),
        };
        results.push(CheckResult { id: id.into(), title: title.into(), status, detail: report::anonymize(&detail), seconds: (t.elapsed().as_secs_f64() * 100.0).round() / 100.0 });
    }
    let _ = fs::remove_dir_all(&base);
    progress(&results, "");
    let failed = results.iter().filter(|r| r.status == "fail").count();
    crate::log::write(&format!("self-test done: {} checks, {failed} failed", results.len()));
    let mut r = report::new("selftest", report::system_info(lang), report::log_tail(150));
    r.results = results;
    r.duration_seconds = (started.elapsed().as_secs_f64() * 10.0).round() / 10.0;
    r
}

/// The test folder of a run, so tests can check that nothing is left behind.
pub fn leftovers() -> Vec<PathBuf> {
    fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{PREFIX}-{}-", std::process::id()))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All checks that need no volume and no network pass, and the test folder is removed.
    #[test]
    fn checks_pass_on_their_own_data() {
        let mut steps = 0;
        let r = run("en", &mut |_, _| steps += 1);
        assert_eq!(steps, CHECKS.len() + 1);
        for c in &r.results {
            assert_ne!(c.status, "fail", "{}: {}", c.id, c.detail);
        }
        for id in ["tools", "frame", "cutlists", "sheets"] {
            assert_eq!(r.results.iter().find(|c| c.id == id).map(|c| c.status), Some("pass"), "{id}");
        }
        assert!(leftovers().is_empty());
    }
}
