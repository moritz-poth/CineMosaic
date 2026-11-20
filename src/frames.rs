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
