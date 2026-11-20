//! CMX3600 EDL (as Media Composer's List Tool writes it). One video track; the frame rate is not in
//! the file, so it comes from the master file.

use crate::timeline::{Item, Rate, Timeline, Track, parse_tc};

pub fn parse(text: &str, rate: Rate) -> Result<Timeline, String> {
    let mut name = String::new();
    let mut drop = false;
    let mut items: Vec<Item> = Vec::new();
    let (mut group, mut event) = (0, String::new());
    for line in text.lines() {
        let line = line.trim();
        if let Some(t) = line.strip_prefix("TITLE:") {
            name = t.trim().to_string();
            continue;
        }
        if let Some(f) = line.strip_prefix("FCM:") {
            drop = f.trim().starts_with("DROP");
            continue;
        }
        // after a dissolve pair, FROM names the pair's first line and TO the second
        if let Some(n) = line.strip_prefix("* FROM CLIP NAME:") {
            if let Some(i) = items.get_mut(group) {
                i.name = n.trim().to_string();
            }
            continue;
        }
        if let Some(n) = line.strip_prefix("* TO CLIP NAME:") {
            if let Some(i) = items.last_mut() {
                i.name = n.trim().to_string();
            }
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 8 || !f[0].bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let track = f[2];
        if !(track.contains('V') || track == "B") {
            continue; // audio-only event
        }
        let tcs = &f[f.len() - 4..];
        let (Some(rec_in), Some(rec_out)) = (parse_tc(tcs[2], rate, drop), parse_tc(tcs[3], rate, drop)) else {
            return Err(format!("unreadable timecode in line: {line}"));
        };
        if f[0] != event {
            (group, event) = (items.len(), f[0].to_string());
        }
        // a dissolve's "from" line has the same record in and out: no picture of its own
        items.push(Item { name: f[1].to_string(), start: rec_in, end: rec_out, key: 0 });
    }
    items.retain(|i| i.end > i.start);
    if items.is_empty() {
        return Err("no video events in the EDL".into());
    }
    items.sort_by_key(|i| i.start);
    Ok(Timeline { name, rate, tracks: vec![Track { items }] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_events_names_and_dissolves() {
        let edl = "TITLE: Rough Cut\nFCM: NON-DROP FRAME\n\n\
001  A001C003 V     C        01:00:10:00 01:00:12:00 10:00:00:00 10:00:02:00\n\
* FROM CLIP NAME:  Szene 1\n\
002  A002     AA    C        02:00:00:00 02:00:05:00 10:00:00:00 10:00:05:00\n\
003  A003     V     C        03:00:00:00 03:00:00:00 10:00:02:00 10:00:02:00\n\
003  A004     V     D    025 04:00:00:00 04:00:03:00 10:00:02:00 10:00:05:00\n\
* FROM CLIP NAME: Szene 1b\n\
* TO CLIP NAME: Szene 2\n";
        let t = parse(edl, Rate { num: 25, den: 1 }).unwrap();
        assert_eq!(t.name, "Rough Cut");
        let i = &t.tracks[0].items;
        assert_eq!(i.len(), 2);
        assert_eq!((i[0].name.as_str(), i[0].start, i[0].end), ("Szene 1", 900_000, 900_050));
        assert_eq!((i[1].name.as_str(), i[1].start, i[1].end), ("Szene 2", 900_050, 900_125));
    }
}
