//! Multi-page PDF: one rendered photo per page, sized to fill an A4 page.

use std::sync::Arc;

use krilla::geom::{Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use krilla::Document;

/// `jpegs`: encoded JPEG pages (in order). Pages are A4-sized, keeping each photo's aspect.
pub fn photos_to_pdf(jpegs: &[Vec<u8>]) -> anyhow::Result<Vec<u8>> {
    let mut doc = Document::new();
    let mm = 72.0 / 25.4;
    for data in jpegs {
        let img = Image::from_jpeg(Arc::new(data.clone()).into(), true).map_err(|e| anyhow::anyhow!("jpeg: {e}"))?;
        let (iw, ih) = img.size();
        let (pw, ph) = if iw > ih { (297.0 * mm, 210.0 * mm) } else { (210.0 * mm, 297.0 * mm) };
        let s = (pw / iw as f32).min(ph / ih as f32);
        let (w, h) = (iw as f32 * s, ih as f32 * s);
        let mut page = doc.start_page_with(PageSettings::from_wh(pw, ph).unwrap());
        let mut surf = page.surface();
        surf.push_transform(&Transform::from_translate((pw - w) / 2.0, (ph - h) / 2.0));
        surf.draw_image(img, Size::from_wh(w, h).unwrap());
        surf.pop();
        surf.finish();
        page.finish();
    }
    doc.finish().map_err(|e| anyhow::anyhow!("pdf: {e:?}"))
}
