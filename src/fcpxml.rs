//! Final Cut Pro X XML (.fcpxml), also written by DaVinci Resolve. The primary storyline is V1,
//! connected clips with lane n > 0 are track n + 1; negative lanes (below) are audio and skipped.

use crate::timeline::{Item, Rate, Timeline, Track};
use roxmltree::{Document, Node};

/// "1001/24000s", "10s", "0s" → seconds as a fraction.
fn time(v: Option<&str>) -> Option<(i64, i64)> {
    let v = v?.trim().strip_suffix('s')?;
    match v.split_once('/') {
        Some((n, d)) => Some((n.parse().ok()?, d.parse().ok()?)),
        None => Some((v.parse().ok()?, 1)),
    }
}

fn frames(t: (i64, i64), rate: Rate) -> i64 {
    let (n, d) = (t.0 as i128 * rate.num as i128, t.1 as i128 * rate.den as i128);
    if d == 0 {
        return 0;
    }
    ((2 * n + d) / (2 * d)) as i64 // rounded
}

const MAX_LANE: i64 = 99;

const PICTURE: [&str; 6] = ["asset-clip", "clip", "ref-clip", "mc-clip", "sync-clip", "video"];

pub fn parse(xml: &str) -> Result<Timeline, String> {
    let doc = Document::parse(xml).map_err(|e| format!("not a readable XML file: {e}"))?;
    let seq = doc.descendants().find(|n| n.has_tag_name("sequence")).ok_or("no sequence in the FCPXML")?;
    let format = seq.attribute("format").and_then(|id| doc.descendants().find(|n| n.has_tag_name("format") && n.attribute("id") == Some(id)));
    let fd = time(format.and_then(|f| f.attribute("frameDuration"))).ok_or("the sequence has no frame rate")?;
    let (Ok(num), Ok(den)) = (u32::try_from(fd.1), u32::try_from(fd.0)) else { return Err("unreadable frame rate".into()) };
    if num == 0 || den == 0 {
        return Err("unreadable frame rate".into());
    }
    let rate = Rate { num, den };
    let spine = seq.children().find(|c| c.has_tag_name("spine")).ok_or("the sequence has no storyline")?;
    let mut tracks: Vec<Track> = vec![Track::default()];
    for e in spine.children().filter(|c| c.is_element()) {
        let (Some(off), Some(dur)) = (time(e.attribute("offset")), time(e.attribute("duration"))) else { continue };
        let pos = frames(off, rate);
        let start = time(e.attribute("start")).map(|t| frames(t, rate)).unwrap_or(0);
        if PICTURE.contains(&e.tag_name().name()) {
            tracks[0].items.push(item(e, pos, frames(dur, rate)));
        }
        // connected clips: their offset is in the parent's source time
        for c in e.children().filter(|c| PICTURE.contains(&c.tag_name().name())) {
            let lane: usize = match c.attribute("lane").and_then(|l| l.parse::<i64>().ok()) {
                // more lanes than any real cut has: a broken or crafted file, skip
                Some(l) if (1..=MAX_LANE).contains(&l) => l as usize,
                _ => continue,
            };
            let (Some(coff), Some(cdur)) = (time(c.attribute("offset")), time(c.attribute("duration"))) else { continue };
            if tracks.len() <= lane {
                tracks.resize(lane + 1, Track::default());
            }
            tracks[lane].items.push(item(c, pos + frames(coff, rate) - start, frames(cdur, rate)));
        }
    }
    let name = seq.parent().and_then(|p| p.attribute("name")).unwrap_or_default().to_string();
    Ok(Timeline { name, rate, tracks })
}

fn item(e: Node, pos: i64, dur: i64) -> Item {
    Item { name: e.attribute("name").unwrap_or_default().to_string(), start: pos, end: pos + dur, key: 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crafted_lanes_and_rates_are_refused() {
        let zero = r#"<fcpxml><resources><format id="r1" frameDuration="0/25s"/></resources><sequence format="r1"><spine/></sequence></fcpxml>"#;
        assert!(parse(zero).is_err());
        let lane = r#"<fcpxml><resources><format id="r1" frameDuration="1/25s"/></resources><sequence format="r1"><spine>
<asset-clip name="A" offset="0s" duration="1s"><asset-clip name="X" lane="2000000000" offset="0s" duration="1s"/></asset-clip></spine></sequence></fcpxml>"#;
        let t = parse(lane).unwrap();
        assert_eq!(t.tracks.len(), 1);
        let zero_dur = r#"<fcpxml><resources><format id="r1" frameDuration="1/25s"/></resources><sequence format="r1"><spine><asset-clip name="A" offset="1/0s" duration="1s"/></spine></sequence></fcpxml>"#;
        assert!(parse(zero_dur).is_ok());
    }

    #[test]
    fn reads_spine_and_connected_clips() {
        let xml = r#"<?xml version="1.0"?><fcpxml version="1.10"><resources>
<format id="r1" frameDuration="1/25s"/></resources><library><event><project name="Cut">
<sequence format="r1" tcStart="36000s"><spine>
 <asset-clip name="A" offset="36000s" start="100s" duration="4s">
   <title name="Text" lane="1" offset="101s" duration="1s"/>
   <asset-clip name="Insert" lane="1" offset="102s" duration="1s"/>
   <asset-clip name="Sound" lane="-1" offset="100s" duration="4s"/>
 </asset-clip>
 <gap name="Gap" offset="36004s" duration="1s"/>
 <asset-clip name="B" offset="36005s" duration="51/25s"/>
</spine></sequence></project></event></library></fcpxml>"#;
        let t = parse(xml).unwrap();
        assert_eq!(t.name, "Cut");
        let got: Vec<(String, i64, i64)> = t.shots().into_iter().map(|s| (s.name, s.start, s.end)).collect();
        let s = 900_000;
        assert_eq!(
            got,
            [("A".into(), s, s + 50), ("Insert".into(), s + 50, s + 75), ("A".into(), s + 75, s + 100), ("B".into(), s + 125, s + 176)]
        );
    }
}
