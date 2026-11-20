//! Reads Avid bins (.avb): sequences with the position of every clip, and the way from a frame of the
//! sequence down to the media file (file mob) and the frame inside it.
//!
//! Reader base from MediaFiles Check (`src/avb.rs`); format knowledge from pyavb by Mark Reid (MIT license,
//! https://github.com/markreidvfx/pyavb). A bin is a table of objects (class id + size + data) that refer to each
//! other by index. Every component starts with the same header; what follows tells clips (0x02 0x01 length) from
//! track groups (0x02 0x08 … length, tracks) – so classes this reader does not know still keep their length and
//! the positions in a sequence stay right.
use std::collections::HashMap;
use std::path::Path;

/// Avid mob ID as 32 bytes in the layout of msmFMID.pmr / MXF: 12-byte label, length, 3-byte instance,
/// 16-byte material (first three fields little-endian).
pub type MobId = [u8; 32];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MobKind {
    Composition,
    Master,
    Source,
    Other(u8),
}

#[derive(Debug, Clone)]
pub struct Mob {
    pub name: String,
    pub id: MobId,
    pub kind: MobKind,
    /// 0 = sequence among compositions, 2 = subclip among masters
    pub usage: i32,
    /// class of the media descriptor (source mobs), e.g. "CDCI"
    pub descriptor: Option<String>,
    /// path of the descriptor's file locator: where a linked (AMA) or imported file lives
    pub path: Option<String>,
    /// (track number, component object)
    pub tracks: Vec<(Option<u16>, u32)>,
}

impl Mob {
    /// Source mob of a media file (not a tape, film or import source).
    pub fn is_file(&self) -> bool {
        const FILE: [&str; 13] = ["MDFL", "MULD", "WAVE", "AIFC", "PCMA", "MPGA", "DIDD", "CDCI", "MPGI", "JPED", "RGBA", "DATD", "ANCD"];
        self.kind == MobKind::Source && self.descriptor.as_deref().is_some_and(|d| FILE.contains(&d))
    }
}

#[derive(Debug, Clone)]
pub enum Part {
    /// source clip: mob, its track, first frame used
    Source { mob: MobId, track: u16, start: i64 },
    /// filler and every other clip without a picture of its own
    Clip,
    /// timecode track: first timecode as a frame count
    Timecode(i64),
    Sequence(Vec<u32>),
    /// effect, selector (multicam), motion effect, transition …: tracks with their component (None = empty)
    Group { class: [u8; 4], tracks: Vec<Option<u32>>, selected: Option<u16>, speed: Option<(i32, i32)>, cutpoint: i64 },
}

#[derive(Debug, Clone)]
pub struct Component {
    pub part: Part,
    /// 1 picture, 2 sound, 3 timecode …
    pub media: u16,
    pub rate: f64,
    /// own length (sequences: see `Bin::length`)
    pub length: i64,
}

#[derive(Debug, Default)]
pub struct Bin {
    pub mobs: Vec<Mob>,
    /// indexes into `mobs` of what the bin shows
    pub items: Vec<usize>,
    pub comps: HashMap<u32, Component>,
    by_id: HashMap<MobId, usize>,
    /// sequence lengths, worked out once when the bin is read
    lengths: HashMap<u32, i64>,
}

type Res<T> = Result<T, String>;

struct R<'a> {
    d: &'a [u8],
    p: usize,
    le: bool,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Res<&'a [u8]> {
        let end = self.p.checked_add(n).filter(|&e| e <= self.d.len()).ok_or("object ends too early")?;
        let s = &self.d[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Res<u8> {
        Ok(self.take(1)?[0])
    }
    fn peek2(&self) -> Option<[u8; 2]> {
        self.d.get(self.p..self.p + 2).map(|b| [b[0], b[1]])
    }
    fn u16(&mut self) -> Res<u16> {
        let b: [u8; 2] = self.take(2)?.try_into().unwrap();
        Ok(if self.le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) })
    }
    fn u32(&mut self) -> Res<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(if self.le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) })
    }
    fn i32(&mut self) -> Res<i32> {
        Ok(self.u32()? as i32)
    }
    fn tag(&mut self, want: u8) -> Res<()> {
        let got = self.u8()?;
        if got == want { Ok(()) } else { Err(format!("unexpected marker {got} (expected {want}) at {}", self.p - 1)) }
    }
    fn ext(&mut self) -> Res<Option<u8>> {
        if self.d.get(self.p) == Some(&0x01) {
            self.p += 1;
            return Ok(Some(self.u8()?));
        }
        Ok(None)
    }
    fn string(&mut self) -> Res<String> {
        let n = self.u16()?;
        if n == 0xFFFF {
            return Ok(String::new());
        }
        let raw: Vec<u8> = self.take(n as usize)?.iter().copied().filter(|&b| b != 0).collect();
        Ok(match std::str::from_utf8(&raw) {
            Ok(s) if raw.is_ascii() => s.to_string(),
            _ => mac_roman(&raw),
        })
    }
    fn string_utf8(&mut self) -> Res<String> {
        let n = self.u16()?;
        if n == 0xFFFF {
            return Ok(String::new());
        }
        let raw: Vec<u8> = self.take(n as usize)?.iter().copied().filter(|&b| b != 0).collect();
        Ok(String::from_utf8(raw.clone()).unwrap_or_else(|_| mac_roman(&raw)))
    }
    fn mob_id(&mut self) -> Res<MobId> {
        let mut id = [0u8; 32];
        self.tag(65)?;
        if self.u32()? != 12 {
            return Err("unexpected mob id label length".into());
        }
        id[..12].copy_from_slice(self.take(12)?);
        for b in id.iter_mut().take(16).skip(12) {
            self.tag(68)?;
            *b = self.u8()?;
        }
        self.tag(72)?;
        let mut a: [u8; 4] = self.take(4)?.try_into().unwrap();
        self.tag(70)?;
        let mut b: [u8; 2] = self.take(2)?.try_into().unwrap();
        self.tag(70)?;
        let mut c: [u8; 2] = self.take(2)?.try_into().unwrap();
        self.tag(65)?;
        if self.u32()? != 8 {
            return Err("unexpected mob id length".into());
        }
        if !self.le {
            a.reverse();
            b.reverse();
            c.reverse();
        }
        id[16..20].copy_from_slice(&a);
        id[20..22].copy_from_slice(&b);
        id[22..24].copy_from_slice(&c);
        id[24..32].copy_from_slice(self.take(8)?);
        Ok(id)
    }
}

/// Mac Roman upper half (0x80–0xFF).
pub fn mac_roman(b: &[u8]) -> String {
    const HIGH: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»… ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";
    let high: Vec<char> = HIGH.chars().collect();
    b.iter().map(|&c| if c < 0x80 { c as char } else { high.get(c as usize - 0x80).copied().unwrap_or('?') }).collect()
}

/// Component header: name, media kind, edit rate.
fn component(r: &mut R) -> Res<(String, u16, f64)> {
    r.tag(0x02)?;
    r.tag(0x03)?;
    r.u32()?; // left bob
    r.u32()?; // right bob
    let kind = r.u16()?;
    let mantissa = r.i32()?;
    let exp = r.u16()? as i16;
    let name = r.string()?;
    r.string()?; // effect id
    r.u32()?; // attributes
    r.u32()?; // session attributes
    r.u32()?; // precomputed
    while let Some(t) = r.ext()? {
        match t {
            0x01 => {
                r.tag(72)?;
                r.u32()?; // parameter list
            }
            _ => return Err(format!("unknown component extension {t}")),
        }
    }
    Ok((name, kind, mantissa as f64 * 10f64.powi(exp as i32)))
}

/// Track group part after the component header: length and (track number, component) of every track.
/// (track number, component) of one track of a track group
type TrackSlot = (Option<u16>, Option<u32>);

fn track_group(r: &mut R) -> Res<(i64, Vec<TrackSlot>)> {
    r.tag(0x02)?;
    r.tag(0x08)?;
    r.u8()?; // mc mode
    let length = r.i32()? as i64;
    r.u32()?; // number of scalars
    let count = r.u32()?;
    if count > 10_000 {
        return Err("implausible track count".into());
    }
    let mut tracks = vec![];
    for _ in 0..count {
        let flags = r.u16()?;
        if flags & 0xFC00 != 0 {
            return Err(format!("unknown track flags {flags:#x}"));
        }
        let index = if flags & 1 != 0 { Some(r.u16()?) } else { None };
        if flags & 2 != 0 {
            r.u32()?;
        }
        if flags & (1 << 9) != 0 {
            r.u32()?;
        }
        let comp = if flags & 4 != 0 { Some(r.u32()?) } else { None };
        for bit in [3, 4] {
            if flags & (1 << bit) != 0 {
                r.u32()?;
            }
        }
        for bit in [5, 6] {
            if flags & (1 << bit) != 0 {
                r.u16()?;
            }
        }
        if flags & (1 << 7) != 0 {
            r.u32()?;
        }
        if flags & (1 << 8) != 0 {
            r.u8()?;
        }
        tracks.push((index, comp));
    }
    while let Some(t) = r.ext()? {
        match t {
            0x01 => {
                for _ in 0..count {
                    r.tag(69)?;
                    r.u16()?;
                }
            }
            _ => return Err(format!("unknown track group extension {t}")),
        }
    }
    Ok((length, tracks))
}

enum Raw {
    Comp(Component),
    Mob(Mob, u32, Component),
    /// descriptor class and its locator object
    Descriptor(String, u32),
    /// file locator path
    Locator(String),
    Nothing,
}

const DESCRIPTORS: [&[u8; 4]; 12] = [b"MULD", b"WAVE", b"AIFC", b"PCMA", b"MPGA", b"DIDD", b"CDCI", b"MPGI", b"JPED", b"RGBA", b"DATD", b"ANCD"];

fn parse_object(class: &[u8; 4], d: &[u8], le: bool) -> Res<Raw> {
    let mut r = R { d, p: 0, le };
    if class.starts_with(b"MD") || DESCRIPTORS.contains(&class) {
        // every descriptor starts with the MDES part: 0x02 0x03, mob kind, locator
        let locator = (|| -> Res<u32> {
            r.tag(0x02)?;
            r.tag(0x03)?;
            r.u8()?;
            r.u32()
        })()
        .unwrap_or(0);
        return Ok(Raw::Descriptor(String::from_utf8_lossy(class).into_owned(), locator));
    }
    if class == b"FILE" || class == b"WINF" {
        // path (old style), then the POSIX and UTF-8 paths as extensions; the newest one wins
        r.tag(0x02)?;
        r.tag(0x02)?;
        let mut path = r.string()?;
        while let Some(t) = r.ext()? {
            r.tag(76)?;
            let p = if t == 0x01 { r.string()? } else { r.string_utf8()? };
            if !p.is_empty() && t <= 0x02 {
                path = p;
            }
        }
        return Ok(Raw::Locator(path));
    }
    // only objects that start like a component are of interest
    if r.peek2() != Some([0x02, 0x03]) {
        return Ok(Raw::Nothing);
    }
    let (name, media, rate) = component(&mut r)?;
    let comp = |part, length| Component { part, media, rate, length };
    if class == b"SEQU" {
        r.tag(0x02)?;
        r.tag(0x03)?;
        let n = r.u32()?;
        let parts = (0..n).map(|_| r.u32()).collect::<Res<_>>()?;
        return Ok(Raw::Comp(comp(Part::Sequence(parts), 0)));
    }
    match r.peek2() {
        Some([0x02, 0x01]) => {
            r.p += 2;
            let length = r.u32()? as i64;
            if class == b"SCLP" {
                r.tag(0x02)?;
                r.tag(0x03)?;
                r.u32()?;
                r.u32()?; // old short mob id
                let track = r.u16()?;
                let start = r.i32()? as i64;
                let mut id = [0u8; 32];
                while let Some(t) = r.ext()? {
                    match t {
                        0x01 => id = r.mob_id()?,
                        _ => return Err(format!("unknown source clip extension {t}")),
                    }
                }
                return Ok(Raw::Comp(comp(Part::Source { mob: id, track, start }, length)));
            }
            if class == b"TCCP" {
                r.tag(0x02)?;
                r.tag(0x01)?;
                r.u32()?; // flags
                r.u16()?; // fps
                r.take(6)?;
                return Ok(Raw::Comp(comp(Part::Timecode(r.u32()? as i64), length)));
            }
            Ok(Raw::Comp(comp(Part::Clip, length)))
        }
        Some([0x02, 0x08]) => {
            let (length, tracks) = track_group(&mut r)?;
            let (mut selected, mut speed, mut cutpoint) = (None, None, 0);
            match class {
                b"CMPO" => {
                    r.tag(0x02)?;
                    r.tag(0x02)?;
                    r.u32()?;
                    r.u32()?; // old short mob id
                    r.u32()?; // last modified
                    let kind = match r.u8()? {
                        1 => MobKind::Composition,
                        2 => MobKind::Master,
                        3 => MobKind::Source,
                        k => MobKind::Other(k),
                    };
                    let usage = r.u32()? as i32;
                    let descriptor = r.u32()?;
                    let mut id = None;
                    while let Some(t) = r.ext()? {
                        match t {
                            0x01 => {
                                r.tag(71)?;
                                r.u32()?;
                            }
                            0x02 => id = Some(r.mob_id()?),
                            _ => return Err(format!("unknown mob extension {t}")),
                        }
                    }
                    let tracks: Vec<(Option<u16>, u32)> = tracks.into_iter().filter_map(|(n, c)| Some((n, c?))).collect();
                    let mob = Mob { name, id: id.ok_or("mob without mob id")?, kind, usage, descriptor: None, path: None, tracks };
                    return Ok(Raw::Mob(mob, descriptor, comp(Part::Clip, length)));
                }
                b"TNFX" => {
                    r.tag(0x02)?;
                    r.tag(0x01)?;
                    cutpoint = r.i32()? as i64;
                }
                b"SLCT" => {
                    r.tag(0x02)?;
                    r.tag(0x01)?;
                    r.u8()?; // ganged
                    selected = Some(r.u16()?);
                }
                b"SPED" => {
                    r.tag(0x02)?;
                    r.tag(0x02)?;
                    r.i32()?; // phase offset
                    r.tag(0x02)?;
                    r.tag(0x03)?;
                    speed = Some((r.i32()?, r.i32()?));
                }
                _ => {}
            }
            let tracks = tracks.into_iter().map(|t| t.1).collect();
            Ok(Raw::Comp(comp(Part::Group { class: *class, tracks, selected, speed, cutpoint }, length)))
        }
        _ => Ok(Raw::Nothing),
    }
}

/// Limits for walking a bin (real sequences stay far below them).
const MAX_DEPTH: u32 = 64;
const MAX_STEPS: u32 = 10_000;

/// What a frame of a component shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    /// frame `frame` of the media file `file` (edit rate `rate`); `clip` = name of the first named clip on the way
    /// `linked`: path and frame of the linked (AMA) file, which lives outside Avid MediaFiles
    Media { file: MobId, frame: i64, rate: f64, clip: String, linked: Option<(String, i64)> },
    /// nothing: filler, an empty effect – lets the tracks below show through
    Empty,
    /// a clip whose mob is in no bin read here
    Missing { clip: String },
    /// a clip without media (tape source)
    Offline { clip: String },
}

impl Bin {
    pub fn read(path: &Path) -> Res<Bin> {
        Bin::parse(&std::fs::read(path).map_err(|e| e.to_string())?)
    }

    pub fn parse(data: &[u8]) -> Res<Bin> {
        let le = match data.get(..2) {
            Some([0x06, 0x00]) => true,
            Some([0x00, 0x06]) => false,
            _ => return Err("not a Media Composer bin".into()),
        };
        let mut r = R { d: data, p: 2, le };
        if r.take(6)? != b"Domain" {
            return Err("not a Media Composer bin".into());
        }
        let fourcc = |r: &mut R| -> Res<[u8; 4]> {
            let mut c: [u8; 4] = r.take(4)?.try_into().unwrap();
            if r.le {
                c.reverse();
            }
            Ok(c)
        };
        if &fourcc(&mut r)? != b"OBJD" || r.string()? != "AObjDoc" || r.u8()? != 4 {
            return Err("unknown bin header".into());
        }
        r.string()?;
        let count = r.u32()? as usize;
        let root = r.u32()?;
        r.u32()?;
        r.u32()?;
        r.u32()?;
        if &fourcc(&mut r)? != b"ATob" || &fourcc(&mut r)? != b"ATve" {
            return Err("unknown bin header".into());
        }
        r.string()?; // creator
        r.take(16)?;
        let mut objects: Vec<([u8; 4], &[u8])> = vec![(*b"    ", &[])];
        for _ in 0..count {
            let class = fourcc(&mut r)?;
            let size = r.u32()? as usize;
            objects.push((class, r.take(size)?));
        }

        let mut bin = Bin::default();
        let mut mob_at = HashMap::new();
        let mut descriptors = HashMap::new();
        let mut locators = HashMap::new();
        let mut mob_desc = vec![];
        // classes whose layout this reader must get right; any other class that fails is simply left out
        const STRICT: [&[u8; 4]; 7] = [b"SEQU", b"SCLP", b"FILL", b"CMPO", b"TNFX", b"SLCT", b"SPED"];
        for (i, (class, d)) in objects.iter().enumerate().skip(1) {
            let raw = match parse_object(class, d, le) {
                Ok(raw) => raw,
                Err(e) if STRICT.contains(&class) => return Err(format!("object {i} ({}): {e}", String::from_utf8_lossy(class))),
                Err(_) => Raw::Nothing,
            };
            match raw {
                Raw::Comp(c) => {
                    bin.comps.insert(i as u32, c);
                }
                Raw::Mob(m, desc, c) => {
                    mob_at.insert(i as u32, bin.mobs.len());
                    mob_desc.push(desc);
                    bin.comps.insert(i as u32, c);
                    bin.mobs.push(m);
                }
                Raw::Descriptor(class, locator) => {
                    descriptors.insert(i as u32, (class, locator));
                }
                Raw::Locator(path) => {
                    locators.insert(i as u32, path);
                }
                Raw::Nothing => {}
            }
        }
        for (m, d) in bin.mobs.iter_mut().zip(mob_desc) {
            if let Some((class, locator)) = descriptors.get(&d) {
                m.descriptor = Some(class.clone());
                m.path = locators.get(locator).filter(|p| !p.is_empty()).cloned();
            }
        }

        let (rclass, rdata) = objects.get(root as usize).ok_or("root object missing")?;
        if rclass != b"ABIN" && rclass != b"BINF" {
            return Err(format!("root object is {}", String::from_utf8_lossy(rclass)));
        }
        let mut b = R { d: rdata, p: 0, le };
        b.tag(0x02)?;
        let version = b.u8()?;
        b.u32()?;
        b.take(8)?;
        let n = match version {
            0x0e => b.u16()? as usize,
            0x0f => b.u32()? as usize,
            v => return Err(format!("unknown bin version {v}")),
        };
        for _ in 0..n {
            let obj = b.u32()?;
            b.take(9)?;
            if let Some(&m) = mob_at.get(&obj) {
                bin.items.push(m);
            }
        }
        bin.by_id = bin.mobs.iter().enumerate().map(|(i, m)| (m.id, i)).collect();
        bin.lengths = bin.all_lengths();
        Ok(bin)
    }

    pub fn mob(&self, id: &MobId) -> Option<&Mob> {
        self.by_id.get(id).map(|&i| &self.mobs[i])
    }

    /// Sequences the bin shows (compositions with usage 0).
    pub fn sequences(&self) -> Vec<&Mob> {
        self.items.iter().map(|&i| &self.mobs[i]).filter(|m| m.kind == MobKind::Composition && m.usage == 0).collect()
    }

    /// Path of a linked file and the frame in it: the file mob's own locator, else the source mob below it
    /// (Media Composer keeps the path of an AMA-linked file there). The file mob stands for the linked file, so
    /// its own frame is the frame in the file; the clip below can carry an offset of its own source numbering
    /// (seen: start 1 for a 25-frame QuickTime file, which would push the last frame past the file's end).
    fn linked(&self, file: &Mob, at: i64, bins: &[Bin]) -> Option<(String, i64)> {
        if let Some(p) = &file.path {
            return Some((p.clone(), at));
        }
        let (_, track) = file.tracks.first()?;
        let part = match self.comps.get(track).map(|c| &c.part) {
            Some(Part::Sequence(_)) => self.pieces(*track).into_iter().find(|p| p.0 <= at && at < p.1).map(|p| p.2)?,
            _ => *track,
        };
        match self.comps.get(&part).map(|c| &c.part) {
            Some(Part::Source { mob, .. }) => {
                let m = self.mob(mob).or_else(|| bins.iter().find_map(|b| b.mob(mob)))?;
                Some((m.path.clone()?, at))
            }
            _ => None,
        }
    }

    /// Start timecode (frame count) of a composition: its timecode track's first component.
    pub fn start_tc(&self, mob: &Mob) -> i64 {
        mob.tracks
            .iter()
            .filter_map(|(_, c)| self.comps.get(c))
            .filter(|c| c.media == 3)
            .find_map(|c| match &c.part {
                Part::Timecode(t) => Some(*t),
                Part::Sequence(parts) => parts.iter().find_map(|p| match self.comps.get(p) {
                    Some(Component { part: Part::Timecode(t), .. }) => Some(*t),
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or(0)
    }

    /// Length of a component; a sequence's transitions overlap their neighbours.
    pub fn length(&self, obj: u32) -> i64 {
        match self.lengths.get(&obj) {
            Some(&l) => l,
            None => self.length_memo(obj, 0, &mut HashMap::new(), &mut std::collections::HashSet::new()),
        }
    }

    /// Every sequence's length, each component counted once: a crafted bin that nests the same sequence many
    /// times over would otherwise double the work at every level.
    fn all_lengths(&self) -> HashMap<u32, i64> {
        let mut memo = HashMap::new();
        let seqs: Vec<u32> = self.comps.iter().filter(|(_, c)| matches!(c.part, Part::Sequence(_))).map(|(&k, _)| k).collect();
        for k in seqs {
            self.length_memo(k, 0, &mut memo, &mut std::collections::HashSet::new());
        }
        memo
    }

    /// Length with a memo; a sequence that contains itself (or nesting past MAX_DEPTH) counts 0.
    fn length_memo(&self, obj: u32, depth: u32, memo: &mut HashMap<u32, i64>, open: &mut std::collections::HashSet<u32>) -> i64 {
        if let Some(&l) = memo.get(&obj) {
            return l;
        }
        let l = match self.comps.get(&obj) {
            Some(Component { part: Part::Sequence(parts), .. }) => {
                if depth > MAX_DEPTH || !open.insert(obj) {
                    return 0;
                }
                let mut sum = 0i64;
                for &p in parts {
                    let l = self.length_memo(p, depth + 1, memo, open);
                    sum = sum.saturating_add(if self.is_transition(p) { -l } else { l });
                }
                open.remove(&obj);
                sum
            }
            Some(c) => c.length,
            None => 0,
        };
        memo.insert(obj, l);
        l
    }

    fn is_transition(&self, obj: u32) -> bool {
        matches!(self.comps.get(&obj), Some(Component { part: Part::Group { class, .. }, .. }) if class == b"TNFX")
    }

    /// The pieces of a sequence: (start, end, component, where the component itself starts), end exclusive, in the
    /// sequence's frames. A transition cuts its two neighbours at its cut point, so the pieces never overlap; the
    /// component after it starts before its piece (it runs under the transition).
    pub fn pieces(&self, obj: u32) -> Vec<(i64, i64, u32, i64)> {
        let Some(Component { part: Part::Sequence(parts), .. }) = self.comps.get(&obj) else {
            return vec![(0, self.length(obj), obj, 0)];
        };
        let mut out: Vec<(i64, i64, u32, i64)> = vec![];
        let mut pos = 0;
        let mut cut: Option<i64> = None;
        for &p in parts {
            let len = self.length(p);
            if self.is_transition(p) {
                pos -= len;
                let point = match self.comps.get(&p) {
                    Some(Component { part: Part::Group { cutpoint, .. }, .. }) => (*cutpoint).clamp(0, len),
                    _ => len / 2,
                };
                if let Some(last) = out.last_mut() {
                    last.1 = pos + point;
                }
                cut = Some(pos + point);
                continue;
            }
            let raw = pos;
            let start = cut.take().unwrap_or(pos);
            pos += len;
            if pos > start {
                out.push((start, pos, p, raw));
            }
        }
        out.retain(|p| p.1 > p.0);
        out
    }

    /// What frame `off` of component `obj` shows; `bins` = other bins of the project for mobs not in this one.
    pub fn resolve(&self, obj: u32, off: i64, bins: &[Bin]) -> Hit {
        self.resolve_in(obj, off, bins, "", 0, &std::cell::Cell::new(0))
    }

    /// `depth` stops cycles, `steps` caps the whole walk: effects try every input, and a crafted bin with nested
    /// many-input effects would otherwise take exponential time.
    fn resolve_in(&self, obj: u32, off: i64, bins: &[Bin], clip: &str, depth: u32, steps: &std::cell::Cell<u32>) -> Hit {
        steps.set(steps.get() + 1);
        if depth > MAX_DEPTH || steps.get() > MAX_STEPS {
            return Hit::Missing { clip: clip.to_string() };
        }
        let Some(c) = self.comps.get(&obj) else { return Hit::Empty };
        match &c.part {
            Part::Clip | Part::Timecode(_) => Hit::Empty,
            Part::Sequence(_) => match self.pieces(obj).into_iter().find(|p| p.0 <= off && off < p.1) {
                Some((_, _, p, raw)) => self.resolve_in(p, off - raw, bins, clip, depth + 1, steps),
                None => Hit::Empty,
            },
            Part::Group { class, tracks, selected, speed, .. } => {
                let inner = match speed {
                    Some((n, d)) if *d != 0 => off * *n as i64 / *d as i64,
                    _ => off,
                };
                if class == b"SLCT"
                    && let Some(Some(t)) = selected.and_then(|s| tracks.get(s as usize))
                {
                    return self.resolve_in(*t, inner, bins, clip, depth + 1, steps);
                }
                // effects: the first input that shows something
                let mut fallback = Hit::Empty;
                for t in tracks.iter().flatten() {
                    match self.resolve_in(*t, inner, bins, clip, depth + 1, steps) {
                        h @ Hit::Media { .. } => return h,
                        Hit::Empty => {}
                        h => fallback = h,
                    }
                }
                fallback
            }
            Part::Source { mob, track, start } => {
                if mob.iter().all(|&b| b == 0) {
                    return Hit::Empty;
                }
                // the mob may live in another bin of the project: continue there
                let found = self.mob(mob).map(|m| (self, m)).or_else(|| bins.iter().find_map(|b| b.mob(mob).map(|m| (b, m))));
                let Some((bin, m)) = found else { return Hit::Missing { clip: clip.to_string() } };
                let clip = if clip.is_empty() && !m.name.trim().is_empty() { m.name.as_str() } else { clip };
                let at = start + off;
                if m.is_file() {
                    return Hit::Media { file: m.id, frame: at, rate: c.rate, clip: clip.to_string(), linked: bin.linked(m, at, bins) };
                }
                if m.kind == MobKind::Source {
                    return Hit::Offline { clip: clip.to_string() };
                }
                // the mob's track with the same number and media kind
                let same = |kind: bool| m.tracks.iter().find(|(n, t)| *n == Some(*track) && (!kind || bin.comps.get(t).is_some_and(|tc| tc.media == c.media)));
                let Some(&(_, t)) = same(true).or_else(|| same(false)) else { return Hit::Offline { clip: clip.to_string() } };
                let rate = bin.comps.get(&t).map_or(c.rate, |tc| tc.rate);
                let at = if rate > 0.0 && c.rate > 0.0 && (rate - c.rate).abs() > 0.001 { (at as f64 * rate / c.rate).round() as i64 } else { at };
                bin.resolve_in(t, at, bins, clip, depth + 1, steps)
            }
        }
    }
}

/// Mob id as hex text (logs, errors).
pub fn id_hex(id: &MobId) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> MobId {
        let mut i = [0u8; 32];
        i[31] = n;
        i
    }
    fn comp(part: Part, length: i64) -> Component {
        Component { part, media: 1, rate: 25.0, length }
    }
    fn group(class: &[u8; 4], tracks: Vec<u32>) -> Part {
        Part::Group { class: *class, tracks: tracks.into_iter().map(Some).collect(), selected: None, speed: None, cutpoint: 0 }
    }

    /// file mob 1 ← master "Cam A" (starts 1000 frames into the file) ← sequence:
    /// [A 100] [dissolve 20, cut at 10] [B 100] [filler 50] [motion 2× of A 40] [multicam selecting track 2 → A 30]
    fn bin() -> Bin {
        let mut b = Bin::default();
        let file = Mob { name: "Cam A.mxf".into(), id: id(1), kind: MobKind::Source, usage: 0, descriptor: Some("CDCI".into()), path: None, tracks: vec![(Some(1), 50)] };
        let master = Mob { name: "Cam A".into(), id: id(2), kind: MobKind::Master, usage: 0, descriptor: None, path: None, tracks: vec![(Some(1), 51)] };
        b.comps.insert(50, comp(Part::Clip, 100_000));
        b.comps.insert(51, comp(Part::Source { mob: id(1), track: 1, start: 1000 }, 100_000));
        let a = |start| Part::Source { mob: id(2), track: 1, start };
        b.comps.insert(1, comp(a(0), 100));
        b.comps.insert(2, comp(Part::Group { class: *b"TNFX", tracks: vec![], selected: None, speed: None, cutpoint: 10 }, 20));
        b.comps.insert(3, comp(a(500), 100));
        b.comps.insert(4, comp(Part::Clip, 50));
        b.comps.insert(5, comp(a(2000), 80));
        b.comps.insert(6, comp(Part::Group { class: *b"SPED", tracks: vec![Some(5)], selected: None, speed: Some((2, 1)), cutpoint: 0 }, 40));
        b.comps.insert(7, comp(Part::Clip, 30));
        b.comps.insert(8, comp(a(3000), 30));
        b.comps.insert(9, comp(Part::Group { class: *b"SLCT", tracks: vec![Some(7), Some(8)], selected: Some(1), speed: None, cutpoint: 0 }, 30));
        b.comps.insert(10, comp(group(b"TKFX", vec![]), 0)); // unused
        b.comps.insert(20, comp(Part::Sequence(vec![1, 2, 3, 4, 6, 9]), 0));
        b.mobs = vec![file, master];
        b.by_id = b.mobs.iter().enumerate().map(|(i, m)| (m.id, i)).collect();
        b
    }

    fn frame(h: Hit) -> i64 {
        match h {
            Hit::Media { frame, clip, .. } => {
                assert_eq!(clip, "Cam A");
                frame
            }
            h => panic!("{h:?}"),
        }
    }

    #[test]
    fn sequence_pieces_cut_transitions_at_their_cut_point() {
        let b = bin();
        // A runs 0–100, the dissolve 80–100 cuts at 90, B runs from 80 (raw) – shown from 90
        let p = b.pieces(20);
        assert_eq!(p[0], (0, 90, 1, 0));
        assert_eq!(p[1], (90, 180, 3, 80));
        assert_eq!(p[2], (180, 230, 4, 180));
        assert_eq!(b.length(20), 100 - 20 + 100 + 50 + 40 + 30);
        assert_eq!(frame(b.resolve(20, 85, &[])), 1000 + 85);
        assert_eq!(frame(b.resolve(20, 95, &[])), 1000 + 500 + 15);
        assert_eq!(b.resolve(20, 200, &[]), Hit::Empty);
    }

    #[test]
    fn motion_effects_and_multicam_reach_the_right_frame() {
        let b = bin();
        // motion effect from 230: frame 10 of it = frame 20 of its clip
        assert_eq!(frame(b.resolve(20, 240, &[])), 1000 + 2000 + 20);
        // multicam from 270: the selected angle (track 2)
        assert_eq!(frame(b.resolve(20, 275, &[])), 1000 + 3000 + 5);
    }

    #[test]
    fn crafted_cycles_and_effect_towers_end_quickly() {
        let mut b = Bin::default();
        // a sequence that contains itself
        b.comps.insert(1, comp(Part::Sequence(vec![1, 2]), 0));
        b.comps.insert(2, comp(Part::Clip, 10));
        assert!(b.length(1) >= 0);
        // 60 levels, each holding the level below twice: 2^60 without the memo
        for i in 200..260u32 {
            b.comps.insert(i, comp(Part::Sequence(vec![i + 1, i + 1]), 0));
        }
        b.comps.insert(260, comp(Part::Clip, 1));
        let t0 = std::time::Instant::now();
        b.lengths = b.all_lengths();
        assert!(b.length(200) >= 0 && t0.elapsed().as_secs() < 1);
        assert!(!matches!(b.resolve(1, 3, &[]), Hit::Media { .. }));
        // 40 levels of effects with 4 inputs each, all pointing at the next level: 4^40 paths without a budget
        for i in 100..140u32 {
            b.comps.insert(i, comp(Part::Group { class: *b"TKFX", tracks: vec![Some(i + 1); 4], selected: None, speed: None, cutpoint: 0 }, 10));
        }
        b.comps.insert(140, comp(Part::Clip, 10));
        let t = std::time::Instant::now();
        // no media anywhere: the walk gives up (Empty or Missing) instead of running for ever
        assert!(!matches!(b.resolve(100, 1, &[]), Hit::Media { .. }));
        assert!(t.elapsed().as_secs() < 2);
    }

    #[test]
    fn clips_from_other_bins_and_missing_ones() {
        let full = bin();
        let mut seq_only = Bin::default();
        seq_only.comps.insert(1, comp(Part::Source { mob: id(2), track: 1, start: 7 }, 10));
        assert_eq!(seq_only.resolve(1, 3, &[]), Hit::Missing { clip: String::new() });
        assert_eq!(frame(seq_only.resolve(1, 3, std::slice::from_ref(&full))), 1010);
    }
}
