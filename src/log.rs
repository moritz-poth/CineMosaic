//! Log file for error analysis: ~/Library/Logs/CineMosaic/CineMosaic.log (Windows: %APPDATA%\CineMosaic).
//! One line per event with local time; at 1 MB the file moves to "… .old.log" and a new one starts.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 1_000_000;

pub fn path() -> PathBuf {
    let dir = if cfg!(windows) {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("CineMosaic"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/CineMosaic"))
    };
    dir.unwrap_or_else(|| PathBuf::from(".")).join("CineMosaic.log")
}

/// Appends a line. Logging never stops the app: errors while writing are ignored.
pub fn write(msg: &str) {
    if cfg!(test) {
        return; // unit tests never write the user's log
    }
    let p = path();
    let _ = fs::create_dir_all(p.parent().unwrap_or(&p));
    if fs::metadata(&p).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&p, p.with_extension("old.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg.replace('\n', " / "));
    }
}
