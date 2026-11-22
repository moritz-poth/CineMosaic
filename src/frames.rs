//! Reading frames with ffprobe / ffmpeg (bundled next to the app binary, else from PATH).

use crate::timeline::{Rate, parse_tc};
use std::path::{Path, PathBuf};
use std::process::Command;

/// ffmpeg / ffprobe: next to our own binary first (inside the app bundle), then the usual places.
pub fn tool(name: &str) -> PathBuf {
    let own = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(name)));
    let candidates = own.into_iter().chain(["/opt/homebrew/bin", "/usr/local/bin"].iter().map(|d| Path::new(d).join(name)));
    candidates.into_iter().find(|p| p.is_file()).unwrap_or_else(|| name.into())
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoInfo {
    pub rate: Rate,
    /// first frame's timecode as a frame count (0 without timecode)
    pub start_tc: i64,
    pub width: u32,
    pub height: u32,
    /// display aspect ratio (pixel aspect applied)
    pub aspect: f64,
}

pub fn probe(file: &Path) -> Result<VideoInfo, String> {
    let out = Command::new(tool("ffprobe"))
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
        .arg("stream=r_frame_rate,width,height,sample_aspect_ratio:stream_tags=timecode:format_tags=timecode")
        .args(["-of", "json"])
        .arg(file)
        .output()
        .map_err(|e| format!("ffprobe could not start: {e}"))?;
    if !out.status.success() {
        return Err(format!("not a readable video file: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let s = &j["streams"][0];
    let ratio = |v: &serde_json::Value, sep: char| -> Option<(u32, u32)> {
        let (a, b) = v.as_str()?.split_once(sep)?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    let (num, den) = ratio(&s["r_frame_rate"], '/').filter(|r| r.0 > 0 && r.1 > 0).ok_or("the file has no video stream")?;
    let rate = Rate { num, den };
    let (width, height) = (s["width"].as_u64().unwrap_or(0) as u32, s["height"].as_u64().unwrap_or(0) as u32);
    let sar = ratio(&s["sample_aspect_ratio"], ':').filter(|r| r.0 > 0 && r.1 > 0).map_or(1.0, |r| r.0 as f64 / r.1 as f64);
    let tc = s["tags"]["timecode"].as_str().or(j["format"]["tags"]["timecode"].as_str());
    let start_tc = tc.and_then(|t| parse_tc(t, rate, false)).unwrap_or(0);
    Ok(VideoInfo { rate, start_tc, width, height, aspect: width as f64 * sar / height.max(1) as f64 })
}

/// Writes frame number `frame` (0 = the file's first frame) as PNG with square pixels.
pub fn grab(file: &Path, frame: i64, rate: Rate, out: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(out); // a stale still must not pass as this one
    // ffmpeg keeps the first frame at or after -ss: aim a quarter frame early so rounding never skips it
    let t = ((frame as f64 - 0.25) / rate.fps()).max(0.0);
    let res = Command::new(tool("ffmpeg"))
        .args(["-v", "error", "-y", "-ss", &format!("{t:.6}"), "-i"])
        .arg(file)
        .args(["-frames:v", "1", "-vf", "scale=trunc(iw*sar/2)*2:ih,setsar=1", "-pix_fmt", "rgb24", "-f", "image2", "-c:v", "png"])
        .arg(out)
        .output()
        .map_err(|e| format!("ffmpeg could not start: {e}"))?;
    if !res.status.success() || !out.is_file() {
        return Err(format!("frame {frame} could not be read: {}", String::from_utf8_lossy(&res.stderr).trim()));
    }
    Ok(())
}

/// Finds the cuts of `file` in the picture (ffmpeg scdet, picture scaled down for speed): the first frame of every
/// shot (0 = the file's first frame) and, last, the frame count. `progress(frames read, frames in total)`; None when
/// `go` says stop. Takes about as long as decoding the whole file.
pub fn detect_cuts(file: &Path, rate: Rate, go: &dyn Fn() -> bool, progress: &dyn Fn(i64, i64)) -> Result<Option<Vec<i64>>, String> {
    use std::io::{BufRead, BufReader, Read};
    let total = Command::new(tool("ffprobe"))
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"])
        .arg(file)
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok())
        .map_or(0, |d| (d * rate.fps()).round() as i64);
    let mut child = Command::new(tool("ffmpeg"))
        .args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "info", "-i"])
        .arg(file)
        .args(["-an", "-sn", "-dn", "-vf", "scale=320:-2,scdet=threshold=10", "-f", "null", "-", "-progress", "pipe:1"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg could not start: {e}"))?;
    let mut err = child.stderr.take().expect("piped");
    let log = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let mut frames = 0;
    for line in BufReader::new(child.stdout.take().expect("piped")).lines() {
        let Ok(line) = line else { break };
        if !go() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        if let Some(n) = line.strip_prefix("frame=").and_then(|n| n.trim().parse().ok()) {
            frames = n;
            progress(n, total.max(n));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let log = log.join().unwrap_or_default();
    if !status.success() {
        return Err(format!("the picture could not be analysed: {}", log.lines().last().unwrap_or("").trim()));
    }
    if frames == 0 {
        return Ok(Some(vec![]));
    }
    Ok(Some(cut_frames(&log, rate, frames)))
}

/// The scdet lines of ffmpeg's log ("lavfi.scd.time: 3.04") → 0, the first frame of each further shot, the frame count.
fn cut_frames(log: &str, rate: Rate, frames: i64) -> Vec<i64> {
    let mut cuts: Vec<i64> = log
        .lines()
        .filter_map(|l| l.split("lavfi.scd.time:").nth(1)?.trim().parse::<f64>().ok())
        .map(|t| (t * rate.fps()).round() as i64)
        .filter(|&f| f > 0 && f < frames)
        .collect();
    cuts.insert(0, 0);
    cuts.push(frames);
    cuts.dedup();
    cuts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scdet_log_gives_shot_starts() {
        let log = "Stream #0:0: Video\n[Parsed_scdet_1 @ 0x6] lavfi.scd.score: 28.084, lavfi.scd.time: 3\n[Parsed_scdet_1 @ 0x6] lavfi.scd.score: 20.5, lavfi.scd.time: 5.04\n";
        assert_eq!(cut_frames(log, Rate { num: 25, den: 1 }, 175), vec![0, 75, 126, 175]);
        assert_eq!(cut_frames("", Rate { num: 25, den: 1 }, 50), vec![0, 50]);
    }
}
