//! Saving the sheet: PNG, TIFF and JPEG carry the resolution; the PDF page has the paper size.

use image::RgbImage;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Png,
    Tiff,
    Jpeg,
    Pdf,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Png, Format::Tiff, Format::Jpeg, Format::Pdf];
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Tiff => "tif",
            Format::Jpeg => "jpg",
            Format::Pdf => "pdf",
        }
    }
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(Format::Png),
            "tif" | "tiff" => Some(Format::Tiff),
            "jpg" | "jpeg" => Some(Format::Jpeg),
            "pdf" => Some(Format::Pdf),
            _ => None,
        }
    }
}

/// Writes to a temp file next to `path`, then renames: a failed save never leaves half a file.
pub fn save(img: &RgbImage, path: &Path, dpi: f64, format: Format) -> Result<(), String> {
    let tmp = path.with_extension(format!("{}.part", format.ext()));
    let res = write(img, &tmp, dpi, format).map_err(|e| e.to_string());
    match res {
        Ok(()) => std::fs::rename(&tmp, path).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write(img: &RgbImage, path: &Path, dpi: f64, format: Format) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = BufWriter::new(File::create(path)?);
    match format {
        Format::Png => {
            let mut e = png::Encoder::new(&mut out, img.width(), img.height());
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            let ppm = (dpi / 0.0254).round() as u32;
            e.set_pixel_dims(Some(png::PixelDimensions { xppu: ppm, yppu: ppm, unit: png::Unit::Meter }));
            e.write_header()?.write_image_data(img.as_raw())?;
        }
        Format::Tiff => {
            use tiff::encoder::{Compression, DeflateLevel, Rational, TiffEncoder, colortype::RGB8};
            let mut enc = TiffEncoder::new(&mut out)?.with_compression(Compression::Deflate(DeflateLevel::Balanced));
            let mut im = enc.new_image::<RGB8>(img.width(), img.height())?;
            let r = Rational { n: dpi.round() as u32, d: 1 };
            im.resolution(tiff::tags::ResolutionUnit::Inch, r);
            im.write_data(img.as_raw())?;
        }
        Format::Jpeg => {
            let mut e = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
            e.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(dpi.round() as u16));
            e.encode_image(img)?;
        }
        Format::Pdf => out.write_all(&pdf(img, dpi))?,
    }
    out.flush()?;
    Ok(())
}

/// A one-page PDF: the sheet as a lossless image filling the page (paper size from pixels and dpi).
fn pdf(img: &RgbImage, dpi: f64) -> Vec<u8> {
    use flate2::{Compression, write::ZlibEncoder};
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(img.as_raw()).expect("writing to memory");
    let data = z.finish().expect("writing to memory");
    let (w, h) = (img.width() as f64 * 72.0 / dpi, img.height() as f64 * 72.0 / dpi);
    let content = format!("q {w:.3} 0 0 {h:.3} 0 0 cm /Im0 Do Q");
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w:.3} {h:.3}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>").into_bytes(),
        [
            format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", img.width(), img.height(), data.len()).into_bytes(),
            data,
            b"\nendstream".to_vec(),
        ]
        .concat(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
    ];
    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend(o);
        pdf.extend(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        pdf.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn every_format_round_trips_with_resolution() {
        let dir = std::env::temp_dir().join(format!("cinemosaic-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let img = RgbImage::from_fn(30, 20, |x, y| Rgb([x as u8 * 8, y as u8 * 12, 77]));
        for f in Format::ALL {
            let p = dir.join(format!("sheet.{}", f.ext()));
            save(&img, &p, 300.0, f).unwrap();
            assert_eq!(Format::from_path(&p), Some(f));
            if f == Format::Pdf {
                let b = std::fs::read(&p).unwrap();
                assert!(b.starts_with(b"%PDF-1.4") && b.ends_with(b"%%EOF\n"));
                assert!(String::from_utf8_lossy(&b).contains("/MediaBox [0 0 7.200 4.800]")); // 30 px at 300 dpi = 0.1 in
                continue;
            }
            let back = image::open(&p).unwrap().to_rgb8();
            assert_eq!(back.dimensions(), (30, 20));
            if f != Format::Jpeg {
                assert_eq!(back, img);
            }
        }
        // the dpi really is in the files
        let png = std::fs::read(dir.join("sheet.png")).unwrap();
        let phys = png.windows(4).position(|w| w == b"pHYs").unwrap();
        assert_eq!(u32::from_be_bytes(png[phys + 4..phys + 8].try_into().unwrap()), 11811); // 300 dpi in px per metre
        let jpg = std::fs::read(dir.join("sheet.jpg")).unwrap();
        assert_eq!(&jpg[6..11], b"JFIF\0");
        assert_eq!((jpg[13], u16::from_be_bytes([jpg[14], jpg[15]])), (1, 300)); // unit dpi, x density
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
