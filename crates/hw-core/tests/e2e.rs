//! End to end on synthetic data: sheet photo -> hand, paper photo -> paper, doc -> layout -> photo.

mod common;

use std::time::Instant;

use hw_core::ingest::text::from_text;
use hw_core::layout::{self, Geometry, Params};
use hw_core::paper::{PaperGeom, PaperKind, Placement};
use hw_core::render::{self, RenderOpts};
use hw_core::charset::{Lang, Level};
use hw_core::hand::Src;
use hw_core::sheet::{self, PageKind, Recipe};
use hw_core::store::Library;
use hw_core::{export, extract, io, synth};

const DOC: &str = "# Uy vazifasi\n\nBugun biz maktabda yangi mavzuni oʻrgandik. Ustoz bizga **qiziqarli** misollar <u>koʻrsatdi</u>.\n\n| Ism | Baho |\n|---|---|\n| Ali | 5 |\n| Олег | 4 |\n\n---\nМы решили все задачи: $x^2 + 3x = 10$. The weather was nice.\n\n$$\\frac{a+b}{2} = \\sqrt{x^2+1}$$\n\nOxirgi qator.";

#[test]
fn full_pipeline() {
    if common::font("inkfree.ttf").is_none() {
        eprintln!("skipping: Ink Free font not available");
        return;
    }
    let root = common::scratch().join(format!("lib-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let mut hand = lib.create_hand("synthetic").unwrap();

    // a sheet for Uzbek (Latin) + Russian, every page but digits; then the copy-text page
    // of the first fixed sheet (v0.2), which must still import
    let recipe = Recipe { langs: vec![Lang::UzLatn, Lang::Ru], level: Level::Normal };
    let mut pages: Vec<sheet::Page> = sheet::recipe_pages(&recipe).into_iter().filter(|p| p.kind != PageKind::Digits).collect();
    pages.push(sheet::first_sheet().pop().unwrap());
    for (k, page) in pages.iter().enumerate() {
        let flat = common::fill_page(page, 10.0, 3);
        let ph = common::photo(&flat, 10 + k as u64, 3000, 0.0);
        let t = Instant::now();
        let rep = extract::import_sheet(&mut hand, &ph, &|c| lib.stored_page(c)).unwrap();
        eprintln!("page {:?} {:?}: {:?} in {:?}", page.code, page.kind, (rep.ok, rep.page, rep.boxes, rep.lines, &rep.error), t.elapsed());
        assert!(rep.ok, "page {:?} not imported: {:?}", page.code, rep.error);
        assert_eq!(rep.page, Some(page.index + 1));
        let lines = page.items.iter().filter(|i| i.kind == sheet::Kind::Line).count();
        let boxes = page.items.len() - lines;
        assert!(rep.lines as f32 >= 0.8 * lines as f32, "copy lines harvested: {} of {lines}", rep.lines);
        assert!(rep.boxes as f32 >= 0.95 * boxes as f32, "boxes on page {:?}: {} of {boxes}", page.code, rep.boxes);
    }
    hand.meta.langs = recipe.langs.clone();
    lib.save_hand(&hand).unwrap();
    let a_count = hand.glyphs.iter().filter(|g| g.text == "a").count();
    assert!(a_count >= 6, "variants of a: {a_count}");
    eprintln!("hand: {} glyphs, xh {:.2} mm, box_scale {:.2}, ink {:?}, stats {:?}",
        hand.glyphs.len(), hand.meta.xh_mm, hand.meta.box_scale, hand.meta.ink, (hand.meta.stats.gap, hand.meta.stats.word, hand.meta.stats.join));
    assert!((2.0..4.5).contains(&hand.meta.xh_mm));
    eprintln!("lean {:.3}, cap {:.2}", hand.meta.lean, hand.meta.cap);
    assert!((1.15..2.0).contains(&hand.meta.cap));

    // the digits page was never written: the document's digits and symbols are reported as
    // not written, exactly, never made up
    let blocks = from_text(DOC);
    let need = synth::missing(&hand, synth::chars_of(&blocks));
    eprintln!("not written: {}", need.iter().collect::<String>());
    assert!(need.contains(&'5') && need.contains(&'=') && !need.contains(&'a'), "{need:?}");
    assert!(hand.glyphs.iter().all(|g| g.src != Src::Auto), "nothing is made up");
    // a sheet of the missing ones is kept in the library, found again from its markers, and
    // its letters are written ones
    let mut refill = sheet::refill_pages(&[("=".into(), 2), ("(".into(), 2)]);
    lib.store_pages(&mut refill).unwrap();
    assert!(sheet::is_stored(refill[0].code));
    let ph = common::photo(&common::fill_page(&refill[0], 10.0, 4), 77, 3000, 0.0);
    let rep = extract::import_sheet(&mut hand, &ph, &|c| lib.stored_page(c)).unwrap();
    assert!(rep.ok && rep.boxes == 4, "refill page: {:?}", (rep.ok, rep.boxes, &rep.error));
    let need = synth::missing(&hand, synth::chars_of(&blocks));
    assert!(!need.contains(&'=') && !need.contains(&'(') && need.contains(&'5'), "{need:?}");
    lib.save_hand(&hand).unwrap();

    // paper
    let flat = common::lined_paper(10.0, (170.0, 205.0), 8.0, false);
    let ph = common::photo(&flat, 9, 3000, 1.0);
    let paper = lib.add_paper("curled", &io::rgb_jpeg(&ph, 92)).unwrap();
    let s = &paper.shots[0];
    eprintln!("paper: kind {:?} size {} rules {} margin {:?} noise {:.4} blur {:.2}", paper.kind, paper.size, s.rules.len(), s.margin_mm, s.noise, s.blur_px);
    assert_eq!(paper.kind, PaperKind::Lined);
    assert_eq!(paper.size, "Notebook");
    assert_eq!(s.rules.len(), 24);
    assert!((s.margin_mm.unwrap() - 25.0).abs() < 1.0);

    // layout + render
    let place = Placement::default();
    let pg = PaperGeom::new(&paper, &place, 1.0);
    let geo = Geometry { width_mm: pg.width() as f32, widths: vec![], lines_per_page: pg.lines_per_page(), pitch_mm: pg.pitch as f32, ruled: true };
    let t = Instant::now();
    let res = layout::layout(&blocks, &hand, &Params::default(), geo.clone());
    eprintln!("layout: {} pages, {} elems, not written {:?} in {:?}", res.pages.len(), res.pages[0].len(), res.missing, t.elapsed());
    assert_eq!(res.pages.len(), 1);
    // what the layout could not write is exactly what was never written
    assert!(res.missing.contains(&'5'), "{:?}", res.missing);
    assert!(res.missing.iter().all(|c| need.contains(c)), "{:?} vs {:?}", res.missing, need);
    let photo = lib.shot_photo(&paper, 0).unwrap();
    let t = Instant::now();
    let out = render::render(&paper, &photo, &hand, &res.pages[0], &place, &RenderOpts::default());
    eprintln!("render full {}x{} in {:?}", out.w, out.h, t.elapsed());
    std::fs::write(common::scratch().join("render_full.jpg"), io::rgb_jpeg(&out, 92)).unwrap();
    let small = photo.resize(1100, photo.h * 1100 / photo.w);
    let t = Instant::now();
    let prev = render::render(&paper, &small, &hand, &res.pages[0], &place, &RenderOpts { scale: 1100.0 / photo.w as f32, ..Default::default() });
    eprintln!("render preview in {:?}", t.elapsed());
    std::fs::write(common::scratch().join("render_preview.jpg"), io::rgb_jpeg(&prev, 90)).unwrap();
    let t = Instant::now();
    let rnd = render::render(&paper, &photo, &hand, &res.pages[0], &place, &RenderOpts { random: true, seed: 4, ..Default::default() });
    eprintln!("render random in {:?}", t.elapsed());
    std::fs::write(common::scratch().join("render_random.jpg"), io::rgb_jpeg(&rnd, 88)).unwrap();

    // realism options: pencil, crossed-out mistakes, ink running low
    let p2 = Params { mistakes: 25.0, speed: 0.7, ..Params::default() };
    let res2 = layout::layout(&blocks, &hand, &p2, geo);
    let pen = hw_core::pen::Pen { kind: hw_core::pen::PenKind::Pencil, color: Some([0.40, 0.40, 0.42]), density: 1.0, ink_low: 0.0 };
    let pencil = render::render(&paper, &photo, &hand, &res2.pages[0], &place, &RenderOpts { pen, ..Default::default() });
    std::fs::write(common::scratch().join("render_pencil.jpg"), io::rgb_jpeg(&pencil, 90)).unwrap();
    let strikes = res2.pages[0].iter().filter(|e| matches!(e, hw_core::layout::Elem::S { .. })).count();
    let base = res.pages[0].iter().filter(|e| matches!(e, hw_core::layout::Elem::S { .. })).count();
    assert!(strikes > base, "mistakes add strike-through strokes ({strikes} vs {base})");

    // exports
    let svg = export::svg::page_svg(&hand, &res.pages[0], &Geometry::default(), hand.meta.ink, true);
    assert!(svg.starts_with("<svg") && svg.contains("<use") && svg.contains("id=\"plot\""));
    let paths = export::svg::plot_paths(&hand, &res.pages[0], &Geometry::default());
    let g = export::gcode::to_gcode(&paths, &Default::default());
    assert!(g.lines().count() > 200);
    let pdf = export::pdf::photos_to_pdf(&[io::rgb_jpeg(&prev, 85)]).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let _ = std::fs::remove_dir_all(&root);
}
