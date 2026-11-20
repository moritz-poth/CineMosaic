//! Final Cut Pro 7 XML (xmeml), as Premiere Pro and DaVinci Resolve export it.

use crate::timeline::{Item, Rate, Timeline, Track};
use roxmltree::{Document, Node};

fn child<'a>(n: Node<'a, 'a>, tag: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn text<'a>(n: Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    child(n, tag).and_then(|c| c.text()).map(str::trim)
}

fn num(n: Node, tag: &str) -> Option<i64> {
    text(n, tag)?.parse().ok()
}

fn rate_of(n: Node) -> Option<Rate> {
    let r = child(n, "rate")?;
    Some(Rate::from_timebase(num(r, "timebase")? as u32, text(r, "ntsc").is_some_and(|v| v.eq_ignore_ascii_case("true"))))
}

pub fn parse(xml: &str) -> Result<Timeline, String> {
    let doc = Document::parse(xml).map_err(|e| format!("not a readable XML file: {e}"))?;
    let seq = doc.descendants().find(|n| n.has_tag_name("sequence")).ok_or("no sequence in the XML")?;
    let rate = rate_of(seq).ok_or("the sequence has no frame rate")?;
    // the start timecode as a frame count of the sequence's rate
    let tc_start = child(seq, "timecode").and_then(|t| num(t, "frame")).unwrap_or(0);
    let video = child(seq, "media").and_then(|m| child(m, "video")).ok_or("the sequence has no video")?;
    let mut tracks = Vec::new();
    for track in video.children().filter(|c| c.has_tag_name("track")) {
        if text(track, "enabled").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
            tracks.push(Track::default()); // keep the numbering
            continue;
        }
        let elems: Vec<Node> = track.children().filter(|c| c.is_element()).collect();
        let mut items = Vec::new();
        for (k, e) in elems.iter().enumerate() {
            if !e.has_tag_name("clipitem") || text(*e, "enabled").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
                continue; // generators (titles, slugs) count as see-through
            }
            // -1 = the edge lies inside a transition: cut in the middle of it
            let mid = |t: &Node| Some((num(*t, "start")? + num(*t, "end")?) / 2);
            let mut start = num(*e, "start").ok_or("clip without start")?;
            if start < 0 {
                start = elems[..k].iter().rev().find(|t| t.has_tag_name("transitionitem")).and_then(mid).ok_or("clip start in an unknown transition")?;
            }
            let mut end = num(*e, "end").ok_or("clip without end")?;
            if end < 0 {
                end = elems[k + 1..].iter().find(|t| t.has_tag_name("transitionitem")).and_then(mid).ok_or("clip end in an unknown transition")?;
            }
            let name = text(*e, "name").unwrap_or_default().to_string();
            items.push(Item { name, start: tc_start + start, end: tc_start + end, key: 0 });
        }
        tracks.push(Track { items });
    }
    let name = text(seq, "name").unwrap_or_default().to_string();
    Ok(Timeline { name, rate, tracks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tracks_transitions_and_start_timecode() {
        let xml = r#"<?xml version="1.0"?><xmeml version="4"><sequence><name>Cut</name>
<rate><timebase>24</timebase><ntsc>TRUE</ntsc></rate>
<timecode><frame>86400</frame></timecode>
<media><video>
 <track>
  <clipitem><name>A</name><start>0</start><end>-1</end></clipitem>
  <transitionitem><start>40</start><end>60</end></transitionitem>
  <clipitem><name>B</name><start>-1</start><end>100</end></clipitem>
  <generatoritem><name>Slug</name><start>100</start><end>120</end></generatoritem>
 </track>
 <track><clipitem><name>Title</name><start>10</start><end>20</end><enabled>FALSE</enabled></clipitem></track>
</video></media></sequence></xmeml>"#;
        let t = parse(xml).unwrap();
        assert_eq!(t.rate, Rate { num: 24000, den: 1001 });
        let got: Vec<(String, i64, i64)> = t.shots().into_iter().map(|s| (s.name, s.start, s.end)).collect();
        assert_eq!(got, [("A".into(), 86400, 86450), ("B".into(), 86450, 86500)]);
    }
}
