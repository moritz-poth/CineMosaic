//! Bin mode: a sequence in a Media Composer bin → the timeline of its picture tracks, and for every shot the
//! media file and frame its middle shows.

use crate::avb::{Bin, Hit, MobId, MobKind};
use crate::timeline::{Item, Pick, Rate, Shot, Timeline, Track};
use crate::{Job, media};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub struct BinCut {
    /// the chosen sequence, or all of them in name order
    pub timelines: Vec<Timeline>,
    bin: Bin,
    /// other bins of the project, read once a sequence uses a clip from them
    others: Vec<Bin>,
    /// per item key: (component, where it starts in the sequence)
    keys: Vec<(u32, i64)>,
}

/// The project folder of a bin: the nearest folder above it holding an .avp file.
fn project_of(bin: &Path) -> Option<PathBuf> {
    bin.ancestors().skip(1).find(|d| std::fs::read_dir(d).is_ok_and(|r| r.flatten().any(|e| e.path().extension().is_some_and(|x| x == "avp")))).map(Path::to_path_buf)
}

/// Every bin of a project except `skip` and the Attic; None when `go()` says stop.
fn project_bins(root: &Path, skip: &Path, go: &dyn Fn() -> bool) -> Option<Vec<Bin>> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if !go() {
                return None;
            }
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "Attic" {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "avb") && p != skip
                && let Ok(b) = Bin::read(&p)
            {
                out.push(b);
            }
        }
    }
    Some(out)
}

/// A linked file's path as the bin stores it → an existing file: POSIX path, "file://" URL or an old
/// "Volume:folder:file" path (then looked for under /Volumes and on the start volume).
fn linked_file(stored: &str) -> Option<PathBuf> {
    let s = stored.strip_prefix("file://").map(|u| u.trim_start_matches("localhost")).unwrap_or(stored);
    let s = percent_decode(s);
    let candidates: Vec<PathBuf> = if s.starts_with('/') {
        vec![PathBuf::from(&s)]
    } else if let Some((volume, rest)) = s.split_once(':') {
        let rest = rest.replace(':', "/");
        vec![Path::new("/Volumes").join(volume).join(&rest), Path::new("/").join(&rest)]
    } else {
        vec![]
    };
    candidates.into_iter().find(|p| p.is_file())
}

/// "%20" → " " in file URLs.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Sort key that orders numbers by value: "Reel 2" before "Reel 10".
fn natural(s: &str) -> Vec<(String, u64)> {
    let mut out = vec![];
    let mut text = String::new();
    let mut num = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            if !num.is_empty() {
                out.push((std::mem::take(&mut text), num.parse().unwrap_or(u64::MAX)));
                num.clear();
            }
            text.push(c);
        }
    }
    out.push((text, num.parse().unwrap_or(0)));
    out
}

/// The sequences a bin shows, in name order (numbers by value).
fn ordered(bin: &Bin) -> Vec<&crate::avb::Mob> {
    let mut seqs = bin.sequences();
    seqs.sort_by_key(|m| natural(&m.name));
    seqs
}

/// Names of the sequences a bin shows, in name order; `BinCut::open` takes indices into this list.
pub fn sequences(bin: &Bin) -> Vec<String> {
    ordered(bin).iter().map(|m| m.name.clone()).collect()
}

impl BinCut {
    /// Sequence number `index` (of `sequences`) of the bin at `path`, None = every sequence of the bin one after the
    /// other. Reads the project's other bins when a sequence uses clips from them; `go()` false = cancelled (Err).
    /// `chosen`: indices of the sequences to show, in their order (None = all).
    pub fn open(path: &Path, bin: Bin, chosen: Option<&[usize]>, go: &dyn Fn() -> bool) -> Result<BinCut, String> {
        let mut cut = BinCut { timelines: vec![], bin, others: vec![], keys: vec![] };
        cut.build(chosen)?;
        // clips from other bins: read the project's bins once and build again
        if cut.has_missing()
            && let Some(root) = project_of(path)
        {
            cut.others = project_bins(&root, path, go).ok_or("cancelled")?;
            cut.build(chosen)?;
        }
        Ok(cut)
    }

    /// Every shot of every sequence, in order.
    pub fn shots(&self) -> Vec<Shot> {
        self.timelines.iter().flat_map(|t| t.shots()).collect()
    }

    /// Frame rate of the (first) sequence.
    pub fn rate(&self) -> Rate {
        self.timelines.first().map_or(Rate { num: 25, den: 1 }, |t| t.rate)
    }

    fn has_missing(&self) -> bool {
        self.timelines.iter().flat_map(|t| &t.tracks).flat_map(|t| &t.items).any(|i| matches!(self.hit(i.key, i.start, i.start + (i.end - i.start) / 2), Hit::Missing { .. }))
    }

    fn build(&mut self, chosen: Option<&[usize]>) -> Result<(), String> {
        let all = ordered(&self.bin);
        let chosen: Vec<&crate::avb::Mob> = match chosen {
            Some(list) => list.iter().map(|&i| all.get(i).copied().ok_or("the bin has no such sequence")).collect::<Result<_, _>>()?,
            None => all,
        };
        let mut keys = vec![];
        let mut timelines = vec![];
        for seq in chosen {
            timelines.push(self.timeline_of(seq, &mut keys));
        }
        self.timelines = timelines;
        self.keys = keys;
        Ok(())
    }

    /// The picture tracks of one sequence; its items get keys at the end of `keys`.
    fn timeline_of(&self, seq: &crate::avb::Mob, keys: &mut Vec<(u32, i64)>) -> Timeline {
        debug_assert!(seq.kind == MobKind::Composition);
        let mut pictures: Vec<(u16, u32)> =
            seq.tracks.iter().filter(|(_, c)| self.bin.comps.get(c).is_some_and(|c| c.media == 1)).map(|(n, c)| (n.unwrap_or(0), *c)).collect();
        pictures.sort();
        let rate = pictures.first().and_then(|(_, c)| self.bin.comps.get(c)).map_or(25.0, |c| c.rate);
        // positions in record timecode, like the cut-list readers
        let tc = self.bin.start_tc(seq);
        let mut tracks = vec![];
        for (_, comp) in &pictures {
            let mut items = vec![];
            for (start, end, part, raw) in self.bin.pieces(*comp) {
                // pieces showing nothing (filler, empty effects) let the tracks below show through
                let name = match self.bin.resolve(part, start + (end - start) / 2 - raw, &self.others) {
                    Hit::Empty => continue,
                    Hit::Media { clip, .. } | Hit::Missing { clip } | Hit::Offline { clip } => clip,
                };
                items.push(Item { name, start: tc + start, end: tc + end, key: keys.len() });
                keys.push((part, tc + raw));
            }
            tracks.push(Track { items });
        }
        Timeline { name: seq.name.clone(), rate: Rate::from_fps(rate), tracks }
    }

    /// What record frame `at` of the item with `key` shows.
    fn hit(&self, key: usize, _item_start: i64, at: i64) -> Hit {
        let (part, raw) = self.keys[key];
        self.bin.resolve(part, at - raw, &self.others)
    }

    /// Each picked still: its frame in the media file, found on `volumes`.
    pub fn jobs(&self, shots: &[Shot], picks: &[Pick], volumes: &[PathBuf], go: &dyn Fn() -> bool, lang: &str) -> Vec<Result<Job, String>> {
        let say = |clip: &str, t: [&str; 4]| format!("{clip}: {}", crate::pick(lang, t));
        let hits: Vec<Hit> = picks.iter().map(|&(i, part)| self.hit(shots[i].key, shots[i].item_start, shots[i].at(part))).collect();
        let wanted: HashSet<MobId> = hits.iter().filter_map(|h| if let Hit::Media { file, .. } = h { Some(*file) } else { None }).collect();
        let paths = media::locate(volumes, &wanted, go);
        hits.into_iter()
            .map(|h| match h {
                Hit::Media { file, frame, rate, clip, linked } => match (paths.get(&file), linked) {
                    (Some(p), _) => Ok(Job { file: p.clone(), frame, rate: Rate::from_fps(rate) }),
                    // linked (AMA) file: read it where Media Composer links it, if it is there
                    (None, Some((l, at))) => match linked_file(&l) {
                        Some(p) => Ok(Job { file: p, frame: at, rate: Rate::from_fps(rate) }),
                        None => Err(format!("{} ({l})", say(&clip, ["linked file not found", "verlinkte Datei nicht gefunden", "fichier lié introuvable", "no se encuentra el archivo vinculado"]))),
                    },
                    (None, None) => Err(say(&clip, ["media file not on the chosen volumes", "Mediendatei nicht auf den gewählten Volumes", "fichier média absent des volumes choisis", "el archivo de medios no está en los volúmenes elegidos"])),
                },
                Hit::Missing { clip } => Err(say(&clip, ["clip not found in the project’s bins", "Clip in keinem Bin des Projekts gefunden", "clip introuvable dans les chutiers du projet", "clip no encontrado en los bins del proyecto"])),
                Hit::Offline { clip } => Err(say(&clip, ["no media", "keine Medien", "pas de média", "sin medios"])),
                Hit::Empty => Err(crate::pick(lang, ["no picture", "kein Bild", "pas d’image", "sin imagen"]).into()),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_in_natural_order() {
        let mut v = vec!["Reel 10", "reel 2", "Reel 1 v3", "Reel 1 v12", "Alt"];
        v.sort_by_key(|s| natural(s));
        assert_eq!(v, ["Alt", "Reel 1 v3", "Reel 1 v12", "reel 2", "Reel 10"]);
    }

    #[test]
    fn linked_paths_in_every_spelling() {
        let dir = std::env::temp_dir().join(format!("cinemosaic-linked {}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("Clip 1.mov");
        std::fs::write(&f, b"x").unwrap();
        let posix = f.to_string_lossy().into_owned();
        assert_eq!(linked_file(&posix), Some(f.clone()));
        assert_eq!(linked_file(&format!("file://{}", posix.replace(' ', "%20"))), Some(f.clone()));
        assert_eq!(linked_file(&format!("file://localhost{posix}")), Some(f.clone()));
        // "Macintosh HD:private:tmp:…" style: found on the start volume
        let hfs = format!("Macintosh HD{}", posix.replace('/', ":"));
        assert_eq!(linked_file(&hfs).map(|p| p.canonicalize().unwrap()), Some(f.canonicalize().unwrap()));
        assert_eq!(linked_file("/no/such/file.mov"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
