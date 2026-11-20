//! The Mac's installed fonts (fontdb) for the headline: family names for the menu, a face for bold / italic.

use ab_glyph::FontArc;
use std::sync::{Arc, OnceLock};

fn db() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

/// Family names, sorted, without the hidden system fonts (".SF …").
pub fn families() -> Vec<String> {
    let mut names: Vec<String> = db().faces().flat_map(|f| f.families.iter().map(|(n, _)| n.clone())).filter(|n| !n.starts_with('.')).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// A face of `family`: the best match for bold / italic. The flags say what the face lacks, so the drawing can
/// fake it (thicker strokes, slanted).
#[derive(Clone)]
pub struct Face {
    pub font: Arc<FontArc>,
    pub fake_bold: bool,
    pub fake_italic: bool,
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Face(fake bold {}, fake italic {})", self.fake_bold, self.fake_italic)
    }
}

pub fn face(family: &str, bold: bool, italic: bool) -> Option<Face> {
    let db = db();
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight: if bold { fontdb::Weight::BOLD } else { fontdb::Weight::NORMAL },
        style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        ..Default::default()
    };
    let id = db.query(&query)?;
    let info = db.face(id)?;
    let font = db.with_face_data(id, |data, index| ab_glyph::FontVec::try_from_vec_and_index(data.to_vec(), index).ok().map(FontArc::new))??;
    Some(Face { font: Arc::new(font), fake_bold: bold && info.weight.0 < 600, fake_italic: italic && info.style == fontdb::Style::Normal })
}
