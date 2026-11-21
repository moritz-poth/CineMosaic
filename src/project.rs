//! Project file: a package folder "<name>.cinemosaic" holding project.json and the grabbed stills, so a sheet can be
//! opened again – with its stills even when the master, the bin or the volumes are not there.

use crate::timeline::{Part, Rate, Shot};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const EXT: &str = "cinemosaic";
const FORMAT: u32 = 1;

/// One still of the project: which shot and where in it, the file (absolute while the app works with it), or why it
/// could not be read; `job` = media file and frame it shows, so a changed cut can reuse it.
#[derive(Clone, Debug, PartialEq)]
pub struct Still {
    pub shot: usize,
    pub part: Part,
    pub file: Result<PathBuf, String>,
    pub job: Option<(PathBuf, i64)>,
}

/// Everything a project keeps. `look` is the sheet's look as the presets store it.
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    /// 0 master + cut list, 1 Media Composer bin
    pub mode: i32,
    pub master: Option<PathBuf>,
    pub list: Option<PathBuf>,
    pub bin: Option<PathBuf>,
    /// bin mode: 0 all sequences, 1.. one of them
    pub sequence: i32,
    /// volumes switched off (bin mode); new volumes are on
    pub volumes_off: Vec<PathBuf>,
    pub look: serde_json::Value,
    pub headline: String,
    /// the cut as read when saved
    pub name: String,
    pub rate: Rate,
    pub sequences: usize,
    pub source_info: String,
    pub shots: Vec<Shot>,
    pub still_aspect: Option<f64>,
    pub stills: Vec<Still>,
}

#[derive(Serialize, Deserialize)]
struct FileShot {
    name: String,
    start: i64,
    end: i64,
    track: usize,
    key: usize,
    item_start: i64,
}

#[derive(Serialize, Deserialize)]
struct FileStill {
    shot: usize,
    part: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    frame: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct FileProject {
    app: String,
    format: u32,
    mode: i32,
    master: Option<PathBuf>,
    list: Option<PathBuf>,
    bin: Option<PathBuf>,
    sequence: i32,
    volumes_off: Vec<PathBuf>,
    look: serde_json::Value,
    headline: String,
    name: String,
    rate: (u32, u32),
    sequences: usize,
    source_info: String,
    shots: Vec<FileShot>,
    still_aspect: Option<f64>,
    stills: Vec<FileStill>,
}

fn part_name(p: Part) -> &'static str {
    match p {
        Part::Middle => "middle",
        Part::FirstThird => "first-third",
        Part::SecondThird => "second-third",
    }
}

/// Writes the project to `path` (a folder ending in .cinemosaic): first next to it as "….part", then swapped in, so
/// a failed save never destroys the project that was there. The stills are copied in; returns the stills with their
/// new place inside the package (the old files may be gone after an overwrite).
pub fn save(p: &Project, path: &Path) -> Result<Vec<Still>, String> {
    let io = |e: std::io::Error| format!("{}: {e}", path.display());
    let part = path.with_extension(format!("{EXT}.part"));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(part.join("stills")).map_err(io)?;
    let mut stills = Vec::with_capacity(p.stills.len());
    let mut file_stills = Vec::with_capacity(p.stills.len());
    for (i, s) in p.stills.iter().enumerate() {
        let (file, error) = match &s.file {
            Ok(src) => {
                let rel = format!("stills/{:05}.png", i + 1);
                std::fs::copy(src, part.join(&rel)).map_err(|e| format!("{}: {e}", src.display()))?;
                (Some(rel), None)
            }
            Err(e) => (None, Some(e.clone())),
        };
        stills.push(Still { file: file.as_ref().map(|r| path.join(r)).ok_or_else(|| error.clone().unwrap_or_default()), ..s.clone() });
        file_stills.push(FileStill {
            shot: s.shot,
            part: part_name(s.part).into(),
            file,
            error,
            media: s.job.as_ref().map(|j| j.0.clone()),
            frame: s.job.as_ref().map(|j| j.1),
        });
    }
    let fp = FileProject {
        app: "CineMosaic".into(),
        format: FORMAT,
        mode: p.mode,
        master: p.master.clone(),
        list: p.list.clone(),
        bin: p.bin.clone(),
        sequence: p.sequence,
        volumes_off: p.volumes_off.clone(),
        look: p.look.clone(),
        headline: p.headline.clone(),
        name: p.name.clone(),
        rate: (p.rate.num, p.rate.den),
        sequences: p.sequences,
        source_info: p.source_info.clone(),
        shots: p.shots.iter().map(|s| FileShot { name: s.name.clone(), start: s.start, end: s.end, track: s.track, key: s.key, item_start: s.item_start }).collect(),
        still_aspect: p.still_aspect,
        stills: file_stills,
    };
    let json = serde_json::to_vec_pretty(&fp).map_err(|e| e.to_string())?;
    std::fs::write(part.join("project.json"), json).map_err(io)?;
    // swap: the old package goes aside first and is removed only when the new one is in place
    let old = path.with_extension(format!("{EXT}.old"));
    let _ = std::fs::remove_dir_all(&old);
    let had_old = path.exists();
    if had_old {
        std::fs::rename(path, &old).map_err(io)?;
    }
    if let Err(e) = std::fs::rename(&part, path) {
        if had_old {
            let _ = std::fs::rename(&old, path);
        }
        return Err(io(e));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(stills)
}

/// Reads a project package. Stills whose file is missing count as not read.
pub fn load(path: &Path) -> Result<Project, String> {
    let bytes = std::fs::read(path.join("project.json")).map_err(|e| format!("{}: {e}", path.display()))?;
    let fp: FileProject = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if fp.app != "CineMosaic" {
        return Err(format!("{}: not a CineMosaic project", path.display()));
    }
    if fp.format > FORMAT {
        return Err(format!("{}: made by a newer CineMosaic – please update", path.display()));
    }
    let shots: Vec<Shot> = fp
        .shots
        .into_iter()
        .map(|s| Shot { name: s.name, start: s.start, end: s.end, track: s.track, key: s.key, item_start: s.item_start })
        .collect();
    let stills = fp
        .stills
        .into_iter()
        .filter(|s| s.shot < shots.len())
        .filter_map(|s| {
            let part = match s.part.as_str() {
                "middle" => Part::Middle,
                "first-third" => Part::FirstThird,
                "second-third" => Part::SecondThird,
                _ => return None,
            };
            // only files inside the package
            let file = match s.file.filter(|r| r.starts_with("stills/") && !r.contains("..")) {
                Some(r) if path.join(&r).is_file() => Ok(path.join(r)),
                Some(_) => Err("still missing in the project".to_string()),
                None => Err(s.error.unwrap_or_default()),
            };
            Some(Still { shot: s.shot, part, file, job: s.media.zip(s.frame) })
        })
        .collect();
    Ok(Project {
        mode: fp.mode,
        master: fp.master,
        list: fp.list,
        bin: fp.bin,
        sequence: fp.sequence,
        volumes_off: fp.volumes_off,
        look: fp.look,
        headline: fp.headline,
        name: fp.name,
        rate: Rate { num: fp.rate.0.max(1), den: fp.rate.1.max(1) },
        sequences: fp.sequences,
        source_info: fp.source_info,
        shots,
        still_aspect: fp.still_aspect,
        stills,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_and_overwrite_keep_the_stills() {
        let dir = std::env::temp_dir().join(format!("cm-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let still = dir.join("grabbed.png");
        std::fs::write(&still, b"png").unwrap();
        let shot = Shot { name: "A".into(), start: 0, end: 90, track: 0, key: 3, item_start: 0 };
        let p = Project {
            mode: 0,
            master: Some("/m.mov".into()),
            list: Some("/c.edl".into()),
            bin: None,
            sequence: 0,
            volumes_off: vec![],
            look: serde_json::json!({"margin": "10"}),
            headline: "Title".into(),
            name: "Cut".into(),
            rate: Rate { num: 25, den: 1 },
            sequences: 1,
            source_info: "1 shot".into(),
            shots: vec![shot],
            still_aspect: Some(1.78),
            stills: vec![
                Still { shot: 0, part: Part::FirstThird, file: Ok(still.clone()), job: Some(("/m.mov".into(), 30)) },
                Still { shot: 0, part: Part::SecondThird, file: Err("no picture".into()), job: None },
            ],
        };
        let path = dir.join("Film.cinemosaic");
        let saved = save(&p, &path).unwrap();
        assert_eq!(saved[0].file, Ok(path.join("stills/00001.png")));
        let back = load(&path).unwrap();
        assert_eq!(back, Project { stills: saved.clone(), ..p.clone() });
        // saving over itself: the stills come from the package that is being replaced
        let again = save(&back, &path).unwrap();
        assert_eq!(std::fs::read(again[0].file.as_ref().unwrap()).unwrap(), b"png");
        assert!(!path.with_extension("cinemosaic.old").exists() && !path.with_extension("cinemosaic.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
