//! AAF (Media Composer, Premiere, Resolve) as a cut list: the top-level composition's picture tracks.
//!
//! An AAF file is a Microsoft compound file. Every object is a storage whose CLSID names its class; its
//! "properties" stream holds (pid, stored form, size) entries followed by the values. Strong references name a
//! child storage; vectors and sets of them name an "<name> index" stream listing the children "<name>{key}".
//! Weak references (e.g. a component's data definition) carry the key of the object they point at.
//! Layout from the AAF object specification, checked against pyaaf2 (MIT) as a reference – no code of it is used.

use crate::timeline::{Item, Rate, Timeline, Track};
use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::Path;

const SF_STRONG: u16 = 0x22;
const SF_STRONG_VECTOR: u16 = 0x32;
const SF_STRONG_SET: u16 = 0x3A;

// property ids
const HEADER_CONTENT: u16 = 0x3B03;
const CONTENT_MOBS: u16 = 0x1901;
const MOB_ID: u16 = 0x4401;
const MOB_NAME: u16 = 0x4402;
const MOB_SLOTS: u16 = 0x4403;
const MOB_USAGE: u16 = 0x4408;
const SLOT_SEGMENT: u16 = 0x4803;
const SLOT_TRACK: u16 = 0x4804;
const SLOT_RATE: u16 = 0x4B01;
const COMP_DATA_DEF: u16 = 0x0201;
const COMP_LENGTH: u16 = 0x0202;
const SEQ_COMPONENTS: u16 = 0x1001;
const SOURCE_ID: u16 = 0x1101;
const TRANSITION_CUT: u16 = 0x1802;
const OP_INPUTS: u16 = 0x0B02;
const SELECTED: u16 = 0x0F01;
const CHOICES: u16 = 0x0501;
const NESTED_SLOTS: u16 = 0x0C01;
const TC_START: u16 = 0x1501;

// class ids (as the storages' CLSIDs read them)
const SEQUENCE: &str = "0d010101-0101-0f00-060e-2b3402060101";
const FILLER: &str = "0d010101-0101-0900-060e-2b3402060101";
const TRANSITION: &str = "0d010101-0101-1700-060e-2b3402060101";
const TIMECODE: &str = "0d010101-0101-1400-060e-2b3402060101";
const COMPOSITION_MOB: &str = "0d010101-0101-3500-060e-2b3402060101";
// data definitions and usage codes (AUIDs)
const PICTURE: [&str; 3] = ["01030202-0100-0000-060e-2b3404010101", "6f3c8ce1-6cef-11d2-807d-006008143e6f", "05cba732-1daa-11d3-80ad-006008143e6f"];
const TOP_LEVEL: &str = "0d010102-0101-0700-060e-2b3404010101";

/// One object: its storage path, class and raw properties (stored form, value).
struct Obj {
    path: String,
    class: String,
    props: HashMap<u16, (u16, Vec<u8>)>,
}

/// AUID stored little-endian (first three fields) → its text form.
fn auid(b: &[u8]) -> Option<String> {
    let b: [u8; 16] = b.get(..16)?.try_into().ok()?;
    Some(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{}",
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        b[8],
        b[9],
        b[10..].iter().map(|x| format!("{x:02x}")).collect::<String>()
    ))
}

fn utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0).collect();
    String::from_utf16_lossy(&u)
}

struct File<F> {
    cf: cfb::CompoundFile<F>,
}

impl<F: Read + Seek> File<F> {
    fn read(&mut self, path: &str) -> Result<Obj, String> {
        let class = self.cf.entry(path).map_err(|e| format!("{path}: {e}"))?.clsid().to_string();
        let mut data = vec![];
        self.cf.open_stream(format!("{path}/properties")).and_then(|mut s| s.read_to_end(&mut data)).map_err(|e| format!("{path}: {e}"))?;
        if data.len() < 4 || data[0] != 0x4C {
            return Err(format!("{path}: unsupported property stream"));
        }
        let count = u16::from_le_bytes([data[2], data[3]]) as usize;
        let mut props = HashMap::new();
        let mut at = 4 + 6 * count;
        for i in 0..count {
            let e = data.get(4 + 6 * i..10 + 6 * i).ok_or("property table ends too early")?;
            let (pid, form, size) = (u16::from_le_bytes([e[0], e[1]]), u16::from_le_bytes([e[2], e[3]]), u16::from_le_bytes([e[4], e[5]]) as usize);
            let value = data.get(at..at + size).ok_or("property value ends too early")?.to_vec();
            at += size;
            props.insert(pid, (form, value));
        }
        Ok(Obj { path: path.to_string(), class, props })
    }

    fn strong(&mut self, o: &Obj, pid: u16) -> Option<Obj> {
        match o.props.get(&pid) {
            Some((SF_STRONG, v)) => self.read(&format!("{}/{}", o.path.trim_end_matches('/'), utf16(v))).ok(),
            _ => None,
        }
    }

    /// Children of a strong reference vector or set, in order.
    fn strong_list(&mut self, o: &Obj, pid: u16) -> Vec<Obj> {
        let Some((form, v)) = o.props.get(&pid) else { return vec![] };
        if *form != SF_STRONG_VECTOR && *form != SF_STRONG_SET {
            return vec![];
        }
        let name = utf16(v);
        let mut index = vec![];
        if self.cf.open_stream(format!("{}/{name} index", o.path)).and_then(|mut s| s.read_to_end(&mut index)).is_err() || index.len() < 12 {
            return vec![];
        }
        let u32_at = |p: usize| index.get(p..p + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let count = u32_at(0).unwrap_or(0) as usize;
        let keys: Vec<u32> = if *form == SF_STRONG_VECTOR {
            (0..count).filter_map(|i| u32_at(12 + 4 * i)).collect()
        } else {
            // set: key pid (u16) and key size (u8), then per entry local key, reference count, key
            let size = *index.get(14).unwrap_or(&16) as usize;
            (0..count).filter_map(|i| u32_at(15 + i * (8 + size))).collect()
        };
        keys.into_iter().filter_map(|k| self.read(&format!("{}/{name}{{{k:x}}}", o.path)).ok()).collect()
    }
}

fn i64_of(o: &Obj, pid: u16) -> Option<i64> {
    o.props.get(&pid).and_then(|(_, v)| v.get(..8)).map(|b| i64::from_le_bytes(b.try_into().unwrap()))
}

fn u32_of(o: &Obj, pid: u16) -> Option<u32> {
    o.props.get(&pid).and_then(|(_, v)| v.get(..4)).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Key of a weak reference: u16 index, u16 key pid, u8 key size, key.
fn weak_key(o: &Obj, pid: u16) -> Option<String> {
    o.props.get(&pid).and_then(|(_, v)| auid(v.get(5..)?))
}

pub fn parse_file(path: &Path) -> Result<Timeline, String> {
    let cf = cfb::open(path).map_err(|e| format!("not a readable AAF file: {e}"))?;
    parse(cf)
}

pub fn parse<F: Read + Seek>(cf: cfb::CompoundFile<F>) -> Result<Timeline, String> {
    let mut f = File { cf };
    // the root object's property 0x0002 names the header ("Header-2" in every file seen)
    let header = match f.read("/").ok().and_then(|root| f.strong(&root, 0x0002)) {
        Some(h) => h,
        None => f.read("/Header-2")?,
    };
    let content = f.strong(&header, HEADER_CONTENT).ok_or("the AAF file has no content")?;
    let mobs = f.strong_list(&content, CONTENT_MOBS);
    let names: HashMap<Vec<u8>, String> = mobs
        .iter()
        .filter_map(|m| Some((m.props.get(&MOB_ID)?.1.clone(), utf16(&m.props.get(&MOB_NAME)?.1))))
        .collect();
    // the sequence: the top-level composition, else the first composition
    let comps: Vec<&Obj> = mobs.iter().filter(|m| m.class == COMPOSITION_MOB).collect();
    let seq = comps
        .iter()
        .find(|m| m.props.get(&MOB_USAGE).and_then(|(_, v)| auid(v)).as_deref() == Some(TOP_LEVEL))
        .or(comps.first())
        .ok_or("the AAF file holds no sequence")?;
    let name = seq.props.get(&MOB_NAME).map(|(_, v)| utf16(v)).unwrap_or_default();

    let mut rate = None;
    let mut start_tc = 0;
    let mut pictures: Vec<(u32, Obj)> = vec![];
    for (n, slot) in f.strong_list(seq, MOB_SLOTS).into_iter().enumerate() {
        let Some(segment) = f.strong(&slot, SLOT_SEGMENT) else { continue };
        let slot_rate = slot.props.get(&SLOT_RATE).and_then(|(_, v)| {
            let num = i32::from_le_bytes(v.get(..4)?.try_into().ok()?);
            let den = i32::from_le_bytes(v.get(4..8)?.try_into().ok()?);
            (num > 0 && den > 0).then(|| num as f64 / den as f64)
        });
        // timecode track: its first timecode component gives the start
        let tc = if segment.class == TIMECODE {
            i64_of(&segment, TC_START)
        } else if segment.class == SEQUENCE {
            f.strong_list(&segment, SEQ_COMPONENTS).iter().find(|c| c.class == TIMECODE).and_then(|c| i64_of(c, TC_START))
        } else {
            None
        };
        if let Some(t) = tc {
            start_tc = t;
            continue;
        }
        if weak_key(&segment, COMP_DATA_DEF).is_some_and(|d| PICTURE.contains(&d.as_str())) {
            rate = rate.or(slot_rate);
            pictures.push((u32_of(&slot, SLOT_TRACK).unwrap_or(n as u32 + 1), segment));
        }
    }
    pictures.sort_by_key(|p| p.0);
    let mut tracks = vec![];
    for (_, segment) in &pictures {
        let parts = if segment.class == SEQUENCE { f.strong_list(segment, SEQ_COMPONENTS) } else { vec![f.read(&segment.path)?] };
        // positions: a transition overlaps its neighbours and cuts them at its cut point
        let mut items: Vec<Item> = vec![];
        let (mut pos, mut cut) = (0i64, None);
        let mut last_visible = false;
        for c in &parts {
            let len = i64_of(c, COMP_LENGTH).unwrap_or(0);
            if c.class == TRANSITION {
                pos -= len;
                let at = pos + i64_of(c, TRANSITION_CUT).unwrap_or(len / 2).clamp(0, len);
                if last_visible && let Some(prev) = items.last_mut() {
                    prev.end = start_tc + at;
                }
                cut = Some(at);
                continue;
            }
            let start = cut.take().unwrap_or(pos);
            pos += len;
            last_visible = c.class != FILLER && pos > start;
            if last_visible {
                items.push(Item { name: clip_name(&mut f, c, &names, 0), start: start_tc + start, end: start_tc + pos, key: 0 });
            }
        }
        tracks.push(Track { items });
    }
    Ok(Timeline { name, rate: Rate::from_fps(rate.unwrap_or(25.0)), tracks })
}

/// Name of the clip a component shows: the mob of the first source clip reached through effects,
/// multicam selections and nests.
fn clip_name<F: Read + Seek>(f: &mut File<F>, c: &Obj, names: &HashMap<Vec<u8>, String>, depth: u32) -> String {
    if depth > 16 {
        return String::new();
    }
    if let Some((_, id)) = c.props.get(&SOURCE_ID) {
        return names.get(id).cloned().unwrap_or_default();
    }
    let mut next: Vec<Obj> = f.strong(c, SELECTED).into_iter().collect();
    for pid in [OP_INPUTS, CHOICES, SEQ_COMPONENTS] {
        next.extend(f.strong_list(c, pid));
    }
    for slot in f.strong_list(c, NESTED_SLOTS) {
        next.extend(f.strong(&slot, SLOT_SEGMENT));
    }
    next.iter().map(|n| clip_name(f, n, names, depth + 1)).find(|n| !n.is_empty()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    /// Writes AAF objects into a compound file the way the format lays them out.
    struct W {
        cf: cfb::CompoundFile<Cursor<Vec<u8>>>,
    }

    enum V<'a> {
        Data(Vec<u8>),
        Strong(&'a str),
        Vector(&'a str, usize),
        Set(&'a str, usize),
    }

    fn u16le(s: &str) -> Vec<u8> {
        s.encode_utf16().chain([0]).flat_map(|c| c.to_le_bytes()).collect()
    }

    fn auid_le(s: &str) -> Vec<u8> {
        let u = uuid::Uuid::parse_str(s).unwrap();
        u.to_bytes_le().to_vec()
    }

    impl W {
        fn obj(&mut self, path: &str, class: &str, props: Vec<(u16, V)>) {
            self.cf.create_storage(path).unwrap();
            self.cf.set_storage_clsid(path, uuid::Uuid::parse_str(class).unwrap()).unwrap();
            let mut head = vec![0x4C, 0x20];
            head.extend((props.len() as u16).to_le_bytes());
            let mut values = vec![];
            for (pid, v) in &props {
                let (form, data) = match v {
                    V::Data(d) => (0x82u16, d.clone()),
                    V::Strong(n) => (SF_STRONG, u16le(n)),
                    V::Vector(n, count) => {
                        let mut idx = vec![];
                        idx.extend((*count as u32).to_le_bytes());
                        idx.extend((*count as u32).to_le_bytes());
                        idx.extend(u32::MAX.to_le_bytes());
                        for k in 0..*count as u32 {
                            idx.extend(k.to_le_bytes());
                        }
                        self.cf.create_stream(format!("{path}/{n} index")).unwrap().write_all(&idx).unwrap();
                        (SF_STRONG_VECTOR, u16le(n))
                    }
                    V::Set(n, count) => {
                        let mut idx = vec![];
                        idx.extend((*count as u32).to_le_bytes());
                        idx.extend((*count as u32).to_le_bytes());
                        idx.extend(u32::MAX.to_le_bytes());
                        idx.extend(MOB_ID.to_le_bytes());
                        idx.push(32);
                        for k in 0..*count as u32 {
                            idx.extend(k.to_le_bytes());
                            idx.extend(1u32.to_le_bytes());
                            idx.extend([k as u8; 32]);
                        }
                        self.cf.create_stream(format!("{path}/{n} index")).unwrap().write_all(&idx).unwrap();
                        (SF_STRONG_SET, u16le(n))
                    }
                };
                head.extend(pid.to_le_bytes());
                head.extend(form.to_le_bytes());
                head.extend((data.len() as u16).to_le_bytes());
                values.extend(data);
            }
            head.extend(values);
            self.cf.create_stream(format!("{path}/properties")).unwrap().write_all(&head).unwrap();
        }
    }

    fn len(n: i64) -> (u16, V<'static>) {
        (COMP_LENGTH, V::Data(n.to_le_bytes().to_vec()))
    }
    fn picture() -> (u16, V<'static>) {
        let mut v = vec![0, 0, 0, 0, 16];
        v.extend(auid_le(PICTURE[0]));
        (COMP_DATA_DEF, V::Data(v))
    }

    #[test]
    fn reads_the_top_level_sequence() {
        let mut w = W { cf: cfb::CompoundFile::create(Cursor::new(vec![])).unwrap() };
        const SOURCE_CLIP: &str = "0d010101-0101-1100-060e-2b3402060101";
        const MASTER: &str = "0d010101-0101-3600-060e-2b3402060101";
        const SLOT: &str = "0d010101-0101-3b00-060e-2b3402060101";
        const OPGROUP: &str = "0d010101-0101-0a00-060e-2b3402060101";
        w.obj("/Header-2", "0d010101-0101-2f00-060e-2b3402060101", vec![(HEADER_CONTENT, V::Strong("Content-3b03"))]);
        // root object: property 0x0002 = the header
        let mut root = vec![0x4C, 0x20, 1, 0];
        let name = u16le("Header-2");
        root.extend(2u16.to_le_bytes());
        root.extend(SF_STRONG.to_le_bytes());
        root.extend((name.len() as u16).to_le_bytes());
        root.extend(name);
        w.cf.create_stream("/properties").unwrap().write_all(&root).unwrap();
        let c = "/Header-2/Content-3b03";
        w.obj(c, "0d010101-0101-1800-060e-2b3402060101", vec![(CONTENT_MOBS, V::Set("Mobs-1901", 3))]);
        // mob 0: master "Cam A", mob 1: master "Cam B", mob 2: the sequence
        let id = |k: u8| V::Data(vec![k; 32]);
        w.obj(&format!("{c}/Mobs-1901{{0}}"), MASTER, vec![(MOB_ID, id(0)), (MOB_NAME, V::Data(u16le("Cam A")))]);
        w.obj(&format!("{c}/Mobs-1901{{1}}"), MASTER, vec![(MOB_ID, id(1)), (MOB_NAME, V::Data(u16le("Cam B")))]);
        let m = format!("{c}/Mobs-1901{{2}}");
        w.obj(&m, COMPOSITION_MOB, vec![(MOB_ID, id(2)), (MOB_NAME, V::Data(u16le("Cut 3"))), (MOB_USAGE, V::Data(auid_le(TOP_LEVEL))), (MOB_SLOTS, V::Vector("Slots-4403", 3))]);
        let rate = || (SLOT_RATE, V::Data([25i32.to_le_bytes(), 1i32.to_le_bytes()].concat()));
        // slot 0: timecode 10:00:00:00
        w.obj(&format!("{m}/Slots-4403{{0}}"), SLOT, vec![rate(), (SLOT_SEGMENT, V::Strong("Segment-4803"))]);
        w.obj(&format!("{m}/Slots-4403{{0}}/Segment-4803"), TIMECODE, vec![len(1000), (TC_START, V::Data(900_000i64.to_le_bytes().to_vec()))]);
        // slot 1 = V1: A 100, dissolve 20 cut at 5, filler 30 … ; slot 2 = V2: filler 40, effect over B 20
        let v1 = format!("{m}/Slots-4403{{1}}");
        w.obj(&v1, SLOT, vec![rate(), (SLOT_TRACK, V::Data(1u32.to_le_bytes().to_vec())), (SLOT_SEGMENT, V::Strong("Segment-4803"))]);
        let s1 = format!("{v1}/Segment-4803");
        w.obj(&s1, SEQUENCE, vec![picture(), len(170), (SEQ_COMPONENTS, V::Vector("Components-1001", 4))]);
        w.obj(&format!("{s1}/Components-1001{{0}}"), SOURCE_CLIP, vec![picture(), len(100), (SOURCE_ID, id(0))]);
        w.obj(&format!("{s1}/Components-1001{{1}}"), TRANSITION, vec![picture(), len(20), (TRANSITION_CUT, V::Data(5i64.to_le_bytes().to_vec()))]);
        w.obj(&format!("{s1}/Components-1001{{2}}"), SOURCE_CLIP, vec![picture(), len(60), (SOURCE_ID, id(1))]);
        w.obj(&format!("{s1}/Components-1001{{3}}"), FILLER, vec![picture(), len(30)]);
        let v2 = format!("{m}/Slots-4403{{2}}");
        w.obj(&v2, SLOT, vec![rate(), (SLOT_TRACK, V::Data(2u32.to_le_bytes().to_vec())), (SLOT_SEGMENT, V::Strong("Segment-4803"))]);
        let s2 = format!("{v2}/Segment-4803");
        w.obj(&s2, SEQUENCE, vec![picture(), len(60), (SEQ_COMPONENTS, V::Vector("Components-1001", 2))]);
        w.obj(&format!("{s2}/Components-1001{{0}}"), FILLER, vec![picture(), len(40)]);
        let fx = format!("{s2}/Components-1001{{1}}");
        w.obj(&fx, OPGROUP, vec![picture(), len(20), (OP_INPUTS, V::Vector("InputSegments-0b02", 1))]);
        w.obj(&format!("{fx}/InputSegments-0b02{{0}}"), SOURCE_CLIP, vec![picture(), len(20), (SOURCE_ID, id(1))]);

        let cf = cfb::CompoundFile::open(Cursor::new(w.cf.into_inner().into_inner())).unwrap();
        let t = parse(cf).unwrap();
        assert_eq!((t.name.as_str(), t.rate), ("Cut 3", Rate { num: 25, den: 1 }));
        let s = 900_000;
        let got: Vec<(String, i64, i64, usize)> = t.shots().into_iter().map(|x| (x.name, x.start - s, x.end - s, x.track)).collect();
        // V1: A 0–85 (dissolve 80–100 cut at 85), B 85–140, filler; V2: effect over B at 40–60 on top
        assert_eq!(
            got,
            [("Cam A".into(), 0, 40, 0), ("Cam B".into(), 40, 60, 1), ("Cam A".into(), 60, 85, 0), ("Cam B".into(), 85, 140, 0)]
        );
    }
}
