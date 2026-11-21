//! CineMosaic: one still from the middle of every shot of a cut, laid out on one sheet.

pub mod aaf;
pub mod avb;
pub mod bin;
pub mod edl;
pub mod export;
pub mod fcp7;
pub mod fcpxml;
pub mod fonts;
pub mod frames;
pub mod layout;
pub mod log;
pub mod media;
pub mod project;
pub mod report;
pub mod selftest;
pub mod timeline;
pub mod update;

use std::path::{Path, PathBuf};

/// What this app is on GitHub and in its Info.plist (updates).
pub const UPDATE_APP: update::App = update::App {
    repo: "moritz-poth/cinemosaic",
    name: "CineMosaic",
    bundle_id: "io.github.moritz-poth.cinemosaic",
    release_key: "CineMosaicRelease",
    public_key: "TKYjO1yclk2X2SXtdRuOEPJNMmLJt5F6tocFf7yrMf0=",
};

/// The text for the language: [English, German, French, Spanish]; English for unknown codes.
pub fn pick<'a>(lang: &str, [en, de, fr, es]: [&'a str; 4]) -> &'a str {
    match lang {
        "de" => de,
        "fr" => fr,
        "es" => es,
        _ => en,
    }
}
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use timeline::{Pick, Rate, Shot, Timeline};

/// Reads a cut list by its extension. EDLs carry no frame rate: `rate` (the master's) is used.
pub fn read_cut_list(path: &Path, rate: Rate) -> Result<Timeline, String> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("aaf")) {
        return aaf::parse_file(path);
    }
    let text = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&text);
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("edl") => edl::parse(&text, rate),
        Some("fcpxml") => fcpxml::parse(&text),
        Some("xml") if text.contains("<fcpxml") => fcpxml::parse(&text),
        Some("xml") => fcp7::parse(&text),
        _ => Err("unknown cut list (EDL, XML, FCPXML or AAF expected)".into()),
    }
}

/// The stills that fill `sheet`'s grid for stills of width/height `aspect` (see `timeline::fill`).
pub fn picks(shots: &[Shot], sheet: &layout::Sheet, aspect: f64) -> Vec<Pick> {
    let g = sheet.grid(shots.len(), aspect);
    timeline::fill(shots, (g.cols * g.rows) as usize)
}

/// Each picked still in master mode: its record frame minus the master's start timecode.
pub fn master_jobs(shots: &[Shot], picks: &[Pick], master: &Path, info: &frames::VideoInfo, lang: &str) -> Vec<Result<Job, String>> {
    picks
        .iter()
        .map(|&(i, part)| match shots[i].at(part) - info.start_tc {
            f if f >= 0 => Ok(Job { file: master.to_path_buf(), frame: f, rate: info.rate }),
            _ => Err(pick(lang, ["the shot lies before the master file’s start timecode", "der Shot liegt vor dem Start-Timecode des Masters", "le plan se trouve avant le timecode de début du master", "el plano queda antes del timecode inicial del máster"]).into()),
        })
        .collect()
}

/// One still to grab: frame `frame` (0 = first) of `file`.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub file: PathBuf,
    pub frame: i64,
    pub rate: Rate,
}

/// Stop flag shared with the UI: cancel ends the run at once, pause holds it.
#[derive(Default)]
pub struct Gate {
    pub cancel: AtomicBool,
    pub pause: AtomicBool,
}

impl Gate {
    /// false = cancelled; blocks while paused.
    pub fn go(&self) -> bool {
        while self.pause.load(Ordering::Relaxed) && !self.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        !self.cancel.load(Ordering::Relaxed)
    }
}

/// Grabs every still into `dir` as 0001.png, 0002.png … (4 at a time). Returns per shot the still or
/// the reason it is missing; None when cancelled. `progress(done, total)` is called from worker threads.
pub fn grab_stills(
    jobs: &[Result<Job, String>],
    dir: &Path,
    gate: &Gate,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Option<Vec<Result<PathBuf, String>>> {
    std::fs::create_dir_all(dir).ok();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Vec<std::sync::Mutex<Option<Result<PathBuf, String>>>> = jobs.iter().map(|_| Default::default()).collect();
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs.len() || !gate.go() {
                        break;
                    }
                    let out = dir.join(format!("{:04}.png", i + 1));
                    let r = match &jobs[i] {
                        Ok(j) => frames::grab(&j.file, j.frame, j.rate, &out).map(|_| out),
                        Err(e) => Err(e.clone()),
                    };
                    *results[i].lock().unwrap() = Some(r);
                    progress(done.fetch_add(1, Ordering::Relaxed) + 1, jobs.len());
                }
            });
        }
    });
    if gate.cancel.load(Ordering::Relaxed) {
        return None;
    }
    Some(results.into_iter().map(|r| r.into_inner().unwrap().unwrap_or_else(|| Err("not read".into()))).collect())
}
