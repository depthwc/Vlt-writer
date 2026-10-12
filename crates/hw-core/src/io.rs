//! Image decode/encode helpers.

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageReader};

use crate::img::Rgb;

/// A text file as people save them on Windows: UTF-8 (with or without a BOM), UTF-16
/// (Notepad's "Unicode") or Windows-1251 (older Russian files).
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(b) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(b).into_owned();
    }
    let utf16 = |b: &[u8], le: bool| -> String {
        let units: Vec<u16> = b.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(b) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(b, true);
    }
    if let Some(b) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(b, false);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    // Windows-1251: the upper half is Cyrillic and typographic marks
    const HIGH: &str = "ЂЃ‚ѓ„…†‡€‰Љ‹ЊЌЋЏђ‘’“”•–—\u{98}™љ›њќћџ\u{a0}ЎўЈ¤Ґ¦§Ё©Є«¬\u{ad}®Ї°±Ііґµ¶·ё№є»јЅѕї";
    let high: Vec<char> = HIGH.chars().collect();
    bytes
        .iter()
        .map(|&b| match b {
            0..=0x7F => b as char,
            0x80..=0xBF => high.get((b - 0x80) as usize).copied().unwrap_or('\u{fffd}'),
            _ => char::from_u32(0x410 + (b - 0xC0) as u32).unwrap_or('\u{fffd}'),
        })
        .collect()
}

/// Decode a photo honouring EXIF rotation, longest side capped at `max_side`.
pub fn decode_photo(bytes: &[u8], max_side: u32) -> anyhow::Result<image::RgbImage> {
    let mut dec = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.into_decoder()?;
    let orient = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(dec)?;
    img.apply_orientation(orient);
    if img.width().max(img.height()) > max_side {
        img = img.resize(max_side, max_side, image::imageops::FilterType::Lanczos3);
    }
    Ok(img.to_rgb8())
}

pub fn jpeg(img: &image::RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    let _ = enc.encode_image(img);
    out
}

pub fn png(img: &DynamicImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let _ = img.write_to(&mut out, image::ImageFormat::Png);
    out.into_inner()
}

pub fn rgb_jpeg(img: &Rgb, quality: u8) -> Vec<u8> {
    jpeg(&img.to_image(), quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_files_in_any_windows_encoding() {
        let t = "Salom, oʻrtoq! Привет, ёж.";
        assert_eq!(decode_text(t.as_bytes()), t);
        assert_eq!(decode_text(&[&[0xEF, 0xBB, 0xBF][..], t.as_bytes()].concat()), t);
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(t.encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
        assert_eq!(decode_text(&le), t);
        // "Привет, ёж" in Windows-1251
        assert_eq!(decode_text(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2, 0x2C, 0x20, 0xB8, 0xE6]), "Привет, ёж");
    }
}
