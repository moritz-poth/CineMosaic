//! Media file of a file mob: the MXF folders' databases (msmFMID.pmr) and, for files they do not list, the MXF header.
//!
//! msmFMID.pmr reader and MXF header reader copied from MediaFiles Check (`src/lib.rs` read_pmr_entries,
//! `src/mxf.rs`); the pmr layout is reverse-engineered there and documented in its lib.rs.
use crate::avb::{MobId, mac_roman};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    le: bool,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or("file ends too early")?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<usize, String> {
        let b: [u8; 2] = self.take(2)?.try_into().unwrap();
        Ok(if self.le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) } as usize)
    }
    fn u32(&mut self) -> Result<usize, String> {
        let b: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(if self.le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) } as usize)
    }
    /// 16-bit length + bytes; 0xFFFF is an empty (null) text
    fn counted(&mut self) -> Result<&'a [u8], String> {
        match self.u16()? {
            0xFFFF => Ok(&[]),
            n => self.take(n),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PmrEntry {
    pub name: String,
    /// mob ID of the media file (same layout as in bins and MXF headers)
    pub id: [u8; 32],
    /// second mob ID of the record (the master clip)
    pub id2: [u8; 32],
    /// project the file was made in (may be empty)
    pub project: String,
}

const PMR_MAGIC: usize = 1961;
const PMR_UNICODE: usize = 16;

struct PmrRaw<'a> {
    id: &'a [u8],
    name: &'a [u8],
    project: &'a [u8],
    id2: &'a [u8],
}

/// Frames one list without decoding it.
fn pmr_list<'a>(r: &mut Reader<'a>, version: usize, count: usize) -> Result<Vec<PmrRaw<'a>>, String> {
    let id_len = if version <= 7 { 8 } else { 32 };
    let smallest = if version == 1 { 14 } else if version <= 7 { 24 } else { 72 };
    if count > (r.data.len() - r.pos) / smallest {
        return Err(format!("{count} entries announced, the file is too short for them"));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.take(id_len)?;
        let name = r.counted()?;
        let (project, id2) = if version == 1 { (&[][..], &[][..]) } else { (r.counted()?, r.take(id_len)?) };
        r.take(4)?;
        out.push(PmrRaw { id, name, project, id2 });
    }
    Ok(out)
}

/// A mob ID in the layout bins and MXF headers use: the material fields (32/16/16 bit) little-endian; the old 8-byte
/// IDs are wrapped the way Media Composer wraps them.
fn pmr_mob_id(raw: &[u8], le: bool) -> [u8; 32] {
    let mut id = [0u8; 32];
    match raw.len() {
        32 => {
            id.copy_from_slice(raw);
            if !le {
                id[16..20].reverse();
                id[20..22].reverse();
                id[22..24].reverse();
            }
        }
        8 => {
            id[..16].copy_from_slice(&[6, 0x0a, 0x2b, 0x34, 1, 1, 1, 1, 1, 1, 0x0f, 0, 0x13, 0, 0, 0]);
            id[16..24].copy_from_slice(raw);
            if !le {
                id[16..20].reverse();
                id[20..24].reverse();
            }
            id[24..].copy_from_slice(&[6, 0x0e, 0x2b, 0x34, 0x7f, 0x7f, 0x2a, 0x80]);
        }
        _ => {}
    }
    id
}

fn cut_nul(b: &[u8]) -> Vec<u8> {
    b.iter().position(|&c| c == 0).map_or(b, |i| &b[..i]).to_vec()
}

fn pmr_decode(list: &[PmrRaw], version: usize, le: bool) -> Result<Vec<PmrEntry>, String> {
    list.iter()
        .map(|e| {
            let name = if version == PMR_UNICODE {
                // two bytes before the UTF-8 name, the first one 0
                if e.name.len() < 2 || e.name[0] != 0 {
                    return Err("unexpected name length".to_string());
                }
                String::from_utf8(cut_nul(&e.name[2..])).map_err(|_| "a name is not valid UTF-8")?
            } else {
                mac_roman(&cut_nul(e.name))
            };
            if name.is_empty() {
                return Err("a name is empty".into());
            }
            Ok(PmrEntry { name, id: pmr_mob_id(e.id, le), id2: pmr_mob_id(e.id2, le), project: mac_roman(&cut_nul(e.project)) })
        })
        .collect()
}

/// Names and mob IDs of the media files in a msmFMID.pmr (see read_pmr for the lists).
pub fn read_pmr_entries(data: &[u8]) -> Result<Vec<PmrEntry>, String> {
    let head: [u8; 4] = data.get(..4).ok_or("file ends too early")?.try_into().unwrap();
    let le = if u32::from_le_bytes(head) as usize == PMR_MAGIC {
        true
    } else if u32::from_be_bytes(head) as usize == PMR_MAGIC {
        false
    } else {
        return Err("not an Avid database (unknown beginning)".into());
    };
    let mut r = Reader { data, pos: 4, le };
    let (version, count) = (r.u32()?, r.u32()?);
    if !(1..=8).contains(&version) {
        return Err(format!("unknown database version {version}"));
    }
    let first = pmr_list(&mut r, version, count)?;
    if r.pos == data.len() {
        return pmr_decode(&first, version, le);
    }
    let extension = r.u32()?;
    if extension != PMR_UNICODE {
        return Err(format!("unknown second list (version {extension})"));
    }
    let count2 = r.u32()?;
    let second = pmr_list(&mut r, PMR_UNICODE, count2)?;
    if r.pos != data.len() {
        return Err(format!("{} unexpected bytes at the end", data.len() - r.pos));
    }
    let Ok(entries) = pmr_decode(&second, PMR_UNICODE, le) else {
        return pmr_decode(&first, version, le);
    };
    if count2 < count {
        return Err(format!("the second list is shorter than the first ({count2} / {count})"));
    }
    let all: HashSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    if first.iter().any(|e| e.name.is_ascii() && !all.contains(String::from_utf8_lossy(&cut_nul(e.name)).as_ref())) {
        return Err("the two lists differ".into());
    }
    Ok(entries)
}

const SET: [u8; 16] = [0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x00, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00];
const ESSENCE_CONTAINER_DATA: u8 = 0x23;
const MATERIAL_PACKAGE: u8 = 0x36;
const PARTITION: [u8; 13] = [0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x00, 0x0d, 0x01, 0x02, 0x01, 0x01];
/// stop reading a header that claims to be larger than this
const MAX_HEADER: u64 = 16 << 20;

fn same_key(key: &[u8], pattern: &[u8], skip: &[usize]) -> bool {
    pattern.iter().enumerate().all(|(i, b)| skip.contains(&i) || key.get(i) == Some(b))
}

/// BER length at `p`: (length, bytes used).
fn ber(d: &[u8], p: usize) -> Option<(u64, usize)> {
    let first = *d.get(p)?;
    if first < 0x80 {
        return Some((first as u64, 1));
    }
    let n = (first & 0x7f) as usize;
    if n == 0 || n > 8 {
        return None;
    }
    let bytes = d.get(p + 1..p + 1 + n)?;
    Some((bytes.iter().fold(0u64, |a, &b| a << 8 | b as u64), 1 + n))
}

/// MXF UMID → our mob ID layout: the material number's first three fields are big-endian in MXF,
/// little-endian in bins and msmFMID.pmr.
fn to_mob_id(umid: &[u8]) -> MobId {
    let mut id: MobId = umid.try_into().unwrap();
    id[16..20].reverse();
    id[20..22].reverse();
    id[22..24].reverse();
    id
}

/// File mob ID from the header bytes of an MXF file.
fn parse(d: &[u8]) -> Result<MobId, String> {
    find_set(d, ESSENCE_CONTAINER_DATA, 0x2701)?.ok_or_else(|| "essence container data without package".into())
}

/// The UMID under `tag` in the first header set of kind `kind`; Ok(None) if that set has no such UMID.
fn find_set(d: &[u8], kind: u8, want: u16) -> Result<Option<MobId>, String> {
    if !same_key(d, &PARTITION, &[7]) {
        return Err("not an MXF file".into());
    }
    let mut p = 0;
    while p + 16 < d.len() {
        let key = &d[p..p + 16];
        let (len, n) = ber(d, p + 16).ok_or("bad KLV length")?;
        let start = p + 16 + n;
        let end = start.checked_add(len as usize).filter(|&e| e <= d.len());
        if same_key(key, &SET, &[7, 14]) && key[14] == kind {
            let end = end.ok_or("header ends too early")?;
            let mut q = start;
            while q + 4 <= end {
                let tag = u16::from_be_bytes([d[q], d[q + 1]]);
                let l = u16::from_be_bytes([d[q + 2], d[q + 3]]) as usize;
                if tag == want && l == 32 && q + 4 + 32 <= end {
                    return Ok(Some(to_mob_id(&d[q + 4..q + 36])));
                }
                q += 4 + l;
            }
            return Ok(None);
        }
        // essence or index data: the header metadata is over
        // (essence element 06 0e 2b 34 01 02 …, index table segment …0d 01 02 01 01 10 – not KLV fill 01 01 …)
        if key[..4] == SET[..4] && ((key[4] == 0x01 && key[5] == 0x02) || (key[4] == 0x02 && key[5] == 0x53 && key[13] == 0x10)) {
            break;
        }
        match end {
            Some(e) => p = e,
            None => break,
        }
    }
    Err(if kind == MATERIAL_PACKAGE { "no material package in the header" } else { "no essence container data in the header" }.into())
}

/// File mob ID of an MXF file, read from its header.
pub fn file_mob_id(path: &Path) -> Result<MobId, String> {
    parse(&header(path)?)
}

fn header(path: &Path) -> Result<Vec<u8>, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let mut d = vec![0u8; 64 << 10];
    let n = read_full(&mut f, &mut d)?;
    d.truncate(n);
    // partition pack: key, length, then major, minor, KAG (8 bytes) and four u64 – the fourth is HeaderByteCount
    if let Some((pack, l)) = ber(&d, 16) {
        let at = 16 + l + 8 + 24;
        if let Some(b) = d.get(at..at + 8) {
            let header = u64::from_be_bytes(b.try_into().unwrap());
            let want = (16 + l as u64 + pack + header).min(MAX_HEADER) as usize;
            if want > d.len() && n == 64 << 10 {
                let have = d.len();
                d.resize(want, 0);
                let more = read_full(&mut f, &mut d[have..])?;
                d.truncate(have + more);
            }
        }
    }
    Ok(d)
}

fn read_full(f: &mut File, buf: &mut [u8]) -> Result<usize, String> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(n)
}

/// Volumes with Avid MediaFiles: every folder in /Volumes holding "Avid MediaFiles/MXF".
pub fn volumes() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("/Volumes")
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.join("Avid MediaFiles/MXF").is_dir()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// The MXF folders of a volume root ("<root>/Avid MediaFiles/MXF/<folder>").
fn mxf_folders(root: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(root.join("Avid MediaFiles/MXF"))
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

/// Paths of the wanted file mobs on these volumes: names from each folder's msmFMID.pmr first; the headers of
/// files no database lists are read only while something is still missing. `go()` false = stop (cancel).
pub fn locate(volumes: &[PathBuf], wanted: &HashSet<MobId>, go: &dyn Fn() -> bool) -> HashMap<MobId, PathBuf> {
    let mut found = HashMap::new();
    let mut unlisted = vec![];
    for folder in volumes.iter().flat_map(|v| mxf_folders(v)) {
        if !go() {
            return found;
        }
        let files: Vec<PathBuf> = std::fs::read_dir(&folder)
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let name = p.file_name().unwrap_or_default().to_string_lossy();
                        !name.starts_with('.') && name.to_ascii_lowercase().ends_with(".mxf")
                    })
                    .collect()
            })
            .unwrap_or_default();
        let entries = std::fs::read(folder.join("msmFMID.pmr")).ok().and_then(|d| read_pmr_entries(&d).ok()).unwrap_or_default();
        let listed: HashSet<String> = entries.iter().map(|e| e.name.clone()).collect();
        for e in entries {
            let p = folder.join(&e.name);
            if wanted.contains(&e.id) && p.is_file() {
                found.entry(e.id).or_insert(p);
            }
        }
        unlisted.extend(files.into_iter().filter(|p| !listed.contains(p.file_name().unwrap_or_default().to_string_lossy().as_ref())));
    }
    for p in unlisted {
        if found.len() == wanted.len() || !go() {
            break;
        }
        if let Ok(id) = file_mob_id(&p)
            && wanted.contains(&id)
        {
            found.entry(id).or_insert(p);
        }
    }
    found
}
