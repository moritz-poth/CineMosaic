//! The sheet: paper size, resolution, margin and gap → a grid of equal cells filled left to right, top to bottom,
//! centred in its area. Optional: a trim area in the picture format with crop marks, a frame line of parallel
//! strands, text elements hung at any point of the sheet (where one covers the line, the line breaks), each with an
//! optional drop shadow as Photoshop's layer style draws it. Line, texts and grid keep out of each other's way.

use crate::fonts::Face;
use ab_glyph::{Font, PxScale, ScaleFont, point};
use image::{Rgb, RgbImage, imageops};

/// DIN A sizes in mm, portrait.
pub const DIN: [(&str, f64, f64); 7] =
    [("A0", 841.0, 1189.0), ("A1", 594.0, 841.0), ("A2", 420.0, 594.0), ("A3", 297.0, 420.0), ("A4", 210.0, 297.0), ("A5", 148.0, 210.0), ("A6", 105.0, 148.0)];

/// Room for the crop marks around the trim area, and the marks themselves (mm).
const MARK_ROOM: f64 = 12.0;
const MARK_OFFSET: f64 = 3.0;
const MARK_LENGTH: f64 = 6.0;
/// Space between the headline and the line ends it breaks, and between headline / line and the stills (mm).
const BREAK_PAD: f64 = 2.0;
/// between the frame line and the stills (mm); the headline has its own space (`Headline::pad_mm`)
const CLEARANCE: f64 = 2.0;

/// Line patterns: parallel strands, no dashes. Each strand: (distance inward from the outer edge, width), both in
/// units of the line thickness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    Single,
    Double,
    ThickThin,
    ThinThick,
    /// thin – thick – thin
    Triple,
    /// two close together, a third further in
    TwoAndOne,
}

impl Pattern {
    pub const ALL: [Pattern; 6] = [Pattern::Single, Pattern::Double, Pattern::ThickThin, Pattern::ThinThick, Pattern::Triple, Pattern::TwoAndOne];

    pub fn strands(self) -> &'static [(f64, f64)] {
        match self {
            Pattern::Single => &[(0.0, 1.0)],
            Pattern::Double => &[(0.0, 1.0), (2.5, 1.0)],
            Pattern::ThickThin => &[(0.0, 3.0), (4.5, 1.0)],
            Pattern::ThinThick => &[(0.0, 1.0), (2.5, 3.0)],
            Pattern::Triple => &[(0.0, 1.0), (2.5, 2.5), (6.5, 1.0)],
            Pattern::TwoAndOne => &[(0.0, 1.0), (2.2, 1.0), (7.0, 1.0)],
        }
    }

    /// Width of the whole pattern in units of the thickness.
    pub fn extent(self) -> f64 {
        self.strands().iter().map(|(o, w)| o + w).fold(0.0, f64::max)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// thickness of a thin strand
    pub width_mm: f64,
    /// distance of the outer strand from the edge of the paper (or of the trim area)
    pub inset_mm: f64,
    pub color: [u8; 3],
    pub pattern: Pattern,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

/// Blend modes of the drop shadow, as Photoshop names them (Dissolve and the colour modes left out).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blend {
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
}

impl Blend {
    pub const ALL: [Blend; 14] = [
        Blend::Normal,
        Blend::Darken,
        Blend::Multiply,
        Blend::ColorBurn,
        Blend::LinearBurn,
        Blend::Lighten,
        Blend::Screen,
        Blend::ColorDodge,
        Blend::LinearDodge,
        Blend::Overlay,
        Blend::SoftLight,
        Blend::HardLight,
        Blend::Difference,
        Blend::Exclusion,
    ];

    /// One channel: `b` the page below, `c` the shadow colour, both 0–1.
    fn mix(self, b: f32, c: f32) -> f32 {
        match self {
            Blend::Normal => c,
            Blend::Darken => b.min(c),
            Blend::Multiply => b * c,
            Blend::ColorBurn => if c <= 0.0 { 0.0 } else { 1.0 - ((1.0 - b) / c).min(1.0) },
            Blend::LinearBurn => (b + c - 1.0).max(0.0),
            Blend::Lighten => b.max(c),
            Blend::Screen => 1.0 - (1.0 - b) * (1.0 - c),
            Blend::ColorDodge => if c >= 1.0 { 1.0 } else { (b / (1.0 - c)).min(1.0) },
            Blend::LinearDodge => (b + c).min(1.0),
            Blend::Overlay => Blend::HardLight.mix(c, b),
            Blend::SoftLight => {
                if c <= 0.5 {
                    b - (1.0 - 2.0 * c) * b * (1.0 - b)
                } else {
                    let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                    b + (2.0 * c - 1.0) * (d - b)
                }
            }
            Blend::HardLight => if c <= 0.5 { 2.0 * b * c } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - c) },
            Blend::Difference => (b - c).abs(),
            Blend::Exclusion => b + c - 2.0 * b * c,
        }
    }
}

/// Contour presets of Photoshop's layer style: how the shadow's opacity runs from its edge (0) to its core (1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Contour {
    Linear,
    Cone,
    ConeInverted,
    Gaussian,
    HalfRound,
    Ring,
    RingDouble,
    RollingSlope,
    RoundedSteps,
    Sawtooth,
}

impl Contour {
    pub const ALL: [Contour; 10] = [
        Contour::Linear,
        Contour::Cone,
        Contour::ConeInverted,
        Contour::Gaussian,
        Contour::HalfRound,
        Contour::Ring,
        Contour::RingDouble,
        Contour::RollingSlope,
        Contour::RoundedSteps,
        Contour::Sawtooth,
    ];

    fn at(self, x: f32) -> f32 {
        use std::f32::consts::PI;
        let y = match self {
            Contour::Linear => x,
            Contour::Cone => 1.0 - (1.0 - 2.0 * x).abs(),
            Contour::ConeInverted => (1.0 - 2.0 * x).abs(),
            Contour::Gaussian => 0.5 - 0.5 * (PI * x).cos(),
            Contour::HalfRound => (1.0 - (1.0 - x) * (1.0 - x)).sqrt(),
            Contour::Ring => (PI * x).sin(),
            Contour::RingDouble => (2.0 * PI * x).sin().abs(),
            Contour::RollingSlope => 1.0 - x + 0.25 * (2.0 * PI * x).sin() * x,
            Contour::RoundedSteps => {
                let k = x * 4.0;
                let step = k.floor().min(3.0);
                let f = k - step;
                (step + f * f * (3.0 - 2.0 * f)) / 4.0
            }
            Contour::Sawtooth => (x * 2.0).fract() * 0.5 + x * 0.5,
        };
        y.clamp(0.0, 1.0)
    }

    /// 256 steps; anti-aliased smooths the curve (like the checkbox beside Photoshop's contour).
    fn table(self, anti_aliased: bool) -> [f32; 256] {
        let mut t = [0.0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            *v = self.at(i as f32 / 255.0);
        }
        if anti_aliased {
            let c = t;
            for i in 0..256usize {
                let (a, b) = (i.saturating_sub(3), (i + 3).min(255));
                t[i] = c[a..=b].iter().sum::<f32>() / (b - a + 1) as f32;
            }
        }
        t
    }
}

/// Drop shadow like Photoshop's layer style. Lengths in mm of the sheet (the UI shows px), fractions 0–1.
#[derive(Clone, Debug, PartialEq)]
pub struct Shadow {
    pub blend: Blend,
    pub color: [u8; 3],
    pub opacity: f64,
    /// where the light comes from, degrees counter-clockwise from the right (120 = from the upper left: the shadow
    /// falls to the lower right)
    pub angle: f64,
    pub distance_mm: f64,
    /// how much of the size hardens the edge (Photoshop's Spread)
    pub spread: f64,
    pub size_mm: f64,
    pub contour: Contour,
    pub anti_aliased: bool,
    pub noise: f64,
    /// the text hides its own shadow (Layer Knocks Out Drop Shadow); only matters with blend modes that lighten
    pub knock_out: bool,
}

impl Shadow {
    /// How far the shadow lies from the text in px: away from the light.
    fn offset(&self, sheet: &Sheet) -> (i64, i64) {
        let (rad, d) = (self.angle.to_radians(), sheet.px(self.distance_mm));
        ((-rad.cos() * d).round() as i64, (rad.sin() * d).round() as i64)
    }
}

/// Photomosaic (user 2026-11-23): the main picture cut into `cols` tiles across (rows follow from its format and
/// the stills'), every tile a still (`tiles`, row by row, index into the sheet's stills; see `mosaic::assign`),
/// tinted towards the tile's colour by `tint` and with the main picture blended over by `overlay` (both 0–1).
#[derive(Clone, Debug)]
pub struct Mosaic {
    pub target: std::sync::Arc<RgbImage>,
    pub cols: u32,
    pub tiles: std::sync::Arc<Vec<usize>>,
    pub tint: f64,
    pub overlay: f64,
}

impl Mosaic {
    /// Rows of tiles for stills of width/height `aspect`, so the tiles together have the main picture's format.
    pub fn rows(&self, aspect: f64) -> u32 {
        let t = self.target.width() as f64 / self.target.height().max(1) as f64;
        ((self.cols as f64 * aspect / t).round() as u32).max(1)
    }
}

/// One text element of the sheet (the headline of earlier versions); several lines with \n.
#[derive(Clone, Debug)]
pub struct Headline {
    pub text: String,
    pub face: Face,
    pub size_pt: f64,
    pub color: [u8; 3],
    /// the point of the sheet (and of the text box) it hangs at: e.g. top centre = the middle of the top edge
    pub align: Align,
    pub valign: VAlign,
    /// the lines inside the text box
    pub lines: Align,
    pub shadow: Option<Shadow>,
    /// moved from that point, right and down positive
    pub dx_mm: f64,
    pub dy_mm: f64,
    /// space the stills keep from the text box: top, right, bottom, left
    pub pad_mm: [f64; 4],
    /// over the stills: they leave its place (with the space around it) free and flow on around it
    pub wrap: bool,
}

#[derive(Clone, Debug)]
pub struct Sheet {
    pub width_mm: f64,
    pub height_mm: f64,
    pub dpi: f64,
    pub margin_mm: f64,
    pub gap_mm: f64,
    /// None = as many columns as make the stills largest
    pub columns: Option<u32>,
    /// trim area of this width / height inside the paper, with crop marks at its corners
    pub trim: Option<f64>,
    pub line: Option<Line>,
    pub texts: Vec<Headline>,
    /// instead of shot after shot: the stills rebuild a main picture
    pub mosaic: Option<Mosaic>,
    /// stills per shot spread over it (photomosaic switched on, also while its main picture is still to come);
    /// 0 = the usual middle (and the longest shots twice to fill the grid)
    pub spread: u32,
    /// colour of the page behind the stills; with crop marks it fills the trim area plus 3 mm bleed
    pub background: [u8; 3],
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet { width_mm: 297.0, height_mm: 210.0, dpi: 300.0, margin_mm: 10.0, gap_mm: 3.0, columns: None, trim: None, line: None, texts: vec![], mosaic: None, spread: 0, background: [255, 255, 255] }
    }
}

/// Rectangle in pixels (x, y, width, height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Where everything goes on the page, in pixels.
#[derive(Clone, Debug)]
pub struct Plan {
    pub trim: Rect,
    /// the area the grid of stills is centred in
    pub area: Rect,
    /// the line's outer rectangle and thickness
    pub line: Option<(Rect, f64)>,
    /// every text with something in it, in the order of `Sheet::texts`
    pub texts: Vec<Placed>,
}

/// Where a text goes: its box, each line's start and baseline, the pixel scale; `i` = index in `Sheet::texts`.
#[derive(Clone, Debug)]
pub struct Placed {
    pub i: usize,
    pub rect: Rect,
    pub lines: Vec<(f64, f64)>,
    pub scale: PxScale,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
    pub cell_w: f64,
    pub cell_h: f64,
    /// top left of the first cell, px
    pub x: f64,
    pub y: f64,
    pub gap: f64,
    /// cells touching these stay empty (texts with their space, when the stills flow around them)
    pub keep_out: Vec<Rect>,
}

/// Pixel scale for a size in points at `dpi`: points are em sizes, ab_glyph scales by the font's height.
fn scale_for(face: &Face, size_pt: f64, dpi: f64) -> PxScale {
    let em = (size_pt / 72.0 * dpi) as f32;
    let f = &*face.font;
    let units = f.units_per_em().unwrap_or(1000.0);
    PxScale::from(em * f.height_unscaled() / units)
}

/// Width of a text at a scale (kerning included).
fn text_width(face: &Face, text: &str, scale: PxScale) -> f64 {
    let sf = (*face.font).as_scaled(scale);
    let mut w = 0.0;
    let mut prev = None;
    for c in text.chars() {
        let id = sf.glyph_id(c);
        if let Some(p) = prev {
            w += sf.kern(p, id);
        }
        w += sf.h_advance(id);
        prev = Some(id);
    }
    w as f64
}

impl Sheet {
    pub fn px(&self, mm: f64) -> f64 {
        mm / 25.4 * self.dpi
    }
    pub fn size_px(&self) -> (u32, u32) {
        (self.px(self.width_mm).round().max(1.0) as u32, self.px(self.height_mm).round().max(1.0) as u32)
    }

    pub fn plan(&self) -> Plan {
        let (w, h) = self.size_px();
        let (w, h) = (w as f64, h as f64);
        let trim = match self.trim {
            Some(a) if a > 0.0 => {
                let room = self.px(MARK_ROOM);
                let (aw, ah) = ((w - 2.0 * room).max(1.0), (h - 2.0 * room).max(1.0));
                let (tw, th) = if aw / ah > a { (ah * a, ah) } else { (aw, aw / a) };
                Rect { x: (w - tw) / 2.0, y: (h - th) / 2.0, w: tw, h: th }
            }
            _ => Rect { x: 0.0, y: 0.0, w, h },
        };
        let line = self.line.as_ref().map(|l| {
            let (i, t) = (self.px(l.inset_mm), self.px(l.width_mm).max(1.0));
            (Rect { x: trim.x + i, y: trim.y + i, w: (trim.w - 2.0 * i).max(1.0), h: (trim.h - 2.0 * i).max(1.0) }, t)
        });
        let texts: Vec<Placed> = self.texts.iter().enumerate().filter(|(_, hl)| !hl.text.trim().is_empty()).map(|(i, hl)| {
            let scale = scale_for(&hl.face, hl.size_pt, self.dpi);
            let sf = (*hl.face.font).as_scaled(scale);
            let (ascent, descent) = (sf.ascent() as f64, sf.descent() as f64);
            let step = ascent - descent + sf.line_gap() as f64;
            let rows: Vec<&str> = hl.text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
            let widths: Vec<f64> = rows.iter().map(|l| text_width(&hl.face, l, scale)).collect();
            let tw = widths.iter().copied().fold(0.0, f64::max);
            let th = ascent - descent + step * (rows.len() - 1) as f64;
            // the anchor point of the sheet, and the same point of the text box on it
            let fx = match hl.align {
                Align::Left => 0.0,
                Align::Center => 0.5,
                Align::Right => 1.0,
            };
            let fy = match hl.valign {
                VAlign::Top => 0.0,
                VAlign::Middle => 0.5,
                VAlign::Bottom => 1.0,
            };
            let x = trim.x + trim.w * fx - tw * fx + self.px(hl.dx_mm);
            let top = trim.y + trim.h * fy - th * fy + self.px(hl.dy_mm);
            let lf = match hl.lines {
                Align::Left => 0.0,
                Align::Center => 0.5,
                Align::Right => 1.0,
            };
            let lines = widths.iter().enumerate().map(|(k, lw)| (x + (tw - lw) * lf, top + ascent + step * k as f64)).collect();
            Placed { i, rect: Rect { x, y: top, w: tw, h: th }, lines, scale }
        }).collect();
        // the stills keep inside the line and clear of a headline at the top or bottom
        let m = self.px(self.margin_mm);
        let clear = self.px(CLEARANCE);
        let mut inset = [m; 4]; // top, right, bottom, left
        if let (Some((r, t)), Some(l)) = (&line, &self.line) {
            let need = r.x - trim.x + t * l.pattern.extent() + clear;
            inset.iter_mut().for_each(|i| *i = i.max(need));
        }
        for t in &texts {
            let (r, hl) = (&t.rect, &self.texts[t.i]);
            // the stills keep a text's space below / above it
            match hl.valign {
                VAlign::Top if r.y + r.h < trim.y + trim.h / 2.0 => inset[0] = inset[0].max(r.y + r.h - trim.y + self.px(hl.pad_mm[2])),
                VAlign::Bottom if r.y > trim.y + trim.h / 2.0 => inset[2] = inset[2].max(trim.y + trim.h - r.y + self.px(hl.pad_mm[0])),
                _ => {} // in the middle a text lies over the stills
            }
        }
        let area = Rect {
            x: trim.x + inset[3],
            y: trim.y + inset[0],
            w: (trim.w - inset[1] - inset[3]).max(1.0),
            h: (trim.h - inset[0] - inset[2]).max(1.0),
        };
        Plan { trim, area, line, texts }
    }

    /// Grid for `n` stills of width/height `aspect`.
    pub fn grid(&self, n: usize, aspect: f64) -> Grid {
        let plan = self.plan();
        let area = plan.area;
        let n = n.max(1) as u32;
        let g = self.px(self.gap_mm);
        // the stills flow around texts that say so: their boxes with the space around them stay free
        let keep_out: Vec<Rect> = plan
            .texts
            .iter()
            .filter(|t| self.texts[t.i].wrap)
            .map(|t| {
                let (r, [top, rt, b, l]) = (t.rect, self.texts[t.i].pad_mm.map(|v| self.px(v)));
                Rect { x: r.x - l, y: r.y - top, w: r.w + l + rt, h: r.h + top + b }
            })
            .collect();
        let keep_out = if self.mosaic.is_some() { vec![] } else { keep_out }; // texts lie over a mosaic
        let place = |cols: u32, rows: u32| {
            let cw = ((area.w - (cols - 1) as f64 * g) / cols as f64).min((area.h - (rows - 1) as f64 * g) / rows as f64 * aspect).max(0.0);
            let ch = cw / aspect;
            let (bw, bh) = (cols as f64 * cw + (cols - 1) as f64 * g, rows as f64 * ch + (rows - 1) as f64 * g);
            Grid { cols, rows, cell_w: cw, cell_h: ch, x: area.x + (area.w - bw) / 2.0, y: area.y + (area.h - bh) / 2.0, gap: g, keep_out: keep_out.clone() }
        };
        // as many rows as the stills need beside the cells the headline takes (a few more rows can free a cell)
        let fit = |cols: u32| {
            let mut rows = n.div_ceil(cols);
            loop {
                let grid = place(cols, rows);
                if grid.slots() >= n as usize || rows > n + 50 {
                    return grid;
                }
                rows += 1;
            }
        };
        if let Some(m) = &self.mosaic {
            // tiles touch: a gap would break up the main picture
            let g = 0.0;
            let (cols, rows) = (m.cols.max(1), m.rows(aspect));
            let cw = (area.w / cols as f64).min(area.h / rows as f64 * aspect).max(0.0);
            let ch = cw / aspect;
            let (bw, bh) = (cols as f64 * cw, rows as f64 * ch);
            return Grid { cols, rows, cell_w: cw, cell_h: ch, x: area.x + (area.w - bw) / 2.0, y: area.y + (area.h - bh) / 2.0, gap: g, keep_out: vec![] };
        }
        match self.columns {
            Some(c) => fit(c.clamp(1, n)),
            None => (1..=n).map(fit).fold(None::<Grid>, |best, c| match best {
                Some(b) if b.cell_w >= c.cell_w => Some(b),
                _ => Some(c),
            }).expect("n >= 1"),
        }
    }
}

impl Grid {
    /// Whether grid position `k` (row by row) is free for a still.
    fn free(&self, k: u32) -> bool {
        let (c, r) = (k % self.cols, k / self.cols);
        let x = self.x + c as f64 * (self.cell_w + self.gap);
        let y = self.y + r as f64 * (self.cell_h + self.gap);
        !self.keep_out.iter().any(|o| x < o.x + o.w && o.x < x + self.cell_w && y < o.y + o.h && o.y < y + self.cell_h)
    }

    /// How many stills the grid holds (its cells less those the texts keep free).
    pub fn slots(&self) -> usize {
        (0..self.cols * self.rows).filter(|&k| self.free(k)).count()
    }

    /// Cell of still `i` as whole pixels (x, y, w, h): the `i`-th free grid position. Neighbouring cells share their
    /// edge pixel when the gap is 0. x and y can be below 0: with a negative margin the stills run off the page.
    pub fn cell(&self, i: usize) -> (i64, i64, u32, u32) {
        let k = if !self.keep_out.is_empty() { (0..).filter(|&k| self.free(k)).nth(i).unwrap_or(i as u32) } else { i as u32 };
        let (c, r) = (k % self.cols, k / self.cols);
        let x = self.x + c as f64 * (self.cell_w + self.gap);
        let y = self.y + r as f64 * (self.cell_h + self.gap);
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        (x0, y0, ((x + self.cell_w).round() as i64 - x0).max(1) as u32, ((y + self.cell_h).round() as i64 - y0).max(1) as u32)
    }
}

/// Film formats offered for cropping, as width : height (presets store the index).
pub const PICTURE: [(f64, f64); 9] = [(4.0, 3.0), (1.66, 1.0), (16.0, 9.0), (1.85, 1.0), (1.9, 1.0), (2.0, 1.0), (2.35, 1.0), (2.39, 1.0), (2.4, 1.0)];

/// Formats as the film world names them, with the ratio they really have where it differs: DCI Scope is 2048 × 858
/// / 4096 × 1716 = 2.387 and is called 2.39:1, the DCI container 2048 × 1080 / 4096 × 2160 = 1.896 is called 1.90:1.
const NOMINAL: [(f64, (f64, f64)); 10] = [
    (4.0 / 3.0, (4.0, 3.0)),
    (1.66, (1.66, 1.0)),
    (16.0 / 9.0, (16.0, 9.0)),
    (1.85, (1.85, 1.0)),
    (256.0 / 135.0, (1.9, 1.0)),
    (2.0, (2.0, 1.0)),
    (2.2, (2.2, 1.0)),
    (2.35, (2.35, 1.0)),
    (2048.0 / 858.0, (2.39, 1.0)),
    (2.4, (2.4, 1.0)),
];

/// The named format a picture of width / height `aspect` is (as width : height), if it is within the rounding of a
/// pixel row of one: 2048 × 858 → 2.39:1, 1920 × 804 → 2.39:1, 1920 × 800 → 2.4:1, 1998 × 1080 → 1.85:1.
pub fn nominal(aspect: f64) -> Option<(f64, f64)> {
    NOMINAL.iter().map(|&(a, n)| ((a - aspect).abs(), n)).filter(|&(d, _)| d < 0.006).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, n)| n)
}

/// Social media sizes for the paper list, in pixels, with the platforms that use them ("" = square).
pub const SOCIAL: [(u32, u32, &str); 7] = [
    (1080, 1920, "Reels, TikTok, Shorts, Stories"),
    (1080, 1350, "Instagram, Facebook"),
    (1080, 1440, "Instagram"),
    (1080, 1080, ""),
    (1000, 1500, "Pinterest"),
    (1200, 628, "Facebook, LinkedIn"),
    (1920, 1080, "YouTube, X"),
];

/// "2.39", "16", "1" – a format number without needless decimals.
pub fn ratio_text(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// "2.39", "2,39", "21:9", "2.39:1" → width / height; None if not a sensible picture format.
pub fn parse_aspect(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let v = match s.split_once(':') {
        Some((w, h)) => w.trim().parse::<f64>().ok()? / h.trim().parse::<f64>().ok()?,
        None => s.parse().ok()?,
    };
    (0.3..=5.0).contains(&v).then_some(v)
}

/// Cuts the still to `aspect` around its centre: letterbox bars (top/bottom) for wider formats,
/// pillarbox bars (left/right) for narrower ones.
pub fn crop(img: &RgbImage, aspect: f64) -> RgbImage {
    let (w, h) = img.dimensions();
    let now = w as f64 / h as f64;
    let (cw, ch) = if aspect > now {
        (w, ((w as f64 / aspect).round() as u32).clamp(1, h))
    } else {
        (((h as f64 * aspect).round() as u32).clamp(1, w), h)
    };
    imageops::crop_imm(img, (w - cw) / 2, (h - ch) / 2, cw, ch).to_image()
}

/// The still scaled to cover the whole cell, the overhang cut off around the centre – no white edges, whatever
/// the rounding or a slightly different aspect.
pub fn cover(img: &RgbImage, cw: u32, ch: u32) -> RgbImage {
    let s = (cw as f64 / img.width() as f64).max(ch as f64 / img.height() as f64);
    let (iw, ih) = (((img.width() as f64 * s).ceil() as u32).max(cw), ((img.height() as f64 * s).ceil() as u32).max(ch));
    let big = imageops::resize(img, iw, ih, imageops::FilterType::CatmullRom);
    imageops::crop_imm(&big, (iw - cw) / 2, (ih - ch) / 2, cw, ch).to_image()
}

fn fill(page: &mut RgbImage, x0: f64, y0: f64, x1: f64, y1: f64, color: [u8; 3]) {
    let (w, h) = page.dimensions();
    let clampx = |v: f64| (v.round().max(0.0) as u32).min(w);
    let clampy = |v: f64| (v.round().max(0.0) as u32).min(h);
    for y in clampy(y0)..clampy(y1) {
        for x in clampx(x0)..clampx(x1) {
            page.put_pixel(x, y, Rgb(color));
        }
    }
}

/// Fills `r` but leaves out where it meets one of `holes` (a horizontal edge loses stretches of its length, a
/// vertical edge stretches of its height).
fn fill_except(page: &mut RgbImage, r: Rect, holes: &[Rect], color: [u8; 3]) {
    let across = r.w >= r.h;
    let (start, end) = if across { (r.x, r.x + r.w) } else { (r.y, r.y + r.h) };
    let mut gaps: Vec<(f64, f64)> = holes
        .iter()
        .filter(|h| h.x < r.x + r.w && r.x < h.x + h.w && h.y < r.y + r.h && r.y < h.y + h.h)
        .map(|h| if across { (h.x, h.x + h.w) } else { (h.y, h.y + h.h) })
        .collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut at = start;
    for (g0, g1) in gaps.into_iter().chain([(end, end)]) {
        let to = g0.min(end);
        if to > at {
            if across { fill(page, at, r.y, to, r.y + r.h, color) } else { fill(page, r.x, at, r.x + r.w, to, color) }
        }
        at = at.max(g1);
    }
}

/// Coverage of a text (0–1 per pixel) in a buffer whose top left is (`x0`, `y0`) on the page.
struct Mask {
    x0: i64,
    y0: i64,
    w: usize,
    h: usize,
    a: Vec<f32>,
}

impl Mask {
    fn get(&self, x: i64, y: i64) -> f32 {
        let (x, y) = (x - self.x0, y - self.y0);
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h { 0.0 } else { self.a[y as usize * self.w + x as usize] }
    }
}

/// The lines of a text as coverage, with `room` px free around them (for the shadow to spread into); fake bold
/// widens the strokes, fake italic slants. Clipped to what can reach the page of `page` px: the page itself and the
/// page moved back by the shadow's `shift` (at most a page each way), plus `room`; so a huge or far-off text (an odd
/// project file) never needs more than a few pages of memory.
fn text_mask(hl: &Headline, t: &Placed, room: f64, page: (u32, u32), shift: (i64, i64)) -> Mask {
    let font = &*hl.face.font;
    let scale = t.scale;
    let sf = font.as_scaled(scale);
    let bold_steps = if hl.face.fake_bold { (scale.y / 28.0).ceil().max(1.0) as i64 } else { 0 };
    let slant = if hl.face.fake_italic { 0.2 } else { 0.0 };
    // glyphs can reach a little beyond the box (accents, italics, overhangs)
    let pad = room + scale.y as f64 * 0.5 + bold_steps as f64;
    let clip = |lo: f64, hi: f64, size: u32, s: i64| {
        let s = s.clamp(-(size as i64), size as i64) as f64;
        let (from, to) = (0f64.min(-s) - room, (size as f64).max(size as f64 - s) + room);
        (lo.max(from).floor() as i64, hi.min(to).ceil() as i64)
    };
    let (x0, x1) = clip(t.rect.x - pad, t.rect.x + t.rect.w + pad, page.0, shift.0);
    let (y0, y1) = clip(t.rect.y - pad, t.rect.y + t.rect.h + pad, page.1, shift.1);
    let (w, h) = ((x1 - x0).max(0) as usize + 1, (y1 - y0).max(0) as usize + 1);
    let mut m = Mask { x0, y0, w, h, a: vec![0.0; w * h] };
    for (line, &(x, baseline)) in hl.text.split('\n').map(|l| l.trim_end_matches('\r')).zip(&t.lines) {
        let mut caret = x as f32;
        let mut prev = None;
        for c in line.chars() {
            let id = sf.glyph_id(c);
            if let Some(p) = prev {
                caret += sf.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(caret, baseline as f32));
            caret += sf.h_advance(id);
            prev = Some(id);
            let Some(outline) = font.outline_glyph(glyph) else { continue };
            let b = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let py = b.min.y as i64 + gy as i64;
                let shift = (slant * (baseline as f32 - py as f32)).round() as i64;
                for dx in 0..=bold_steps {
                    let (mx, my) = (b.min.x as i64 + gx as i64 + shift + dx - x0, py - y0);
                    if mx >= 0 && my >= 0 && (mx as usize) < w && (my as usize) < h {
                        let v = &mut m.a[my as usize * w + mx as usize];
                        *v = v.max(coverage);
                    }
                }
            });
        }
    }
    m
}

/// Box blur of radius `r` along rows (`horizontal`) or columns, in place.
fn box_blur(a: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    if r == 0 {
        return;
    }
    let (n, lines) = if horizontal { (w, h) } else { (h, w) };
    let idx = |line: usize, k: usize| if horizontal { line * w + k } else { k * w + line };
    let mut buf = vec![0.0f32; n];
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = 0.0f32;
        for k in 0..=r.min(n - 1) {
            sum += a[idx(line, k)];
        }
        for k in 0..n {
            buf[k] = sum * norm;
            if k + r + 1 < n {
                sum += a[idx(line, k + r + 1)];
            }
            if k >= r {
                sum -= a[idx(line, k - r)];
            }
        }
        for k in 0..n {
            a[idx(line, k)] = buf[k];
        }
    }
}

/// Square dilation of radius `r` (Photoshop's Spread hardens the shadow by growing it before the blur): a sliding
/// maximum with a deque, so its cost does not grow with the radius.
fn dilate(a: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 {
        return;
    }
    for horizontal in [true, false] {
        let (n, lines) = if horizontal { (w, h) } else { (h, w) };
        let idx = |line: usize, k: usize| if horizontal { line * w + k } else { k * w + line };
        let mut buf = vec![0.0f32; n];
        let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        for line in 0..lines {
            q.clear();
            // window of output k is k - r ..= k + r; j runs r ahead of k
            for j in 0..n + r {
                if j < n {
                    while q.back().is_some_and(|&b| a[idx(line, b)] <= a[idx(line, j)]) {
                        q.pop_back();
                    }
                    q.push_back(j);
                }
                if j >= r {
                    let k = j - r;
                    while q.front().is_some_and(|&f| f + r < k) {
                        q.pop_front();
                    }
                    buf[k] = q.front().map_or(0.0, |&f| a[idx(line, f)]);
                }
            }
            for k in 0..n {
                a[idx(line, k)] = buf[k];
            }
        }
    }
}

/// The drop shadow of a text: the coverage grown by spread, blurred to its size, shaped by the contour, roughened
/// by noise, then mixed into the page with its blend mode and opacity, moved by the distance away from the light.
fn draw_shadow(page: &mut RgbImage, sheet: &Sheet, text: &Mask, sh: &Shadow) {
    let size = sheet.px(sh.size_mm).max(0.0);
    let choke = (size * sh.spread.clamp(0.0, 1.0)).round() as usize;
    let blur = size * (1.0 - sh.spread.clamp(0.0, 1.0));
    let mut a = text.a.clone();
    dilate(&mut a, text.w, text.h, choke);
    // three box passes are close to a Gaussian; together they reach `blur` px out
    let r = (blur / 3.0).round() as usize;
    for _ in 0..3 {
        box_blur(&mut a, text.w, text.h, r, true);
        box_blur(&mut a, text.w, text.h, r, false);
    }
    let lut = sh.contour.table(sh.anti_aliased);
    let (ox, oy) = sh.offset(sheet);
    let (pw, ph) = page.dimensions();
    let c = sh.color.map(|v| v as f32 / 255.0);
    for my in 0..text.h {
        for mx in 0..text.w {
            let v = a[my * text.w + mx];
            if v <= 0.0 {
                continue;
            }
            let (px, py) = (text.x0 + mx as i64 + ox, text.y0 + my as i64 + oy);
            if px < 0 || py < 0 || px >= pw as i64 || py >= ph as i64 {
                continue;
            }
            let mut alpha = lut[(v.clamp(0.0, 1.0) * 255.0).round() as usize] * sh.opacity as f32;
            if sh.noise > 0.0 {
                // a fixed pattern per pixel, so the preview does not flicker
                let hsh = ((px as u64).wrapping_mul(73856093) ^ (py as u64).wrapping_mul(19349663)).wrapping_mul(0x9E3779B97F4A7C15) >> 40;
                alpha *= 1.0 - sh.noise as f32 * (hsh as f32 / (1u64 << 24) as f32);
            }
            if sh.knock_out {
                alpha *= 1.0 - text.get(px, py);
            }
            if alpha <= 0.0 {
                continue;
            }
            let p = page.get_pixel_mut(px as u32, py as u32);
            for k in 0..3 {
                let b = p[k] as f32 / 255.0;
                p[k] = ((b + (sh.blend.mix(b, c[k]) - b) * alpha) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// A text (and its shadow, under it) on the page.
fn draw_text(page: &mut RgbImage, sheet: &Sheet, hl: &Headline, t: &Placed) {
    // room for the shadow to spread; its offset is allowed for by the clip
    let room = hl.shadow.as_ref().map_or(0.0, |s| sheet.px(s.size_mm) + 2.0);
    let shift = hl.shadow.as_ref().map_or((0, 0), |s| s.offset(sheet));
    let m = text_mask(hl, t, room, page.dimensions(), shift);
    if let Some(sh) = &hl.shadow {
        draw_shadow(page, sheet, &m, sh);
    }
    let (pw, ph) = page.dimensions();
    for my in 0..m.h {
        for mx in 0..m.w {
            let cov = m.a[my * m.w + mx];
            let (px, py) = (m.x0 + mx as i64, m.y0 + my as i64);
            if cov <= 0.0 || px < 0 || py < 0 || px >= pw as i64 || py >= ph as i64 {
                continue;
            }
            let p = page.get_pixel_mut(px as u32, py as u32);
            for k in 0..3 {
                p[k] = (p[k] as f32 * (1.0 - cov) + hl.color[k] as f32 * cov).round() as u8;
            }
        }
    }
}

/// The mosaic's tiles: each still is scaled once (cells differ by a pixel at most, so one size serves all), tinted
/// towards its tile's colour, then the main picture is blended over the whole block.
fn draw_mosaic(page: &mut RgbImage, m: &Mosaic, grid: &Grid, tile: &mut impl FnMut(usize, u32, u32) -> Option<RgbImage>) {
    let cells = (grid.cols * grid.rows) as usize;
    let (cw, ch) = (grid.cell_w.ceil().max(1.0) as u32, grid.cell_h.ceil().max(1.0) as u32);
    // the main picture at the size of the block, for the tile colours and the blend
    let (x0, y0) = (grid.x.round() as i64, grid.y.round() as i64);
    let (bw, bh) = ((grid.cols as f64 * grid.cell_w + (grid.cols - 1) as f64 * grid.gap).round().max(1.0) as u32, (grid.rows as f64 * grid.cell_h + (grid.rows - 1) as f64 * grid.gap).round().max(1.0) as u32);
    let target = imageops::resize(&*m.target, bw, bh, imageops::FilterType::Triangle);
    let mut scaled: std::collections::HashMap<usize, Option<(RgbImage, [f32; 3])>> = std::collections::HashMap::new();
    for c in 0..cells {
        let (x, y, w, h) = grid.cell(c);
        let Some(&i) = m.tiles.get(c) else { continue };
        let Some((img, mean)) = scaled.entry(i).or_insert_with(|| tile(i, cw, ch).map(|t| { let mean = crate::mosaic::mean(&t); (t, mean) })).clone() else { continue };
        let img = if img.dimensions() == (w, h) { img } else { imageops::resize(&img, w, h, imageops::FilterType::Triangle) };
        // the tile's colour in the main picture
        let (tx, ty) = ((x - x0).max(0) as u32, (y - y0).max(0) as u32);
        let part = imageops::crop_imm(&target, tx.min(bw - 1), ty.min(bh - 1), w.min(bw - tx.min(bw - 1)), h.min(bh - ty.min(bh - 1))).to_image();
        let want = crate::mosaic::mean(&part);
        let shift: [f32; 3] = std::array::from_fn(|k| (want[k] - mean[k]) * m.tint as f32);
        let (pw, ph) = page.dimensions();
        for (px, py, p) in img.enumerate_pixels() {
            let (gx, gy) = (x + px as i64, y + py as i64);
            if gx < 0 || gy < 0 || gx >= pw as i64 || gy >= ph as i64 {
                continue;
            }
            let t = part.get_pixel(px.min(part.width() - 1), py.min(part.height() - 1));
            let out = page.get_pixel_mut(gx as u32, gy as u32);
            for k in 0..3 {
                let v = p[k] as f32 + shift[k];
                out[k] = (v + (t[k] as f32 - v) * m.overlay as f32).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Draws the sheet. `still(i)` delivers still i (any size; it covers its cell). None leaves the cell grey.
/// Stills are loaded one at a time, so a big sheet never holds all.
pub fn render(sheet: &Sheet, n: usize, aspect: f64, mut still: impl FnMut(usize) -> Option<RgbImage>) -> RgbImage {
    let grid = sheet.grid(n, aspect);
    compose(sheet, n, &grid, |i, cw, ch| still(i).map(|img| cover(&img, cw, ch)))
}

/// Like `render`, for the preview: `tile(i, width, height)` gives still `i` already at its cell size (a cache can
/// answer it), and the tiles are made on all cores.
pub fn render_tiles(sheet: &Sheet, n: usize, aspect: f64, tile: impl Fn(usize, u32, u32) -> Option<RgbImage> + Sync) -> RgbImage {
    let grid = sheet.grid(n, aspect);
    let mut tiles: Vec<Option<RgbImage>> = vec![None; n];
    let per = n.div_ceil(std::thread::available_parallelism().map_or(4, |c| c.get())).max(1);
    std::thread::scope(|s| {
        for (k, chunk) in tiles.chunks_mut(per).enumerate() {
            let (grid, tile) = (&grid, &tile);
            s.spawn(move || {
                for (j, t) in chunk.iter_mut().enumerate() {
                    let (_, _, cw, ch) = grid.cell(k * per + j);
                    *t = tile(k * per + j, cw, ch);
                }
            });
        }
    });
    // each still is asked for once (a mosaic asks in its own order)
    compose(sheet, n, &grid, |i, _, _| tiles.get_mut(i).and_then(Option::take))
}

fn compose(sheet: &Sheet, n: usize, grid: &Grid, mut tile: impl FnMut(usize, u32, u32) -> Option<RgbImage>) -> RgbImage {
    let (w, h) = sheet.size_px();
    let mut page = RgbImage::from_pixel(w, h, Rgb(if sheet.trim.is_some() { [255, 255, 255] } else { sheet.background }));
    let plan = sheet.plan();
    if sheet.trim.is_some() {
        // bleed up to where the crop marks start, so a cut a little off the line shows no white edge
        let (r, b) = (plan.trim, sheet.px(MARK_OFFSET));
        fill(&mut page, r.x - b, r.y - b, r.x + r.w + b, r.y + r.h + b, sheet.background);
    }
    match &sheet.mosaic {
        Some(m) => draw_mosaic(&mut page, m, grid, &mut tile),
        None => {
            for i in 0..n {
                let (x, y, cw, ch) = grid.cell(i);
                let tile = tile(i, cw, ch).unwrap_or_else(|| RgbImage::from_pixel(cw, ch, Rgb([200, 200, 200])));
                imageops::overlay(&mut page, &tile, x, y);
            }
        }
    }
    if let (Some((outer, t)), Some(l)) = (plan.line, &sheet.line) {
        // every text cuts a gap into every strand it covers
        let p = sheet.px(BREAK_PAD);
        let holes: Vec<Rect> = plan.texts.iter().map(|t| Rect { x: t.rect.x - p, y: t.rect.y - p, w: t.rect.w + 2.0 * p, h: t.rect.h + 2.0 * p }).collect();
        for (off, width) in l.pattern.strands() {
            let (o, sw) = (off * t, (width * t).max(1.0));
            let r = Rect { x: outer.x + o, y: outer.y + o, w: outer.w - 2.0 * o, h: outer.h - 2.0 * o };
            if r.w <= 2.0 * sw || r.h <= 2.0 * sw {
                continue;
            }
            for edge in [
                Rect { x: r.x, y: r.y, w: r.w, h: sw },
                Rect { x: r.x, y: r.y + r.h - sw, w: r.w, h: sw },
                Rect { x: r.x, y: r.y, w: sw, h: r.h },
                Rect { x: r.x + r.w - sw, y: r.y, w: sw, h: r.h },
            ] {
                fill_except(&mut page, edge, &holes, l.color);
            }
        }
    }
    for t in &plan.texts {
        draw_text(&mut page, sheet, &sheet.texts[t.i], t);
    }
    if sheet.trim.is_some() {
        let r = plan.trim;
        let (off, len, t) = (sheet.px(MARK_OFFSET), sheet.px(MARK_LENGTH), sheet.px(0.1).max(1.0));
        let black = [0, 0, 0];
        for (cx, sx) in [(r.x, -1.0), (r.x + r.w, 1.0)] {
            for (cy, sy) in [(r.y, -1.0), (r.y + r.h, 1.0)] {
                // horizontal mark beside the corner, vertical mark above / below it
                let (hx0, hx1) = if sx < 0.0 { (cx - off - len, cx - off) } else { (cx + off, cx + off + len) };
                fill(&mut page, hx0, cy - t / 2.0, hx1, cy + t / 2.0, black);
                let (vy0, vy1) = if sy < 0.0 { (cy - off - len, cy - off) } else { (cy + off, cy + off + len) };
                fill(&mut page, cx - t / 2.0, vy0, cx + t / 2.0, vy1, black);
            }
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a4(columns: Option<u32>) -> Sheet {
        // 10 px per mm
        Sheet { width_mm: 210.0, height_mm: 297.0, dpi: 254.0, margin_mm: 10.0, gap_mm: 2.0, columns, ..Default::default() }
    }

    #[test]
    fn auto_grid_picks_the_largest_stills() {
        let s = a4(None);
        assert_eq!(s.size_px(), (2100, 2970));
        let g = s.grid(12, 16.0 / 9.0);
        // 2 columns × 6 rows: (2770 - 5 gaps) / 6 = 445 high → 791 wide; 3 columns would give only 620
        assert_eq!((g.cols, g.rows), (2, 6));
        assert!((g.cell_h - 445.0).abs() < 1e-9);
        assert!((g.y - 100.0).abs() < 1e-9);
        assert!((g.x * 2.0 + 2.0 * g.cell_w + 20.0 - 2100.0).abs() < 1e-9);
    }

    #[test]
    fn crops_letterbox_and_pillarbox_around_the_centre() {
        assert_eq!(parse_aspect("2,39"), Some(2.39));
        assert_eq!(parse_aspect("21:9"), Some(21.0 / 9.0));
        assert_eq!(parse_aspect("2.39:1"), Some(2.39));
        assert_eq!(parse_aspect("0"), None);
        assert_eq!(parse_aspect("x"), None);
        let img = RgbImage::from_fn(1920, 1080, |_, y| if (138..942).contains(&y) { Rgb([255, 0, 0]) } else { Rgb([0, 0, 0]) });
        let c = crop(&img, 2.39);
        assert_eq!(c.dimensions(), (1920, 803));
        assert!(c.pixels().all(|p| p == &Rgb([255, 0, 0])));
        assert_eq!(crop(&img, 4.0 / 3.0).dimensions(), (1440, 1080));
    }

    #[test]
    fn background_fills_the_page_or_trim_plus_bleed() {
        let blue = [20, 40, 200];
        let page = render(&Sheet { background: blue, ..a4(None) }, 0, 1.5, |_| None);
        assert_eq!(page.get_pixel(1, 1), &Rgb(blue));
        let s = Sheet { background: blue, trim: Some(2.0), ..a4(None) };
        let (page, r) = (render(&s, 0, 1.5, |_| None), s.plan().trim);
        // 1 mm (10 px) outside the trim edge: bleed; 5 mm out: white paper beside the crop marks
        assert_eq!(page.get_pixel((r.x - 10.0) as u32, (r.y + r.h / 2.0) as u32), &Rgb(blue));
        assert_eq!(page.get_pixel((r.x - 50.0).max(0.0) as u32, (r.y + r.h / 2.0) as u32), &Rgb([255, 255, 255]));
    }

    #[test]
    fn no_white_between_stills_at_gap_zero() {
        // odd sizes on purpose: cells of fractional width, stills of a slightly different aspect
        let s = Sheet { width_mm: 211.3, height_mm: 149.7, dpi: 173.0, margin_mm: 7.3, gap_mm: 0.0, columns: Some(7), ..Default::default() };
        let page = render(&s, 40, 1.85, |_| Some(RgbImage::from_pixel(1920, 1080, Rgb([255, 0, 0]))));
        let g = s.grid(40, 1.85);
        let (x0, y0, _, _) = g.cell(0);
        let (x1, y1, w1, h1) = g.cell(34); // last full row, last column
        let (x0, y0, x1, y1) = (x0 as u32, y0 as u32, x1 as u32, y1 as u32);
        for y in y0..y1 + h1 {
            for x in x0..x1 + w1 {
                assert_eq!(page.get_pixel(x, y), &Rgb([255, 0, 0]), "white at {x},{y}");
            }
        }
    }

    #[test]
    fn trim_area_has_the_picture_format_and_crop_marks() {
        let s = Sheet { trim: Some(2.0), ..a4(None) };
        let p = s.plan();
        assert!((p.trim.w / p.trim.h - 2.0).abs() < 1e-9);
        // width-limited on portrait A4: paper minus 12 mm mark room each side
        assert!((p.trim.w - (2100.0 - 240.0)).abs() < 1e-9);
        assert!((p.area.x - (p.trim.x + 100.0)).abs() < 1e-9);
        let page = render(&s, 0, 2.0, |_| None);
        // a mark left of the top left corner, 3–9 mm out; nothing at the corner itself
        let y = p.trim.y.round() as u32;
        assert_eq!(page.get_pixel((p.trim.x - 50.0) as u32, y), &Rgb([0, 0, 0]));
        assert_eq!(page.get_pixel((p.trim.x - 10.0) as u32, y), &Rgb([255, 255, 255]));
    }

    fn face() -> Option<crate::fonts::Face> {
        crate::fonts::face("Helvetica", false, false).or_else(|| crate::fonts::face(crate::fonts::families().first()?, false, false))
    }

    #[test]
    fn headline_hangs_at_its_anchor_and_breaks_only_a_line_it_covers() {
        let Some(face) = face() else { return }; // no fonts (not on a Mac)
        let hl = |dy: f64| Headline { text: "Rough Cut 3".into(), face: face.clone(), size_pt: 24.0, color: [200, 0, 0], align: Align::Center, valign: VAlign::Top, lines: Align::Center, shadow: None, dx_mm: 0.0, dy_mm: dy, pad_mm: [2.0; 4], wrap: false };
        let line = Line { width_mm: 0.5, inset_mm: 8.0, color: [0, 0, 0], pattern: Pattern::Single };
        // 20 mm down from the middle of the top edge: below the line, the line stays whole
        let s = Sheet { margin_mm: 5.0, line: Some(line.clone()), texts: vec![hl(20.0)], ..a4(None) };
        let p = s.plan();
        let tr = p.texts[0].rect;
        assert!((tr.y - 200.0).abs() < 1e-6);
        assert!((tr.x + tr.w / 2.0 - 1050.0).abs() < 1e-6);
        assert!(p.area.y >= tr.y + tr.h, "stills below the headline");
        let page = render(&s, 0, 1.0, |_| None);
        assert_eq!(page.get_pixel(1050, 82), &Rgb([0, 0, 0]), "line whole above the headline");
        // moved onto the line: the line breaks under the text
        let mut s2 = s.clone();
        // centre the text on the line (82.5 px = 8.25 mm; 10 px per mm)
        s2.texts = vec![hl((82.5 - tr.h / 2.0) / 10.0)];
        let tr2 = s2.plan().texts[0].rect;
        assert!(tr2.y < 82.0 && tr2.y + tr2.h > 82.0);
        let page = render(&s2, 0, 1.0, |_| None);
        assert_eq!(page.get_pixel((tr2.x - 10.0) as u32, 82), &Rgb([255, 255, 255]), "gap before the text");
        assert_eq!(page.get_pixel(200, 82), &Rgb([0, 0, 0]), "line goes on beside it");
        // the space below a top headline keeps the stills away
        let mut s3 = s.clone();
        s3.texts = vec![Headline { pad_mm: [0.0, 0.0, 30.0, 0.0], ..hl(20.0) }];
        assert!(s3.plan().area.y >= tr.y + tr.h + 300.0 - 1e-6);
    }

    #[test]
    fn stills_flow_around_a_headline_over_them() {
        let Some(face) = face() else { return };
        let hl = Headline {
            text: "Rough Cut 3".into(),
            face,
            size_pt: 40.0,
            color: [0, 0, 0],
            align: Align::Center,
            valign: VAlign::Middle,
            lines: Align::Center,
            shadow: None,
            dx_mm: 0.0,
            dy_mm: 0.0,
            pad_mm: [3.0; 4],
            wrap: true,
        };
        let s = Sheet { texts: vec![hl.clone()], ..a4(Some(3)) };
        let g = s.grid(12, 16.0 / 9.0);
        // the middle cells are kept free, so the grid has more rows and still holds all 12
        assert!(g.rows > 4 && g.slots() >= 12, "{g:?}");
        let r = s.plan().texts[0].rect;
        let o = Rect { x: r.x - 30.0, y: r.y - 30.0, w: r.w + 60.0, h: r.h + 60.0 };
        for i in 0..12 {
            let (x, y, w, h) = g.cell(i);
            let (x, y, w, h) = (x as f64, y as f64, w as f64, h as f64);
            assert!(!(x < o.x + o.w && o.x < x + w && y < o.y + o.h && o.y < y + h), "still {i} under the headline");
        }
        // without flowing the stills lie under it, in the usual 3 × 4
        let plain = Sheet { texts: vec![Headline { wrap: false, ..hl }], ..a4(Some(3)) };
        assert_eq!(plain.grid(12, 16.0 / 9.0).rows, 4);
    }

    #[test]
    fn several_texts_of_several_lines_each_break_the_line() {
        let Some(face) = face() else { return };
        let t = |text: &str, align: Align, lines: Align| Headline {
            text: text.into(),
            face: face.clone(),
            size_pt: 20.0,
            color: [0, 0, 0],
            align,
            valign: VAlign::Top,
            lines,
            shadow: None,
            dx_mm: 0.0,
            dy_mm: 6.0,
            pad_mm: [2.0; 4],
            wrap: false,
        };
        let line = Line { width_mm: 0.5, inset_mm: 8.0, color: [0, 0, 0], pattern: Pattern::Single };
        let s = Sheet { line: Some(line), texts: vec![t("Left", Align::Left, Align::Left), t("Wide first line\nB", Align::Right, Align::Right), t("", Align::Center, Align::Center)], ..a4(None) };
        let p = s.plan();
        // the empty one is left out, the others keep their index
        assert_eq!(p.texts.iter().map(|t| t.i).collect::<Vec<_>>(), vec![0, 1]);
        let two = &p.texts[1];
        assert_eq!(two.lines.len(), 2);
        assert!(two.lines[1].1 > two.lines[0].1, "second line below the first");
        // right aligned: the short line starts further right, both end at the box edge
        assert!(two.lines[1].0 > two.lines[0].0);
        let page = render(&s, 0, 1.0, |_| None);
        // the line (at 82–87 px) breaks under both texts and goes on between them
        let y = 84;
        for t in &p.texts {
            assert_eq!(page.get_pixel((t.rect.x - 5.0) as u32, y), &Rgb([255, 255, 255]), "gap at text {}", t.i);
        }
        assert_eq!(page.get_pixel(1050, y), &Rgb([0, 0, 0]), "line between the texts");
    }

    #[test]
    fn dilation_is_a_sliding_maximum() {
        let mut a = vec![0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0];
        dilate(&mut a, 8, 1, 1);
        assert_eq!(a, vec![0.0, 0.5, 0.5, 0.5, 0.0, 1.0, 1.0, 1.0]);
        let mut b = vec![0.0; 9];
        b[4] = 1.0;
        dilate(&mut b, 3, 3, 5);
        assert!(b.iter().all(|&v| v == 1.0), "a radius beyond the size fills all");
    }

    #[test]
    fn drop_shadow_falls_away_from_the_light() {
        let Some(face) = face() else { return };
        let sh = Shadow {
            blend: Blend::Multiply,
            color: [0, 0, 0],
            opacity: 1.0,
            angle: 120.0,
            distance_mm: 3.0,
            spread: 1.0,
            size_mm: 0.0,
            contour: Contour::Linear,
            anti_aliased: false,
            noise: 0.0,
            knock_out: true,
        };
        let t = Headline {
            text: "I".into(),
            face,
            size_pt: 60.0,
            color: [255, 255, 255],
            align: Align::Center,
            valign: VAlign::Middle,
            lines: Align::Center,
            shadow: Some(sh.clone()),
            dx_mm: 0.0,
            dy_mm: 0.0,
            pad_mm: [0.0; 4],
            wrap: false,
        };
        let s = Sheet { background: [200, 200, 200], texts: vec![t.clone()], ..a4(None) };
        let page = render(&s, 0, 1.0, |_| None);
        let p = s.plan();
        let (r, baseline) = (p.texts[0].rect, p.texts[0].lines[0].1);
        // 120°: 30 px away = 15 px right, 26 px down; below the foot of the white I (its baseline) lies its black shadow
        let (cx, foot) = ((r.x + r.w / 2.0) as u32, baseline as u32);
        let dark = (foot + 2..foot + 24).filter(|&y| page.get_pixel(cx + 15, y)[0] < 50).count();
        assert!(dark > 15, "shadow below and right of the text: {dark}");
        assert!((foot + 30..foot + 40).all(|y| page.get_pixel(cx + 15, y)[0] == 200), "and not further down");
        // Screen with a black shadow changes nothing; a blur spreads it beyond the hard edge
        let none = Sheet { texts: vec![Headline { shadow: Some(Shadow { blend: Blend::Screen, ..sh.clone() }), ..t.clone() }], ..s.clone() };
        assert!(render(&none, 0, 1.0, |_| None).pixels().all(|p| p[0] >= 200 || p[0] == p[1]));
        assert_eq!(Blend::Multiply.mix(0.5, 0.5), 0.25);
        assert!((Blend::Screen.mix(0.5, 0.5) - 0.75).abs() < 1e-6);
        assert_eq!(Contour::Linear.table(false)[255], 1.0);
        assert!(Contour::Cone.table(false)[128] > 0.98 && Contour::Cone.table(false)[0] == 0.0);
    }

    #[test]
    fn patterns_draw_their_strands_and_keep_the_stills_inside() {
        // 10 px per mm, 1 mm strands: Two-and-one = strands at 0, 22, 70 px in from 80 px
        let s = Sheet { margin_mm: 1.0, line: Some(Line { width_mm: 1.0, inset_mm: 8.0, color: [0, 0, 0], pattern: Pattern::TwoAndOne }), ..a4(None) };
        let page = render(&s, 0, 1.0, |_| None);
        let at = |x: u32| page.get_pixel(x, 1500)[0];
        assert_eq!((at(85), at(95), at(105), at(125), at(155)), (0, 255, 0, 255, 0));
        assert!(s.plan().area.x >= 80.0 + 80.0);
    }

    #[test]
    fn picture_sizes_get_the_names_the_film_world_uses() {
        let name = |w: f64, h: f64| nominal(w / h).map(|(a, b)| format!("{}:{}", ratio_text(a), ratio_text(b)));
        for (w, h, n) in [
            // DCI (SMPTE ST 428-1): full container, Flat, Scope in 2K and 4K
            (2048.0, 1080.0, "1.9:1"),
            (4096.0, 2160.0, "1.9:1"),
            (1998.0, 1080.0, "1.85:1"),
            (3996.0, 2160.0, "1.85:1"),
            (2048.0, 858.0, "2.39:1"),
            (4096.0, 1716.0, "2.39:1"),
            // HD / UHD / 8K and letterboxed masters in them
            (1280.0, 720.0, "16:9"),
            (1920.0, 1080.0, "16:9"),
            (3840.0, 2160.0, "16:9"),
            (7680.0, 4320.0, "16:9"),
            (1920.0, 1038.0, "1.85:1"),
            (3840.0, 2076.0, "1.85:1"),
            (1920.0, 960.0, "2:1"),
            (3840.0, 1920.0, "2:1"),
            (1920.0, 817.0, "2.35:1"),
            (1920.0, 804.0, "2.39:1"),
            (3840.0, 1608.0, "2.39:1"),
            (1920.0, 800.0, "2.4:1"),
            (3840.0, 1600.0, "2.4:1"),
            (1440.0, 1080.0, "4:3"),
        ] {
            assert_eq!(name(w, h).as_deref(), Some(n), "{w} × {h}");
        }
        assert_eq!(nominal(2.1), None);
    }

    #[test]
    fn a_negative_margin_runs_the_stills_off_the_page() {
        // 10 px per mm; margin -20 mm: the area is 40 mm larger than the page each way
        let s = Sheet { margin_mm: -20.0, gap_mm: 0.0, ..a4(Some(2)) };
        let a = s.plan().area;
        assert!((a.x + 200.0).abs() < 1e-9 && (a.w - 2500.0).abs() < 1e-9);
        let g = s.grid(4, 1.0);
        let (x, y, _, _) = g.cell(0);
        assert!(x < 0 || y < 0, "first cell starts off the page");
        let page = render(&s, 4, 1.0, |_| Some(RgbImage::from_pixel(10, 10, Rgb([255, 0, 0]))));
        // the page is red right to its corners along the sides the grid overruns
        let (w, h) = page.dimensions();
        assert_eq!(page.get_pixel(0, h / 2), &Rgb([255, 0, 0]));
        assert_eq!(page.get_pixel(w - 1, h / 2), &Rgb([255, 0, 0]));
    }

    #[test]
    fn fixed_columns_and_cells() {
        let g = a4(Some(3)).grid(12, 16.0 / 9.0);
        assert_eq!((g.cols, g.rows), (3, 4));
        assert!((g.cell_w - 620.0).abs() < 1e-9);
        assert!((g.x - 100.0).abs() < 1e-9);
        let page = render(&a4(Some(3)), 3, 16.0 / 9.0, |i| (i < 2).then(|| RgbImage::from_pixel(160, 90, Rgb([255, 0, 0]))));
        let mid = |i| {
            let (x, y, w, h) = a4(Some(3)).grid(3, 16.0 / 9.0).cell(i);
            *page.get_pixel(x as u32 + w / 2, y as u32 + h / 2)
        };
        assert_eq!((mid(0), mid(2)), (Rgb([255, 0, 0]), Rgb([200, 200, 200])));
        assert_eq!(page.get_pixel(5, 5), &Rgb([255, 255, 255]));
    }
}
