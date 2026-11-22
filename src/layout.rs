//! The sheet: paper size, resolution, margin and gap → a grid of equal cells filled left to right, top to bottom,
//! centred in its area. Optional: a trim area in the picture format with crop marks, a frame line of parallel
//! strands, a headline hung at any point of the sheet (where it covers the line, the line breaks). Line, headline and grid keep out of each other's way.

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

#[derive(Clone, Debug)]
pub struct Headline {
    pub text: String,
    pub face: Face,
    pub size_pt: f64,
    pub color: [u8; 3],
    /// the point of the sheet (and of the text box) it hangs at: e.g. top centre = the middle of the top edge
    pub align: Align,
    pub valign: VAlign,
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
    pub headline: Option<Headline>,
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet { width_mm: 297.0, height_mm: 210.0, dpi: 300.0, margin_mm: 10.0, gap_mm: 3.0, columns: None, trim: None, line: None, headline: None }
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
    /// the headline's box (x, top, width, height), its baseline and pixel scale
    pub text: Option<(Rect, f64, PxScale)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
    pub cell_w: f64,
    pub cell_h: f64,
    /// top left of the first cell, px
    pub x: f64,
    pub y: f64,
    pub gap: f64,
    /// cells touching this stay empty (the headline with its space, when the stills flow around it)
    pub keep_out: Option<Rect>,
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
        let text = self.headline.as_ref().filter(|hl| !hl.text.trim().is_empty()).map(|hl| {
            let scale = scale_for(&hl.face, hl.size_pt, self.dpi);
            let sf = (*hl.face.font).as_scaled(scale);
            let (ascent, descent) = (sf.ascent() as f64, sf.descent() as f64);
            let tw = text_width(&hl.face, &hl.text, scale);
            let th = ascent - descent;
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
            (Rect { x, y: top, w: tw, h: th }, top + ascent, scale)
        });
        // the stills keep inside the line and clear of a headline at the top or bottom
        let m = self.px(self.margin_mm);
        let clear = self.px(CLEARANCE);
        let mut inset = [m; 4]; // top, right, bottom, left
        if let (Some((r, t)), Some(l)) = (&line, &self.line) {
            let need = r.x - trim.x + t * l.pattern.extent() + clear;
            inset.iter_mut().for_each(|i| *i = i.max(need));
        }
        if let (Some((r, _, _)), Some(hl)) = (&text, &self.headline) {
            // the stills keep the headline's space below / above it
            match hl.valign {
                VAlign::Top if r.y + r.h < trim.y + trim.h / 2.0 => inset[0] = inset[0].max(r.y + r.h - trim.y + self.px(hl.pad_mm[2])),
                VAlign::Bottom if r.y > trim.y + trim.h / 2.0 => inset[2] = inset[2].max(trim.y + trim.h - r.y + self.px(hl.pad_mm[0])),
                _ => {} // in the middle the headline lies over the stills
            }
        }
        let area = Rect {
            x: trim.x + inset[3],
            y: trim.y + inset[0],
            w: (trim.w - inset[1] - inset[3]).max(1.0),
            h: (trim.h - inset[0] - inset[2]).max(1.0),
        };
        Plan { trim, area, line, text }
    }

    /// Grid for `n` stills of width/height `aspect`.
    pub fn grid(&self, n: usize, aspect: f64) -> Grid {
        let plan = self.plan();
        let area = plan.area;
        let n = n.max(1) as u32;
        let g = self.px(self.gap_mm);
        // the stills flow around the headline: its box with the space around it stays free
        let keep_out = match (&plan.text, &self.headline) {
            (Some((r, _, _)), Some(hl)) if hl.wrap => {
                let [t, rt, b, l] = hl.pad_mm.map(|v| self.px(v));
                Some(Rect { x: r.x - l, y: r.y - t, w: r.w + l + rt, h: r.h + t + b })
            }
            _ => None,
        };
        let place = |cols: u32, rows: u32| {
            let cw = ((area.w - (cols - 1) as f64 * g) / cols as f64).min((area.h - (rows - 1) as f64 * g) / rows as f64 * aspect).max(0.0);
            let ch = cw / aspect;
            let (bw, bh) = (cols as f64 * cw + (cols - 1) as f64 * g, rows as f64 * ch + (rows - 1) as f64 * g);
            Grid { cols, rows, cell_w: cw, cell_h: ch, x: area.x + (area.w - bw) / 2.0, y: area.y + (area.h - bh) / 2.0, gap: g, keep_out }
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
        let Some(o) = self.keep_out else { return true };
        let (c, r) = (k % self.cols, k / self.cols);
        let x = self.x + c as f64 * (self.cell_w + self.gap);
        let y = self.y + r as f64 * (self.cell_h + self.gap);
        !(x < o.x + o.w && o.x < x + self.cell_w && y < o.y + o.h && o.y < y + self.cell_h)
    }

    /// How many stills the grid holds (its cells less those the headline keeps free).
    pub fn slots(&self) -> usize {
        (0..self.cols * self.rows).filter(|&k| self.free(k)).count()
    }

    /// Cell of still `i` as whole pixels (x, y, w, h): the `i`-th free grid position. Neighbouring cells share their
    /// edge pixel when the gap is 0. x and y can be below 0: with a negative margin the stills run off the page.
    pub fn cell(&self, i: usize) -> (i64, i64, u32, u32) {
        let k = if self.keep_out.is_some() { (0..).filter(|&k| self.free(k)).nth(i).unwrap_or(i as u32) } else { i as u32 };
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
fn cover(img: &RgbImage, cw: u32, ch: u32) -> RgbImage {
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

/// Fills `r` but leaves out where it meets `hole` (a horizontal edge loses a stretch of its length, a vertical
/// edge a stretch of its height).
fn fill_except(page: &mut RgbImage, r: Rect, hole: Option<Rect>, color: [u8; 3]) {
    let meets = hole.filter(|h| h.x < r.x + r.w && r.x < h.x + h.w && h.y < r.y + r.h && r.y < h.y + h.h);
    match meets {
        None => fill(page, r.x, r.y, r.x + r.w, r.y + r.h, color),
        Some(h) if r.w >= r.h => {
            fill(page, r.x, r.y, h.x.max(r.x), r.y + r.h, color);
            fill(page, (h.x + h.w).min(r.x + r.w), r.y, r.x + r.w, r.y + r.h, color);
        }
        Some(h) => {
            fill(page, r.x, r.y, r.x + r.w, h.y.max(r.y), color);
            fill(page, r.x, (h.y + h.h).min(r.y + r.h), r.x + r.w, r.y + r.h, color);
        }
    }
}

/// The text with its baseline at `baseline`, starting at `x`; fake bold widens the strokes, fake italic slants.
fn draw_text(page: &mut RgbImage, hl: &Headline, x: f64, baseline: f64, scale: PxScale) {
    let font = &*hl.face.font;
    let sf = font.as_scaled(scale);
    let bold_steps = if hl.face.fake_bold { (scale.y / 28.0).ceil().max(1.0) as i32 } else { 0 };
    let slant = if hl.face.fake_italic { 0.2 } else { 0.0 };
    let (w, h) = page.dimensions();
    let mut caret = x as f32;
    let mut prev = None;
    for c in hl.text.chars() {
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
            let py = b.min.y as i32 + gy as i32;
            let shift = (slant * (baseline as f32 - py as f32)).round() as i32;
            for dx in 0..=bold_steps {
                let px = b.min.x as i32 + gx as i32 + shift + dx;
                if px < 0 || py < 0 || px as u32 >= w || py as u32 >= h {
                    continue;
                }
                let p = page.get_pixel_mut(px as u32, py as u32);
                for k in 0..3 {
                    p[k] = (p[k] as f32 * (1.0 - coverage) + hl.color[k] as f32 * coverage).round() as u8;
                }
            }
        });
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
    let mut tiles = tiles.into_iter();
    compose(sheet, n, &grid, |_, _, _| tiles.next().flatten())
}

/// The still `img` filling a cell of `cw` × `ch` (cut to it, centred).
pub fn cover_cell(img: &RgbImage, cw: u32, ch: u32) -> RgbImage {
    cover(img, cw, ch)
}

fn compose(sheet: &Sheet, n: usize, grid: &Grid, mut tile: impl FnMut(usize, u32, u32) -> Option<RgbImage>) -> RgbImage {
    let (w, h) = sheet.size_px();
    let mut page = RgbImage::from_pixel(w, h, Rgb([255, 255, 255]));
    let plan = sheet.plan();
    for i in 0..n {
        let (x, y, cw, ch) = grid.cell(i);
        let tile = tile(i, cw, ch).unwrap_or_else(|| RgbImage::from_pixel(cw, ch, Rgb([200, 200, 200])));
        imageops::overlay(&mut page, &tile, x, y);
    }
    if let (Some((outer, t)), Some(l)) = (plan.line, &sheet.line) {
        // the headline cuts a gap into every strand it covers
        let hole = plan.text.map(|(tr, _, _)| {
            let p = sheet.px(BREAK_PAD);
            Rect { x: tr.x - p, y: tr.y - p, w: tr.w + 2.0 * p, h: tr.h + 2.0 * p }
        });
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
                fill_except(&mut page, edge, hole, l.color);
            }
        }
    }
    if let (Some((tr, baseline, scale)), Some(hl)) = (plan.text, &sheet.headline) {
        draw_text(&mut page, hl, tr.x, baseline, scale);
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
        let hl = |dy: f64| Headline { text: "Rough Cut 3".into(), face: face.clone(), size_pt: 24.0, color: [200, 0, 0], align: Align::Center, valign: VAlign::Top, dx_mm: 0.0, dy_mm: dy, pad_mm: [2.0; 4], wrap: false };
        let line = Line { width_mm: 0.5, inset_mm: 8.0, color: [0, 0, 0], pattern: Pattern::Single };
        // 20 mm down from the middle of the top edge: below the line, the line stays whole
        let s = Sheet { margin_mm: 5.0, line: Some(line.clone()), headline: Some(hl(20.0)), ..a4(None) };
        let p = s.plan();
        let (tr, _, _) = p.text.unwrap();
        assert!((tr.y - 200.0).abs() < 1e-6);
        assert!((tr.x + tr.w / 2.0 - 1050.0).abs() < 1e-6);
        assert!(p.area.y >= tr.y + tr.h, "stills below the headline");
        let page = render(&s, 0, 1.0, |_| None);
        assert_eq!(page.get_pixel(1050, 82), &Rgb([0, 0, 0]), "line whole above the headline");
        // moved onto the line: the line breaks under the text
        let mut s2 = s.clone();
        // centre the text on the line (82.5 px = 8.25 mm; 10 px per mm)
        s2.headline = Some(hl((82.5 - tr.h / 2.0) / 10.0));
        let (tr2, _, _) = s2.plan().text.unwrap();
        assert!(tr2.y < 82.0 && tr2.y + tr2.h > 82.0);
        let page = render(&s2, 0, 1.0, |_| None);
        assert_eq!(page.get_pixel((tr2.x - 10.0) as u32, 82), &Rgb([255, 255, 255]), "gap before the text");
        assert_eq!(page.get_pixel(200, 82), &Rgb([0, 0, 0]), "line goes on beside it");
        // the space below a top headline keeps the stills away
        let mut s3 = s.clone();
        s3.headline = Some(Headline { pad_mm: [0.0, 0.0, 30.0, 0.0], ..hl(20.0) });
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
            dx_mm: 0.0,
            dy_mm: 0.0,
            pad_mm: [3.0; 4],
            wrap: true,
        };
        let s = Sheet { headline: Some(hl.clone()), ..a4(Some(3)) };
        let g = s.grid(12, 16.0 / 9.0);
        // the middle cells are kept free, so the grid has more rows and still holds all 12
        assert!(g.rows > 4 && g.slots() >= 12, "{g:?}");
        let (r, _, _) = s.plan().text.unwrap();
        let o = Rect { x: r.x - 30.0, y: r.y - 30.0, w: r.w + 60.0, h: r.h + 60.0 };
        for i in 0..12 {
            let (x, y, w, h) = g.cell(i);
            let (x, y, w, h) = (x as f64, y as f64, w as f64, h as f64);
            assert!(!(x < o.x + o.w && o.x < x + w && y < o.y + o.h && o.y < y + h), "still {i} under the headline");
        }
        // without flowing the stills lie under it, in the usual 3 × 4
        let plain = Sheet { headline: Some(Headline { wrap: false, ..hl }), ..a4(Some(3)) };
        assert_eq!(plain.grid(12, 16.0 / 9.0).rows, 4);
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
