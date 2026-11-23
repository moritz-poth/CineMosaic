//! Timeline model shared by every cut-list reader: video tracks of items placed in absolute record frames
//! (the sequence's start timecode included), and the "what you see" flattening into shots.

/// Frame rate as a fraction, e.g. 25/1 or 24000/1001.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    pub num: u32,
    pub den: u32,
}

impl Rate {
    pub fn fps(self) -> f64 {
        self.num as f64 / self.den as f64
    }
    /// Frames per timecode second (30 for 29.97).
    pub fn nominal(self) -> i64 {
        self.fps().round() as i64
    }
    /// From frames per second as a number (23.976 → 24000/1001).
    pub fn from_fps(fps: f64) -> Rate {
        let r = fps.round();
        if (fps - r).abs() < 0.001 { Rate { num: r as u32, den: 1 } } else { Rate { num: r as u32 * 1000, den: 1001 } }
    }
    /// From an XML timebase + NTSC flag (FCP7 style).
    pub fn from_timebase(timebase: u32, ntsc: bool) -> Rate {
        if ntsc { Rate { num: timebase * 1000, den: 1001 } } else { Rate { num: timebase, den: 1 } }
    }
}

/// Timecode "HH:MM:SS:FF" (";" or "." before the frames = drop frame) → frame count.
pub fn parse_tc(s: &str, rate: Rate, drop: bool) -> Option<i64> {
    let s = s.trim();
    let drop = drop || s.contains(';') || s.contains('.');
    let parts: Vec<i64> = s.split([':', ';', '.']).map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let [h, m, sec, f] = parts[..] else { return None };
    let nom = rate.nominal();
    let mut frames = ((h * 60 + m) * 60 + sec) * nom + f;
    if drop && nom % 30 == 0 {
        let skip = nom / 15; // 2 frames at 29.97, 4 at 59.94
        let minutes = h * 60 + m;
        frames -= skip * (minutes - minutes / 10);
    }
    Some(frames)
}

/// Frame count → "HH:MM:SS:FF" (non-drop display).
pub fn format_tc(frames: i64, rate: Rate) -> String {
    let nom = rate.nominal();
    let f = frames.rem_euclid(nom);
    let s = frames.div_euclid(nom);
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, f)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: String,
    /// absolute record frames, end exclusive
    pub start: i64,
    pub end: i64,
    /// the reader's own reference to the item (bin mode: which component), 0 otherwise
    pub key: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Track {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub struct Timeline {
    pub name: String,
    pub rate: Rate,
    /// video tracks, V1 first
    pub tracks: Vec<Track>,
}

/// One visible piece of the cut: a still is taken from its middle.
#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub name: String,
    pub start: i64,
    pub end: i64,
    /// 0 = V1
    pub track: usize,
    /// the item the shot belongs to: its key and its start (a shot can be part of an item)
    pub key: usize,
    pub item_start: i64,
}

impl Shot {
    /// Record frame of the middle of the shot.
    pub fn middle(&self) -> i64 {
        self.at(Part::Middle)
    }

    /// Record frame of `part` of the shot.
    pub fn at(&self, part: Part) -> i64 {
        let d = self.end - self.start;
        self.start
            + match part {
                Part::Middle => d / 2,
                Part::FirstThird => d / 3,
                Part::SecondThird => d * 2 / 3,
                Part::Of(k, n) => d * k as i64 / (n as i64 + 1),
            }
    }
}

/// Where in its shot a still is taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Middle,
    /// after the first third
    FirstThird,
    /// after the second third
    SecondThird,
    /// the k-th of n frames spread evenly over the shot (1 ≤ k ≤ n): more colours for a photomosaic
    Of(u8, u8),
}

/// `frames` stills of every shot, spread evenly (1 = the middle), cut order kept: the stills of a photomosaic.
pub fn spread(shots: &[Shot], frames: u32) -> Vec<Pick> {
    let n = frames.clamp(1, 5) as u8;
    (0..shots.len()).flat_map(|i| (1..=n).map(move |k| (i, if n == 1 { Part::Middle } else { Part::Of(k, n) }))).collect()
}

/// One still of the sheet: shot index and where in the shot.
pub type Pick = (usize, Part);

/// The stills for a grid of `cells`: the middle of every shot; while cells are left over, the longest shots
/// (the earlier one on a tie) give two stills instead, after the first and after the second third. Cut order is kept.
pub fn fill(shots: &[Shot], cells: usize) -> Vec<Pick> {
    let extra = cells.saturating_sub(shots.len()).min(shots.len());
    let mut longest: Vec<usize> = (0..shots.len()).collect();
    longest.sort_by_key(|&i| std::cmp::Reverse(shots[i].end - shots[i].start)); // stable: ties keep cut order
    let mut split = vec![false; shots.len()];
    for &i in &longest[..extra] {
        split[i] = true;
    }
    (0..shots.len())
        .flat_map(|i| if split[i] { vec![(i, Part::FirstThird), (i, Part::SecondThird)] } else { vec![(i, Part::Middle)] })
        .collect()
}

impl Timeline {
    /// The cut as seen: at every moment the topmost track with an item wins; neighbouring moments showing
    /// the same item form one shot. An item partly covered from above therefore gives two shots.
    pub fn shots(&self) -> Vec<Shot> {
        let mut cuts: Vec<i64> =
            self.tracks.iter().flat_map(|t| t.items.iter()).filter(|i| i.end > i.start).flat_map(|i| [i.start, i.end]).collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut shots: Vec<(usize, usize, Shot)> = Vec::new();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let top = self.tracks.iter().enumerate().rev().find_map(|(t, track)| {
                track.items.iter().position(|i| i.start <= a && a < i.end).map(|n| (t, n))
            });
            let Some((t, n)) = top else { continue };
            if let Some(last) = shots.last_mut()
                && last.0 == t
                && last.1 == n
                && last.2.end == a
            {
                last.2.end = b;
                continue;
            }
            let item = &self.tracks[t].items[n];
            shots.push((t, n, Shot { name: item.name.clone(), start: a, end: b, track: t, key: item.key, item_start: item.start }));
        }
        shots.into_iter().map(|s| s.2).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, start: i64, end: i64) -> Item {
        Item { name: name.into(), start, end, key: 0 }
    }

    #[test]
    fn timecodes() {
        let pal = Rate { num: 25, den: 1 };
        assert_eq!(parse_tc("10:00:00:00", pal, false), Some(900_000));
        assert_eq!(format_tc(900_024, pal), "10:00:00:24");
        let ntsc = Rate { num: 30000, den: 1001 };
        assert_eq!(parse_tc("00:01:00;02", ntsc, false), Some(1800)); // first frame after the dropped 00 and 01
        assert_eq!(parse_tc("00:10:00;00", ntsc, false), Some(17982));
        assert_eq!(parse_tc("01:00:00:00", ntsc, true), Some(107892));
        assert_eq!(parse_tc("bad", pal, false), None);
    }

    #[test]
    fn topmost_track_wins_and_splits_the_shot_below() {
        let t = Timeline {
            name: "t".into(),
            rate: Rate { num: 25, den: 1 },
            tracks: vec![
                Track { items: vec![item("A", 0, 100), item("B", 100, 200)] },
                Track { items: vec![item("C", 40, 60), item("D", 100, 200)] },
            ],
        };
        let got: Vec<(String, i64, i64)> = t.shots().into_iter().map(|s| (s.name, s.start, s.end)).collect();
        assert_eq!(got, [("A".into(), 0, 40), ("C".into(), 40, 60), ("A".into(), 60, 100), ("D".into(), 100, 200)]);
        assert_eq!(t.shots()[1].middle(), 50);
    }

    #[test]
    fn shot_keeps_its_item() {
        let t = Timeline {
            name: "t".into(),
            rate: Rate { num: 25, den: 1 },
            tracks: vec![
                Track { items: vec![Item { name: "A".into(), start: 0, end: 100, key: 7 }] },
                Track { items: vec![item("C", 40, 60)] },
            ],
        };
        let s = &t.shots()[2];
        assert_eq!((s.key, s.item_start, s.start, s.middle()), (7, 0, 60, 80));
    }

    #[test]
    fn longest_shots_fill_the_empty_cells_with_thirds() {
        let shot = |start, end| Shot { name: String::new(), start, end, track: 0, key: 0, item_start: start };
        // lengths 30, 90, 60, 90: two cells left over go to the two 90-frame shots, the earlier one first on the tie
        let shots = [shot(0, 30), shot(30, 120), shot(120, 180), shot(180, 270)];
        use Part::*;
        assert_eq!(fill(&shots, 6), [(0, Middle), (1, FirstThird), (1, SecondThird), (2, Middle), (3, FirstThird), (3, SecondThird)]);
        assert_eq!(fill(&shots, 5), [(0, Middle), (1, FirstThird), (1, SecondThird), (2, Middle), (3, Middle)]);
        assert_eq!(fill(&shots, 4), fill(&shots, 0));
        assert_eq!(fill(&shots, 99).len(), 8); // never more than two per shot
        assert_eq!((shots[1].at(FirstThird), shots[1].at(SecondThird), shots[1].middle()), (60, 90, 75));
    }
}
