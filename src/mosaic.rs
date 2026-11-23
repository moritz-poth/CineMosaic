//! Photomosaic: the stills rebuild a main picture. The picture is cut into tiles; every tile gets the still whose
//! colours match it best – compared as a 3 × 3 pattern in Lab (the colour space close to how the eye sees), so edges
//! and gradients match too, not only the average colour. A still may come back, but not within `repeat` tiles of
//! itself, and every use makes it a little dearer, so the whole set gets used.

use image::{RgbImage, imageops};

/// Colour pattern of a picture: the Lab colour of each of its 3 × 3 parts, row by row.
pub type Feat = [[f32; 3]; 9];

/// sRGB (0–255) → CIE Lab (D65).
fn lab(p: [u8; 3]) -> [f32; 3] {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(p[0]), lin(p[1]), lin(p[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// The colour pattern of a whole picture (a still, already cropped to the tile's format).
pub fn features(img: &RgbImage) -> Feat {
    let small = imageops::resize(img, 3, 3, imageops::FilterType::Triangle);
    let mut f = [[0.0; 3]; 9];
    for (k, p) in small.pixels().enumerate() {
        f[k] = lab(p.0);
    }
    f
}

/// The colour pattern of every tile of `target` cut into `cols` × `rows` tiles, row by row.
pub fn target_features(target: &RgbImage, cols: u32, rows: u32) -> Vec<Feat> {
    let small = imageops::resize(target, cols * 3, rows * 3, imageops::FilterType::Triangle);
    (0..rows)
        .flat_map(|r| (0..cols).map(move |c| (c, r)))
        .map(|(c, r)| {
            let mut f = [[0.0; 3]; 9];
            for k in 0..9u32 {
                f[k as usize] = lab(small.get_pixel(c * 3 + k % 3, r * 3 + k / 3).0);
            }
            f
        })
        .collect()
}

fn distance(a: &Feat, b: &Feat) -> f32 {
    a.iter().zip(b).map(|(p, q)| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sum()
}

/// For every tile (row by row, `cols` wide) the index of the still that fits it best. Stills without a pattern
/// (not grabbed yet) are skipped; a still is not used again within `repeat` tiles (columns and rows) of itself
/// while another one is free there. None when there are no stills.
pub fn assign(tiles: &[Feat], cols: u32, stills: &[Option<Feat>], repeat: u32) -> Option<Vec<usize>> {
    let have: Vec<(usize, &Feat)> = stills.iter().enumerate().filter_map(|(i, f)| Some((i, f.as_ref()?))).collect();
    if have.is_empty() {
        return None;
    }
    let cols = cols.max(1) as usize;
    let r = repeat as usize;
    let mut uses = vec![0u32; stills.len()];
    let mut out: Vec<usize> = Vec::with_capacity(tiles.len());
    for (t, want) in tiles.iter().enumerate() {
        let (c, row) = (t % cols, t / cols);
        // the stills already placed within `repeat` tiles (only rows above and the tiles before in this row are set)
        let mut near: Vec<usize> = vec![];
        for y in row.saturating_sub(r)..=row {
            let x_to = if y == row { c } else { (c + r + 1).min(cols) };
            near.extend((c.saturating_sub(r)..x_to).filter_map(|x| out.get(y * cols + x).copied()));
        }
        let best = |skip_near: bool| {
            have.iter()
                .filter(|(i, _)| !skip_near || !near.contains(i))
                .map(|&(i, f)| (i, distance(want, f) * (1.0 + 0.04 * uses[i] as f32)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        let i = best(true).or_else(|| best(false)).expect("stills");
        uses[i] += 1;
        out.push(i);
    }
    Some(out)
}

/// Mean colour of a picture, per channel.
pub fn mean(img: &RgbImage) -> [f32; 3] {
    let n = (img.width() * img.height()).max(1) as f32;
    let mut s = [0.0f32; 3];
    for p in img.pixels() {
        for k in 0..3 {
            s[k] += p[k] as f32;
        }
    }
    s.map(|v| v / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn tiles_get_the_still_of_their_colour_and_repeats_keep_apart() {
        let solid = |c: [u8; 3]| features(&RgbImage::from_pixel(16, 9, Rgb(c)));
        let (red, blue, dark_red) = (solid([220, 20, 20]), solid([20, 20, 220]), solid([150, 10, 10]));
        // a target: left half red, right half blue
        let mut target = RgbImage::from_pixel(40, 10, Rgb([220, 20, 20]));
        for y in 0..10 {
            for x in 20..40 {
                target.put_pixel(x, y, Rgb([20, 20, 220]));
            }
        }
        let tiles = target_features(&target, 4, 1);
        let got = assign(&tiles, 4, &[Some(blue), Some(red), None, Some(dark_red)], 0).unwrap();
        assert_eq!(got, vec![1, 1, 0, 0]);
        // with a distance of 1, the red still is not used twice side by side: the dark red takes the second place
        let got = assign(&tiles, 4, &[Some(blue), Some(red), None, Some(dark_red)], 1).unwrap();
        assert_eq!(got[..2], [1, 3]);
        assert!(assign(&tiles, 4, &[None], 1).is_none());
    }

    #[test]
    fn lab_of_white_and_black() {
        let w = lab([255, 255, 255]);
        assert!((w[0] - 100.0).abs() < 0.1 && w[1].abs() < 0.5 && w[2].abs() < 0.5);
        assert!(lab([0, 0, 0])[0].abs() < 0.01);
    }
}
