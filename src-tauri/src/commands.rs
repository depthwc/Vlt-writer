//! Commands the UI calls. Heavy work runs on worker threads (jobs or spawn_blocking).

use std::path::PathBuf;

use hw_core::camera::ShotStyle;
use hw_core::charset::{self, Lang};
use hw_core::export::{gcode, pdf as pdfx, svg};
use hw_core::hand::{CovGroup, Edit, Flag, Glyph, Hand, HandMeta, How, Src};
use hw_core::ingest::{self, Block};
use hw_core::layout::{self, Geometry, LayoutResult};
use hw_core::paper::{self, PaperGeom, PaperMeta, Placement};
use hw_core::project::Project;
use hw_core::render::{self, RenderOpts};
use hw_core::sheet::{Page, Recipe, SheetText};
use hw_core::{extract, geom, guide, io, sheet, synth};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::state::{err, AppState, Res};

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}

// =========================================================================== hands
#[derive(Serialize)]
pub struct GlyphView {
    id: String,
    text: String,
    src: Src,
    w: u32,
    h: u32,
    bl: f32,
    on: bool,
    flags: Vec<Flag>,
    edits: usize,
    how: Option<How>,
    from: String,
    /// a print letter
    print: bool,
}

#[derive(Serialize)]
pub struct HandView {
    meta: HandMeta,
    glyphs: Vec<GlyphView>,
    coverage: Vec<CovGroup>,
    /// characters not written yet (letters joined and print apart)
    refill: usize,
}

fn hand_view(h: &Hand) -> HandView {
    HandView {
        meta: h.meta.clone(),
        glyphs: h
            .glyphs
            .iter()
            .map(|g| GlyphView {
                id: g.id.clone(),
                text: g.text.clone(),
                src: g.src,
                w: g.w,
                h: g.h,
                bl: g.bl,
                on: g.on,
                flags: g.flags.clone(),
                edits: g.edits.len(),
                how: g.how,
                from: g.from.clone(),
                print: g.print,
            })
            .collect(),
        coverage: h.coverage(),
        refill: h.not_written(),
    }
}

/// After any change to a hand: drop stale made-up letters, make the missing ones, save.
fn tidy_save(s: &AppState, h: &mut Hand) -> Res<()> {
    synth::drop_made(h);
    s.save_hand(h)
}

#[tauri::command]
pub fn list_hands(s: State<AppState>) -> Res<Vec<hw_core::store::Summary>> {
    s.lib.hands().map_err(err)
}

#[tauri::command]
pub fn create_hand(s: State<AppState>, name: String) -> Res<String> {
    let h = s.lib.create_hand(name.trim()).map_err(err)?;
    Ok(h.meta.id)
}

#[tauri::command]
pub fn rename_hand(s: State<AppState>, id: String, name: String) -> Res<()> {
    let h = s.hand(&id)?;
    let mut h = h.write();
    h.meta.name = name;
    s.save_hand(&h)
}

#[tauri::command]
pub fn delete_hand(s: State<AppState>, id: String) -> Res<()> {
    s.forget_hand(&id);
    s.lib.delete_hand(&id).map_err(err)
}

#[tauri::command]
pub fn get_hand(s: State<AppState>, id: String) -> Res<HandView> {
    let h = s.hand(&id)?;
    let h = h.read();
    Ok(hand_view(&h))
}

/// Pages a sheet would have: for a recipe (languages + level), or the missing letters of a hand.
#[tauri::command]
pub fn sheet_plan(s: State<AppState>, recipe: Option<Recipe>, hand: Option<String>) -> Res<Vec<Page>> {
    match (recipe, hand) {
        (Some(r), _) => Ok(sheet::recipe_pages(&r)),
        (None, Some(h)) => Ok(sheet::refill_pages(&s.hand(&h)?.read().refill_list())),
        _ => Err("nothing to print".into()),
    }
}

#[tauri::command]
pub async fn save_sheet_pdf(s: State<'_, AppState>, path: String, recipe: Option<Recipe>, hand: Option<String>, text: SheetText) -> Res<usize> {
    let mut pages = sheet_plan(s.clone(), recipe.clone(), hand.clone())?;
    if pages.is_empty() {
        return Err("nothing to print".into());
    }
    match (&recipe, &hand) {
        // pages of missing letters cannot be rebuilt from their markers: keep them
        (None, _) => s.lib.store_pages(&mut pages).map_err(err)?,
        // remember what this person writes in
        (Some(r), Some(h)) => {
            let h = s.hand(h)?;
            let mut hw = h.write();
            hw.meta.langs = r.langs.clone();
            s.save_hand(&hw)?;
        }
        _ => {}
    }
    blocking(move || {
        let fonts = std::env::var("WINDIR").map(|w| PathBuf::from(w).join("Fonts")).unwrap_or_else(|_| PathBuf::from(r"C:\Windows\Fonts"));
        let font = std::fs::read(fonts.join("arial.ttf")).or_else(|_| std::fs::read(fonts.join("segoeui.ttf"))).map_err(err)?;
        let pdf = sheet::render_pdf(&pages, font, &text).map_err(err)?;
        std::fs::write(&path, pdf).map_err(err)?;
        Ok(pages.len())
    })
    .await
}

#[tauri::command]
pub fn set_hand_langs(s: State<AppState>, hand: String, langs: Vec<Lang>) -> Res<HandView> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    hw.meta.langs = langs;
    tidy_save(&s, &mut hw)?;
    Ok(hand_view(&hw))
}

#[tauri::command]
pub fn import_sheet_photos(app: AppHandle, s: State<AppState>, hand: String, paths: Vec<String>) -> Res<u64> {
    let h = s.hand(&hand)?;
    let n = paths.len();
    let id = s.jobs.spawn(&app, "sheet", n, move |ctx| {
        let st = ctx.app.state::<AppState>();
        let lib = &st.lib;
        let mut reports = vec![];
        for (i, p) in paths.iter().enumerate() {
            if ctx.cancelled() {
                break;
            }
            let name = PathBuf::from(p).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            ctx.progress(i, &name);
            let rep = (|| -> anyhow::Result<extract::Report> {
                let img = io::decode_photo(&std::fs::read(p)?, 5000)?;
                let mut hw = h.write();
                let r = extract::import_sheet(&mut hw, &hw_core::img::Rgb::from_image(&img), &|c| lib.stored_page(c))?;
                lib.save_hand(&hw)?;
                Ok(r)
            })();
            reports.push(match rep {
                Ok(r) => serde_json::json!({"file": name, "report": r}),
                Err(e) => serde_json::json!({"file": name, "report": {"ok": false, "error": e.to_string()}}),
            });
        }
        let mut hw = h.write();
        tidy_save(&st, &mut hw)?;
        Ok(serde_json::Value::Array(reports))
    });
    Ok(id)
}

#[derive(Serialize)]
pub struct GuideDone {
    letters: usize,
    /// words of the page that could not be found on the photo
    unmatched: Vec<String>,
    /// the photo was too small for sharp letters
    small: bool,
}

/// An import's photo was too small for sharp letters.
fn small(e: &serde_json::Value) -> bool {
    e["px_mm"].as_f64().is_some_and(|v| v > 0.0 && (v as f32) < extract::SHARP_PX_MM)
}

#[derive(Serialize)]
pub struct GuidePage {
    #[serde(flatten)]
    page: guide::Page,
    done: Option<GuideDone>,
}

/// One built-in language's pages.
#[derive(Serialize)]
pub struct GuideGroup {
    lang: Lang,
    pages: Vec<GuidePage>,
}

/// A page the person wrote on their own, imported with its text.
#[derive(Serialize)]
pub struct OwnPage {
    import: u64,
    letters: usize,
    small: bool,
    /// its photo's file name, or its first line
    name: String,
}

#[derive(Serialize)]
pub struct GuideView {
    main: Lang,
    /// every built-in page, by language (the main one first): all are needed to start
    required: Vec<GuideGroup>,
    /// pages of what did not come through, once the required pages are written
    missing: Vec<GuidePage>,
    /// pages of only the characters not written yet: the ones done, then the ones to write
    fill: Vec<GuidePage>,
    /// the person's own writing: photos with their text
    own: Vec<OwnPage>,
}

/// How many pages of missing letters and joins are offered, at most.
const MISSING_PAGES: usize = 3;

/// The samples to write: every built-in page, then pages of what did not come through, and
/// the person's own writing.
#[tauri::command]
pub fn guide_pages(s: State<AppState>, hand: String) -> Res<GuideView> {
    let h = s.hand(&hand)?;
    let h = h.read();
    let done = |id: &str| -> Option<(GuideDone, Vec<String>)> {
        let e = h.meta.imports.iter().rev().find(|e| e["kind"] == "guide" && e["page"] == id)?;
        let unmatched = e["unmatched"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let lines = e["lines"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        Some((GuideDone { letters: e["glyphs"].as_u64().unwrap_or(0) as usize, unmatched, small: small(e) }, lines))
    };
    // the lines of a page written in print, as they were when its photo was read
    let print_of = |id: &str| -> Option<Vec<bool>> {
        let e = h.meta.imports.iter().rev().find(|e| e["kind"] == "guide" && e["page"] == id)?;
        e["print"].as_array().map(|v| v.iter().map(|x| x.as_bool().unwrap_or(false)).collect())
    };
    let with_done = |mut p: guide::Page| -> GuidePage {
        match done(&p.id) {
            Some((d, lines)) => {
                // the text the photo was read with (pages change between versions)
                if !lines.is_empty() {
                    p.print = print_of(&p.id).filter(|v| v.len() == lines.len()).unwrap_or_else(|| vec![p.print.first().copied().unwrap_or(false); lines.len()]);
                    p.lines = lines;
                }
                GuidePage { page: p, done: Some(d) }
            }
            None => GuidePage { page: p, done: None },
        }
    };
    let main = h.meta.langs.first().copied().filter(|l| *l != Lang::Math).unwrap_or(Lang::UzLatn);
    let required: Vec<GuideGroup> = guide::required(main).into_iter().map(|(lang, ps)| GuideGroup { lang, pages: ps.into_iter().map(with_done).collect() }).collect();
    let mut missing = vec![];
    let mut n = 0;
    while let Some((d, lines)) = done(&format!("extra-{}", n + 1)) {
        n += 1;
        missing.push(GuidePage { page: guide::Page { id: format!("extra-{n}"), lang: None, kind: guide::Kind::Extra, n, print: vec![false; lines.len()], lines }, done: Some(d) });
    }
    if n < MISSING_PAGES && required.iter().flat_map(|g| g.pages.iter()).all(|p| p.done.is_some()) {
        if let Some(p) = guide::missing_page(&h, n + 1) {
            missing.push(GuidePage { page: p, done: None });
        }
    }
    // only what is not written yet: the pages written, then the ones still to write
    let mut fill = vec![];
    let mut k = 0;
    while let Some((d, lines)) = done(&format!("fill-{}", k + 1)) {
        k += 1;
        let id = format!("fill-{k}");
        let print = print_of(&id).filter(|v| v.len() == lines.len()).unwrap_or_else(|| vec![false; lines.len()]);
        fill.push(GuidePage { page: guide::Page { id, lang: None, kind: guide::Kind::Fill, n: k, lines, print }, done: Some(d) });
    }
    fill.extend(guide::fill_pages(&h, k + 1).into_iter().map(|p| GuidePage { page: p, done: None }));
    // a photo by its file name, with its folder when two have the same name
    let tail = |f: &str, n: usize| -> String {
        let parts: Vec<String> = std::path::Path::new(f).components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        parts[parts.len().saturating_sub(n)..].join("/")
    };
    let photos: Vec<&serde_json::Value> = h.meta.imports.iter().filter(|e| e["kind"] == "photo").collect();
    let files: Vec<&str> = photos.iter().filter_map(|e| e["file"].as_str()).collect();
    let own: Vec<OwnPage> = photos
        .iter()
        .map(|e| OwnPage {
            import: e["import"].as_u64().unwrap_or(0),
            letters: e["glyphs"].as_u64().unwrap_or(0) as usize,
            small: small(e),
            name: match e["file"].as_str() {
                Some(f) => {
                    let same = files.iter().filter(|g| tail(g, 1) == tail(f, 1)).count() > 1;
                    if same { format!("{} · {}", tail(f, 1), tail(f, 2).split('/').next().unwrap_or("")) } else { tail(f, 1) }
                }
                None => e["first"].as_str().unwrap_or("").to_string(),
            },
        })
        .collect();
    Ok(GuideView { main, required, missing, fill, own })
}

/// Letters from these imports go, and the imports with them.
fn drop_imports(hw: &mut Hand, keys: &[u64]) {
    if keys.is_empty() {
        return;
    }
    let old: Vec<String> = hw.glyphs.iter().filter(|g| g.ctx.as_ref().is_some_and(|c| keys.contains(&(c.word >> 32)))).map(|g| g.id.clone()).collect();
    for id in old {
        hw.remove_glyph(&id);
    }
    hw.meta.imports.retain(|e| !e["import"].as_u64().is_some_and(|k| keys.contains(&k)));
}

/// An imported page goes, with its letters.
#[tauri::command]
pub fn remove_import(s: State<AppState>, hand: String, import: u64) -> Res<HandView> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    drop_imports(&mut hw, &[import]);
    tidy_save(&s, &mut hw)?;
    Ok(hand_view(&hw))
}

/// The photo of one written page: its letters are found with the page's known text. Taking a
/// page again replaces the letters of the earlier photo.
#[tauri::command]
pub fn import_guide_page(app: AppHandle, s: State<AppState>, hand: String, page: String, lines: Vec<String>, print: Vec<bool>, paths: Vec<String>) -> Res<u64> {
    let h = s.hand(&hand)?;
    Ok(s.jobs.spawn(&app, "photo", 1, move |ctx| {
        let st = ctx.app.state::<AppState>();
        // a page that did not fit on one sheet: its photos in order
        let imgs = photos_of(&paths).map_err(err)?;
        let mut hw = h.write();
        // the earlier photo of this page, if any: its letters go
        let before: Vec<u64> = hw.meta.imports.iter().filter(|e| e["kind"] == "guide" && e["page"] == page.as_str()).filter_map(|e| e["import"].as_u64()).collect();
        drop_imports(&mut hw, &before);
        hw.meta.imports.retain(|e| !(e["kind"] == "guide" && e["page"] == page.as_str()));
        // the lines with text, each with its style
        let print: Vec<bool> = lines.iter().zip(print.iter().chain(std::iter::repeat(&false))).filter(|(l, _)| !l.trim().is_empty()).map(|(_, p)| *p).collect();
        let r = extract::import_styled(&mut hw, &imgs.iter().collect::<Vec<_>>(), &lines.join("\n"), "A4", &print).map_err(err)?;
        let unmatched: Vec<String> = r
            .warnings
            .iter()
            .filter_map(|w| w.strip_prefix("could not align: ").or_else(|| w.strip_prefix("line not found: ")).map(String::from))
            .collect();
        if r.ok {
            if let Some(e) = hw.meta.imports.last_mut() {
                e["kind"] = "guide".into();
                e["page"] = page.clone().into();
                e["lines"] = serde_json::json!(lines);
                e["print"] = serde_json::json!(print);
                e["unmatched"] = serde_json::json!(unmatched);
            }
        }
        tidy_save(&st, &mut hw)?;
        serde_json::to_value(r).map_err(err)
    }))
}

/// A photo the person picked, small enough to show next to the text they type (data URL).
#[tauri::command]
pub async fn photo_preview(path: String) -> Res<String> {
    blocking(move || {
        let img = io::decode_photo(&std::fs::read(&path).map_err(err)?, 1800).map_err(err)?;
        use base64::Engine;
        Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(io::jpeg(&img, 85))))
    })
    .await
}

/// One page of the person's own writing: a photo and the text on it (`print`: written in
/// print letters). A photo imported before is replaced, so a page can be imported again after
/// its text is fixed.
fn import_own(hw: &mut Hand, paths: &[String], transcript: &str, size: &str, print: bool) -> anyhow::Result<extract::Report> {
    let imgs = photos_of(paths)?;
    let first = paths.first().map(String::as_str).unwrap_or("");
    let before: Vec<u64> = hw.meta.imports.iter().filter(|e| e["kind"] == "photo" && e["file"] == first).filter_map(|e| e["import"].as_u64()).collect();
    drop_imports(hw, &before);
    let r = extract::import_styled(hw, &imgs.iter().collect::<Vec<_>>(), transcript, size, &vec![print; transcript.lines().count()])?;
    if r.ok {
        if let Some(e) = hw.meta.imports.last_mut() {
            if e["kind"] == "photo" {
                e["first"] = transcript.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().into();
                e["file"] = first.into();
                e["photos"] = paths.len().into();
            }
        }
    }
    Ok(r)
}

/// The photos of one page, in order.
fn photos_of(paths: &[String]) -> anyhow::Result<Vec<hw_core::img::Rgb>> {
    anyhow::ensure!(!paths.is_empty(), "no photo");
    paths.iter().map(|p| Ok(hw_core::img::Rgb::from_image(&io::decode_photo(&std::fs::read(p)?, 5000)?))).collect()
}

#[tauri::command]
pub fn import_handwriting_photo(app: AppHandle, s: State<AppState>, hand: String, paths: Vec<String>, transcript: String, size: String, print: Option<bool>) -> Res<u64> {
    let h = s.hand(&hand)?;
    Ok(s.jobs.spawn(&app, "photo", 1, move |ctx| {
        let st = ctx.app.state::<AppState>();
        let mut hw = h.write();
        let r = import_own(&mut hw, &paths, &transcript, &size, print.unwrap_or(false)).map_err(err)?;
        tidy_save(&st, &mut hw)?;
        serde_json::to_value(r).map_err(err)
    }))
}

/// A text file, whatever its encoding.
#[tauri::command]
pub fn read_text_file(path: String) -> Res<String> {
    Ok(io::decode_text(&std::fs::read(&path).map_err(err)?))
}

/// The text of a photo: the .txt of the same name next to it, if there is one.
#[tauri::command]
pub fn photo_text(path: String) -> Res<Option<String>> {
    let t = std::path::Path::new(&path).with_extension("txt");
    Ok(std::fs::read(&t).ok().map(|b| io::decode_text(&b)))
}

#[derive(Serialize)]
pub struct FolderPage {
    name: String,
    /// the page's photos in order (3.jpg, or 3_1.jpg and 3_2.jpg)
    images: Vec<String>,
    text: String,
    lines: usize,
    /// imported before (importing replaces it)
    again: bool,
}

#[derive(Serialize)]
pub struct FolderScan {
    pages: Vec<FolderPage>,
    /// photos with no .txt of their name, and texts with no photo
    no_text: Vec<String>,
    no_photo: Vec<String>,
}

const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "heic", "bmp", "tif", "tiff"];

/// The name of a page's text a photo belongs to, and its number among the page's photos.
fn page_of(stem: &str) -> (String, Option<u32>) {
    let s = stem.trim_end_matches(')');
    let digits: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    let head = &s[..s.len() - digits.len()];
    match head.strip_suffix(['_', '-', '.', ' ', '(']).map(|h| h.trim_end_matches(" (").trim_end()) {
        Some(h) if !digits.is_empty() && !h.is_empty() => (h.to_lowercase(), digits.parse().ok()),
        _ => (stem.to_lowercase(), None),
    }
}

/// Pages in a folder and the folders inside it: a photo goes with the .txt of the same name
/// in the same folder (3.jpg with 3.txt), and a page over several photos is numbered (3_1.jpg
/// and 3_2.jpg, or 3-1, "3 (1)").
#[tauri::command]
pub async fn scan_folder(s: State<'_, AppState>, hand: String, dir: String) -> Res<FolderScan> {
    let done: Vec<String> = {
        let h = s.hand(&hand)?;
        let h = h.read();
        h.meta.imports.iter().filter(|e| e["kind"] == "photo").filter_map(|e| e["file"].as_str().map(String::from)).collect()
    };
    blocking(move || {
        fn walk(dir: &std::path::Path, depth: usize, out: &mut Vec<PathBuf>) {
            let Ok(rd) = std::fs::read_dir(dir) else { return };
            let mut es: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            es.sort();
            for p in es {
                if out.len() >= 5000 {
                    return;
                }
                if p.is_dir() {
                    if depth < 4 && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                        walk(&p, depth + 1, out);
                    }
                } else {
                    out.push(p);
                }
            }
        }
        let mut files = vec![];
        walk(std::path::Path::new(&dir), 0, &mut files);
        let ext = |p: &PathBuf| p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let key = |p: &PathBuf| (p.parent().map(PathBuf::from), p.file_stem().map(|s| s.to_string_lossy().to_lowercase()));
        let texts: Vec<&PathBuf> = files.iter().filter(|p| ext(p) == "txt").collect();
        let images: Vec<&PathBuf> = files.iter().filter(|p| IMAGE_EXT.contains(&ext(p).as_str())).collect();
        let rel = |p: &PathBuf| p.strip_prefix(&dir).unwrap_or(p).to_string_lossy().to_string();
        let mut out = FolderScan { pages: vec![], no_text: vec![], no_photo: vec![] };
        // a photo with the text of its own name, else the text it is a numbered part of
        let text_of = |im: &PathBuf| -> Option<(&PathBuf, u32)> {
            if let Some(t) = texts.iter().find(|t| key(t) == key(im)) {
                return Some((t, 0));
            }
            let stem = im.file_stem()?.to_string_lossy().to_string();
            let (page, n) = page_of(&stem);
            let n = n?;
            texts.iter().find(|t| t.parent() == im.parent() && t.file_stem().is_some_and(|s| s.to_string_lossy().to_lowercase() == page)).map(|t| (*t, n))
        };
        let mut grouped: Vec<(&PathBuf, Vec<(u32, &PathBuf)>)> = vec![];
        for im in &images {
            match text_of(im) {
                Some((t, n)) => match grouped.iter_mut().find(|g| g.0 == t) {
                    Some(g) => g.1.push((n, im)),
                    None => grouped.push((t, vec![(n, im)])),
                },
                None => out.no_text.push(rel(im)),
            }
        }
        for (t, mut ims) in grouped {
            ims.sort();
            let images: Vec<String> = ims.iter().map(|(_, p)| p.to_string_lossy().to_string()).collect();
            let lines = std::fs::read(t).map(|b| io::decode_text(&b).lines().filter(|l| !l.trim().is_empty()).count()).unwrap_or(0);
            let name = if ims.len() > 1 { format!("{} (+{})", rel(ims[0].1), ims.len() - 1) } else { rel(ims[0].1) };
            out.pages.push(FolderPage { name, again: done.contains(&images[0]), images, text: t.to_string_lossy().to_string(), lines });
        }
        out.no_photo = texts.iter().filter(|t| !out.pages.iter().any(|p| p.text == t.to_string_lossy())).map(|t| rel(t)).collect();
        Ok(out)
    })
    .await
}

#[derive(Deserialize)]
pub struct OwnItem {
    images: Vec<String>,
    text: String,
}

/// Pages of the person's own writing, each a photo and its text file.
#[tauri::command]
pub fn import_own_pages(app: AppHandle, s: State<AppState>, hand: String, pages: Vec<OwnItem>) -> Res<u64> {
    let h = s.hand(&hand)?;
    let n = pages.len();
    Ok(s.jobs.spawn(&app, "photo", n, move |ctx| {
        let st = ctx.app.state::<AppState>();
        let mut reports = vec![];
        for (i, p) in pages.iter().enumerate() {
            if ctx.cancelled() {
                break;
            }
            let name = p.images.first().and_then(|f| PathBuf::from(f).file_name().map(|f| f.to_string_lossy().to_string())).unwrap_or_default();
            ctx.progress(i, &name);
            let rep = (|| -> anyhow::Result<extract::Report> {
                let text = io::decode_text(&std::fs::read(&p.text)?);
                let mut hw = h.write();
                let r = import_own(&mut hw, &p.images, &text, "A4", false)?;
                st.lib.save_hand(&hw)?;
                Ok(r)
            })();
            reports.push(match rep {
                Ok(r) => serde_json::json!({"file": name, "report": r}),
                Err(e) => serde_json::json!({"file": name, "report": {"ok": false, "glyphs": 0, "warnings": [], "error": e.to_string()}}),
            });
        }
        let mut hw = h.write();
        tidy_save(&st, &mut hw)?;
        Ok(serde_json::Value::Array(reports))
    }))
}

#[derive(Deserialize)]
pub struct GlyphPatch {
    text: Option<String>,
    on: Option<bool>,
    bl: Option<f32>,
    edits: Option<Vec<Edit>>,
}

#[tauri::command]
pub async fn update_glyph(s: State<'_, AppState>, hand: String, id: String, patch: GlyphPatch) -> Res<()> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    let g: &mut Glyph = hw.get_mut(&id).ok_or("glyph not found")?;
    if let Some(t) = patch.text.filter(|t| !t.trim().is_empty()) {
        g.text = charset::normalize(t.trim());
    }
    if let Some(o) = patch.on {
        g.on = o;
    }
    if let Some(b) = patch.bl {
        g.bl = b;
    }
    let edited = patch.edits.is_some();
    if let Some(e) = patch.edits {
        g.edits = e;
    }
    if edited {
        hw.invalidate(&id);
    }
    hw.refresh();
    tidy_save(&s, &mut hw)
}

/// Cut one glyph image in two at column `x` (glyph px) — for pairs that were captured as one.
#[tauri::command]
pub fn split_glyph(s: State<AppState>, hand: String, id: String, x: f32, left: String, right: String) -> Res<()> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    let g = hw.get(&id).cloned().ok_or("glyph not found")?;
    let a = hw.alpha(&id);
    let xi = (x.round() as usize).clamp(1, a.w.saturating_sub(1));
    let pad = (0.6 * g.ppm) as usize;
    for (text, part) in [(left, a.crop(0, 0, xi, a.h)), (right, a.crop(xi, 0, a.w, a.h))] {
        let text = charset::normalize(text.trim());
        if text.is_empty() {
            continue;
        }
        if let Some((crop, _, cy)) = hw_core::matte::tight_crop(&part, pad) {
            hw.add_glyph(&text, &crop, g.bl - cy as f32, g.ppm, g.src, false).map_err(err)?;
        }
    }
    hw.remove_glyph(&id);
    hw.refresh();
    tidy_save(&s, &mut hw)
}

#[tauri::command]
pub fn glyph_edits(s: State<AppState>, hand: String, id: String) -> Res<Vec<Edit>> {
    let h = s.hand(&hand)?;
    let hw = h.read();
    Ok(hw.get(&id).map(|g| g.edits.clone()).unwrap_or_default())
}

#[tauri::command]
pub fn delete_glyphs(s: State<AppState>, hand: String, ids: Vec<String>) -> Res<()> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    for id in ids {
        hw.remove_glyph(&id);
    }
    hw.refresh();
    tidy_save(&s, &mut hw)
}

/// These versions are print letters (or joined writing); only letters have a style.
#[tauri::command]
pub fn set_glyphs_print(s: State<AppState>, hand: String, ids: Vec<String>, print: bool) -> Res<()> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    for g in hw.glyphs.iter_mut().filter(|g| ids.contains(&g.id)) {
        g.print = print && hw_core::hand::has_letter(&g.text);
    }
    tidy_save(&s, &mut hw)
}

#[tauri::command]
pub fn set_hand_stats(s: State<AppState>, hand: String, join: Option<f32>, xh_mm: Option<f32>) -> Res<()> {
    let h = s.hand(&hand)?;
    let mut hw = h.write();
    if let Some(j) = join {
        hw.meta.stats.join = j.clamp(0.0, 1.0);
    }
    if let Some(x) = xh_mm {
        hw.meta.xh_mm = x.clamp(1.0, 8.0);
    }
    s.save_hand(&hw)
}

#[tauri::command]
pub async fn export_hand(s: State<'_, AppState>, id: String, path: String) -> Res<()> {
    s.lib.export_hand(&id, &PathBuf::from(path)).map_err(err)
}

#[tauri::command]
pub async fn import_hand(s: State<'_, AppState>, path: String) -> Res<String> {
    s.lib.import_hand(&PathBuf::from(path)).map_err(err)
}

// =========================================================================== papers
#[derive(Serialize)]
pub struct PaperView {
    meta: PaperMeta,
    /// writing lines in photo px, per shot
    lines_px: Vec<Vec<Vec<[f64; 2]>>>,
}

fn paper_view(p: PaperMeta) -> PaperView {
    let lines_px = (0..p.shots.len())
        .map(|i| {
            let g = PaperGeom::new(&p, &Placement { shot: Some(i), mesh: None, ..Default::default() }, 1.0);
            p.writing_lines(i).iter().map(|l| l.iter().map(|&q| g.h.apply(q)).collect()).collect()
        })
        .collect();
    PaperView { meta: p, lines_px }
}

#[tauri::command]
pub fn list_papers(s: State<AppState>) -> Res<Vec<PaperMeta>> {
    s.lib.papers().map_err(err)
}

#[tauri::command]
pub fn get_paper(s: State<AppState>, id: String) -> Res<PaperView> {
    Ok(paper_view(s.lib.load_paper(&id).map_err(err)?))
}

/// A photo of paper, as a notebook's left or right page or as any paper.
#[tauri::command]
pub async fn add_paper(s: State<'_, AppState>, name: String, path: String, side: Option<paper::Side>) -> Res<PaperView> {
    let bytes = std::fs::read(&path).map_err(err)?;
    let root = s.lib.root.clone();
    let p = blocking(move || {
        let lib = hw_core::store::Library::open(&root).map_err(err)?;
        let mut p = lib.add_paper(&name, &bytes).map_err(err)?;
        p.side = side.unwrap_or_default();
        lib.save_paper(&p).map_err(err)?;
        Ok(p)
    })
    .await?;
    Ok(paper_view(p))
}

#[tauri::command]
pub async fn add_paper_shot(s: State<'_, AppState>, id: String, path: String) -> Res<PaperView> {
    let bytes = std::fs::read(&path).map_err(err)?;
    let root = s.lib.root.clone();
    let p = blocking(move || hw_core::store::Library::open(&root).and_then(|l| l.add_shot(&id, &bytes)).map_err(err)).await?;
    Ok(paper_view(p))
}

#[derive(Deserialize)]
pub struct Calib {
    shot: usize,
    corners: Option<[[f64; 2]; 4]>,
    size: Option<String>,
    step: Option<usize>,
}

/// Re-measure a shot (new corners / size), or change the grid step.
#[tauri::command]
pub async fn calibrate_paper(s: State<'_, AppState>, id: String, c: Calib) -> Res<PaperView> {
    let mut p = s.lib.load_paper(&id).map_err(err)?;
    let photo = s.photo(&p, c.shot, 0)?;
    // no size given: the main photo's size is measured again (a close-up of squared paper from
    // its squares); another photo keeps the paper's size
    let size = c.size.or_else(|| (c.shot > 0).then(|| p.size.clone()));
    let file = p.shots[c.shot].file.clone();
    let corners = c.corners;
    let (shot, measured) = blocking(move || Ok(paper::measure_shot(&photo, &file, corners, size.as_deref()))).await?;
    p.size = measured;
    p.shots[c.shot] = shot;
    if c.shot == 0 {
        p.classify();
    }
    if let Some(st) = c.step {
        p.step = st.max(1);
    }
    p.defaults();
    s.lib.save_paper(&p).map_err(err)?;
    Ok(paper_view(p))
}

#[derive(Deserialize)]
pub struct PaperPatch {
    name: Option<String>,
    start_line: Option<usize>,
    left_mm: Option<f64>,
    right_mm: Option<f64>,
    kind: Option<paper::PaperKind>,
    side: Option<paper::Side>,
    /// replace the ruled lines of a shot (sheet mm polylines)
    rules: Option<(usize, Vec<Vec<[f64; 2]>>)>,
    /// bend the writing area's left and right edges on a shot (x shifts down the sheet)
    area_bend: Option<(usize, [Vec<f64>; 2])>,
    remove_shot: Option<usize>,
}

#[tauri::command]
pub fn update_paper(s: State<AppState>, id: String, patch: PaperPatch) -> Res<PaperView> {
    let mut p = s.lib.load_paper(&id).map_err(err)?;
    if let Some(n) = patch.name {
        p.name = n;
    }
    if let Some(v) = patch.start_line {
        p.start_line = v;
    }
    if let Some(v) = patch.left_mm {
        p.left_mm = v;
    }
    if let Some(v) = patch.right_mm {
        p.right_mm = v;
    }
    if let Some(k) = patch.kind {
        p.kind = k;
    }
    if let Some(v) = patch.side {
        p.side = v;
    }
    if let Some((shot, rules)) = patch.rules {
        if let Some(sh) = p.shots.get_mut(shot) {
            sh.rules = rules;
            sh.rules_edited = true;
            sh.rules.sort_by(|a, b| a[a.len() / 2][1].partial_cmp(&b[b.len() / 2][1]).unwrap());
        }
    }
    if let Some((shot, bend)) = patch.area_bend {
        if let Some(sh) = p.shots.get_mut(shot) {
            sh.area_bend = Some(bend);
        }
    }
    if let Some(i) = patch.remove_shot {
        if p.shots.len() > 1 && i < p.shots.len() {
            let f = p.shots.remove(i).file;
            let _ = std::fs::remove_file(s.lib.paper_dir(&id).join(f));
            s.drop_photos(&id);
        }
    }
    s.lib.save_paper(&p).map_err(err)?;
    Ok(paper_view(p))
}

#[tauri::command]
pub fn delete_paper(s: State<AppState>, id: String) -> Res<()> {
    s.drop_photos(&id);
    s.lib.delete_paper(&id).map_err(err)
}

// =========================================================================== projects
#[tauri::command]
pub fn list_projects(s: State<AppState>) -> Res<Vec<hw_core::store::Summary>> {
    s.lib.projects().map_err(err)
}

#[tauri::command]
pub fn create_project(s: State<AppState>, name: String, hand: Option<String>) -> Res<Project> {
    let mut p = s.lib.create_project(&name).map_err(err)?;
    p.hand = hand.or_else(|| s.lib.hands().ok().and_then(|h| h.first().map(|x| x.id.clone())));
    // a notebook when there are its pages, else the first paper
    let papers = s.lib.papers().unwrap_or_default();
    let notebook = papers.iter().any(|x| x.side != paper::Side::Any);
    p.paper = if notebook { Some(NOTEBOOK.into()) } else { papers.first().map(|x| x.id.clone()) };
    s.lib.save_project(&p).map_err(err)?;
    Ok(p)
}

#[tauri::command]
pub fn get_project(s: State<AppState>, id: String) -> Res<Project> {
    s.lib.load_project(&id).map_err(err)
}

/// Save a project; when the editor sent `source` text, the blocks are re-parsed from it here,
/// so Rust stays the only parser of formulas, tables and lists.
#[tauri::command]
pub fn save_project(s: State<AppState>, mut project: Project) -> Res<Project> {
    project.sync_blocks();
    s.lib.save_project(&project).map_err(err)?;
    Ok(project)
}

#[tauri::command]
pub fn delete_project(s: State<AppState>, id: String) -> Res<()> {
    s.lib.delete_project(&id).map_err(err)
}

/// Read any supported document into blocks (the editor converts them to its own format).
#[tauri::command]
pub async fn import_document(s: State<'_, AppState>, path: String) -> Res<Vec<Block>> {
    let dir = s.pdfium_dir.clone();
    blocking(move || ingest::load(&PathBuf::from(path), dir.as_deref()).map_err(err)).await
}

#[tauri::command]
pub fn html_to_blocks(html: String) -> Vec<Block> {
    ingest::html::from_html(&html)
}

/// A document written in a notebook: left and right pages take turns, each a photo picked at
/// random from that side's papers.
pub const NOTEBOOK: &str = "notebook";
/// Every page on a photo picked at random from all papers.
pub const RANDOM: &str = "random";

/// Pages the random picks are made for; after them they come round again.
const PICKS: usize = 60;

/// The paper (and the photo of it) of each page: page i is on sheets[i % len]. One paper, a
/// notebook (left, right, left…) or random papers; picks follow the document's variation seed.
/// The writing area is the paper's own (set in Paper).
pub fn sheets(s: &AppState, p: &Project, paper: Option<&str>) -> Res<Vec<(PaperMeta, Placement)>> {
    let place = |shot: usize| Placement { shot: Some(shot), ..Default::default() };
    let photos = |list: Vec<PaperMeta>| -> Vec<(PaperMeta, Placement)> { list.into_iter().flat_map(|pm| (0..pm.shots.len()).map(move |i| (pm.clone(), place(i)))).collect() };
    let picked = |pool: &[(PaperMeta, Placement)], n: usize, seed: u64| -> Vec<(PaperMeta, Placement)> { paper::pick_photos(pool.len(), n, seed).into_iter().map(|i| pool[i].clone()).collect() };
    let seed = p.params.seed;
    match paper.or(p.paper.as_deref()) {
        None => Ok(vec![]),
        Some(NOTEBOOK) => {
            let papers = s.lib.papers().map_err(err)?;
            let side = |sd: paper::Side| photos(papers.iter().filter(|pm| pm.side == sd).cloned().collect());
            let (left, right) = (side(paper::Side::Left), side(paper::Side::Right));
            Ok(match (left.is_empty(), right.is_empty()) {
                (true, true) => return Err("add a notebook's left and right pages in Paper".into()),
                (false, true) => picked(&left, PICKS, seed),
                (true, false) => picked(&right, PICKS, seed),
                // left, right, left…: the first page is a left one
                _ => picked(&left, PICKS / 2, seed).into_iter().zip(picked(&right, PICKS / 2, seed ^ 1)).flat_map(|(l, r)| [l, r]).collect(),
            })
        }
        Some(RANDOM) => {
            let all = photos(s.lib.papers().map_err(err)?);
            if all.is_empty() {
                return Err("add a paper photo in Paper".into());
            }
            Ok(picked(&all, PICKS, seed))
        }
        Some(id) => {
            let pm = s.lib.load_paper(id).map_err(err)?;
            let shot = p.place.get(&pm.id).and_then(|pl| pl.shot).unwrap_or(0).min(pm.shots.len() - 1);
            Ok(vec![(pm, place(shot))])
        }
    }
}

/// The layout of a project on its pages' papers (none: plain pages, for SVG and plotters).
pub fn project_layout(s: &AppState, p: &Project, sheets: &[(PaperMeta, Placement)]) -> Res<LayoutResult> {
    let hid = p.hand.as_ref().ok_or("choose a hand")?;
    let h = s.hand(hid)?;
    let h = h.read();
    let geos: Vec<Geometry> = sheets
        .iter()
        .map(|(pm, pl)| {
            let pg = PaperGeom::new(pm, pl, 1.0);
            Geometry { width_mm: pg.width() as f32, widths: pg.widths().iter().map(|&w| w as f32).collect(), lines_per_page: pg.lines_per_page(), pitch_mm: pg.pitch as f32, ruled: pm.kind != paper::PaperKind::Blank }
        })
        .collect();
    Ok(layout::layout_pages(&p.blocks, &h, &p.params, if geos.is_empty() { vec![Geometry::default()] } else { geos }))
}

#[derive(Serialize)]
pub struct LayoutInfo {
    pages: usize,
    /// pages as clean sheets (another size than the paper's)
    clean_pages: usize,
    /// characters the person never wrote, with how often the document uses each
    missing: Vec<(char, usize)>,
    /// letters to print the person wrote only joined
    missing_print: Vec<(char, usize)>,
    /// width of the sheet, mm (margins are measured on it)
    sheet_w_mm: f64,
    lines: usize,
    lines_per_page: usize,
    start_line: usize,
    left_mm: f64,
    right_mm: f64,
    nodes: Vec<Vec<[f64; 2]>>,
    mesh: Vec<Vec<[f64; 2]>>,
    w: u32,
    h: u32,
    shot: usize,
    shots: usize,
    /// the paper of this page (a notebook's left or right page)
    paper: Option<String>,
    side: paper::Side,
    /// every page: its paper and photo
    sheets: Vec<PageSheet>,
}

#[derive(Serialize, Clone)]
pub struct PageSheet {
    paper: String,
    name: String,
    side: paper::Side,
    shot: usize,
    w: u32,
    h: u32,
}

#[tauri::command]
pub async fn layout_info(s: State<'_, AppState>, project: String, paper: Option<String>, page: Option<usize>) -> Res<LayoutInfo> {
    let p = s.lib.load_project(&project).map_err(err)?;
    let sh = sheets(&s, &p, paper.as_deref())?;
    let res = project_layout(&s, &p, &sh)?;
    // exactly what is not in the person's writing, in the order the document uses it
    let counts = synth::char_counts(&p.blocks);
    let missing: Vec<(char, usize)> = {
        let h = s.hand(p.hand.as_deref().unwrap_or_default())?;
        let h = h.read();
        synth::missing(&h, synth::chars_of(&p.blocks).into_iter().chain(res.missing.iter().copied())).into_iter().map(|c| (c, counts.get(&c).copied().unwrap_or(1))).collect()
    };
    let missing_print: Vec<(char, usize)> = res.missing_print.iter().filter(|c| !missing.iter().any(|m| m.0 == **c)).map(|&c| (c, counts.get(&c).copied().unwrap_or(1))).collect();
    let page = page.unwrap_or(0).min(res.pages.len().saturating_sub(1));
    let clean_pages = project_layout(&s, &p, &[])?.pages.len();
    let sheets: Vec<PageSheet> = if sh.is_empty() {
        vec![]
    } else {
        (0..res.pages.len().max(1))
            .map(|i| {
                let (pm, pl) = &sh[i % sh.len()];
                let shot = pl.shot.unwrap_or(0).min(pm.shots.len() - 1);
                PageSheet { paper: pm.id.clone(), name: pm.name.clone(), side: pm.side, shot, w: pm.shots[shot].w, h: pm.shots[shot].h }
            })
            .collect()
    };
    Ok(match sh.get(page % sh.len().max(1)) {
        Some((pm, pl)) => {
            let g = PaperGeom::new(pm, pl, 1.0);
            let shot = pl.shot.unwrap_or(0).min(pm.shots.len() - 1);
            LayoutInfo {
                pages: res.pages.len(),
                clean_pages,
                missing: missing.clone(),
                missing_print: missing_print.clone(),
                sheet_w_mm: g.size.0,
                lines: g.lines.len(),
                lines_per_page: g.lines_per_page(),
                start_line: g.start,
                left_mm: g.left,
                right_mm: g.right,
                nodes: g.base_nodes(),
                mesh: pl.mesh.clone().unwrap_or_else(|| vec![vec![[0.0, 0.0]; paper::MESH.1]; paper::MESH.0]),
                w: pm.shots[shot].w,
                h: pm.shots[shot].h,
                shot,
                shots: pm.shots.len(),
                paper: Some(pm.id.clone()),
                side: pm.side,
                sheets,
            }
        }
        _ => LayoutInfo {
            pages: res.pages.len(),
            clean_pages,
            missing,
            missing_print,
            sheet_w_mm: 0.0,
            lines: 0,
            lines_per_page: 32,
            start_line: 0,
            left_mm: 0.0,
            right_mm: 0.0,
            nodes: vec![],
            mesh: vec![],
            w: 0,
            h: 0,
            shot: 0,
            shots: 0,
            paper: None,
            side: paper::Side::Any,
            sheets,
        },
    })
}

/// Swap the glyph at (word, char) for the next variant of the same letter.
#[tauri::command]
pub fn cycle_glyph(s: State<AppState>, project: String, w: u32, c: u32) -> Res<Option<String>> {
    let mut p = s.lib.load_project(&project).map_err(err)?;
    let res = project_layout(&s, &p, &[])?;
    let cur = res.pages.iter().flatten().find_map(|e| match e {
        layout::Elem::G { gid, wi, ci, .. } if *wi == w && *ci == c => Some(gid.clone()),
        _ => None,
    });
    let Some(cur) = cur else { return Ok(None) };
    let h = s.hand(p.hand.as_ref().ok_or("no hand")?)?;
    let h = h.read();
    let text = h.get(&cur).map(|g| g.text.clone()).unwrap_or_default();
    let mut it = text.chars();
    // same letter in either script (a / а share one bank)
    let keys: Vec<String> = match (it.next(), it.next()) {
        (Some(ch), None) => charset::candidates(ch).iter().take(2).map(|k| k.to_string()).collect(),
        _ => vec![text.clone()],
    };
    let options: Vec<String> = h.glyphs.iter().filter(|g| g.on && keys.contains(&g.text)).map(|g| g.id.clone()).collect();
    if options.len() < 2 {
        return Ok(None);
    }
    let i = options.iter().position(|x| *x == cur).map(|i| (i + 1) % options.len()).unwrap_or(0);
    p.params.overrides.insert(format!("{w}:{c}"), options[i].clone());
    s.lib.save_project(&p).map_err(err)?;
    Ok(Some(options[i].clone()))
}

// =========================================================================== export
#[derive(Deserialize, Clone)]
pub struct ExportReq {
    project: String,
    paper: Option<String>,
    /// "jpg" | "png" | "pdf" | "svg" | "gcode"
    formats: Vec<String>,
    pages: Option<Vec<usize>>,
    shots_per_page: usize,
    random: bool,
    quality: u8,
    /// output width in px (0 = photo size)
    width: u32,
    folder: String,
    template: String,
    style: Option<ShotStyle>,
    gcode: Option<gcode::Profile>,
}

fn file_name(t: &str, name: &str, page: usize, n: usize, ext: &str) -> String {
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    let base = t.replace("{name}", clean.trim()).replace("{page}", &format!("{:02}", page + 1)).replace("{n}", &n.to_string());
    format!("{base}.{ext}")
}

#[tauri::command]
pub fn export_project(app: AppHandle, s: State<AppState>, req: ExportReq) -> Res<u64> {
    let p = s.lib.load_project(&req.project).map_err(err)?;
    let sh = sheets(&s, &p, req.paper.as_deref())?;
    let res = project_layout(&s, &p, &sh)?;
    let hand = s.hand(p.hand.as_ref().ok_or("choose a hand")?)?;
    let pages: Vec<usize> = req.pages.clone().unwrap_or_else(|| (0..res.pages.len()).collect()).into_iter().filter(|&i| i < res.pages.len()).collect();
    let photos = req.formats.iter().any(|f| f == "jpg" || f == "png" || f == "pdf");
    if photos && sh.is_empty() {
        return Err("choose a paper".into());
    }
    let shots = req.shots_per_page.max(1);
    let total = pages.len() * if photos { shots } else { 1 };
    let svg_layout = project_layout(&s, &p, &[])?;
    let chosen = req.paper.as_deref().or(p.paper.as_deref());
    let (notebook, random_papers) = (chosen == Some(NOTEBOOK), chosen == Some(RANDOM));
    Ok(s.jobs.spawn(&app, "export", total, move |ctx| {
        let s = ctx.app.state::<AppState>();
        std::fs::create_dir_all(&req.folder).map_err(err)?;
        let folder = PathBuf::from(&req.folder);
        let h = hand.read();
        let ink = p.pen.color.unwrap_or(h.meta.ink);
        let mut files = vec![];
        let mut jpegs = vec![];
        let mut done = 0;
        let mut seed: u64 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7);
        for &pi in &pages {
            if !sh.is_empty() {
                if photos {
                    for k in 0..shots {
                        if ctx.cancelled() {
                            return Ok(serde_json::json!({ "files": files, "cancelled": true }));
                        }
                        // more photos of one page: the next photos of its paper (of its side, in a notebook)
                        let (pm, pl) = if notebook { &sh[(pi + 2 * k) % sh.len()] } else { &sh[pi % sh.len()] };
                        let shot = if notebook || random_papers || pm.shots.len() < 2 { pl.shot.unwrap_or(0) } else { (pl.shot.unwrap_or(0) + k) % pm.shots.len() };
                        let mut place = pl.clone();
                        if Some(shot) != pl.shot {
                            place.mesh = None;
                        }
                        place.shot = Some(shot);
                        let full = s.photo(pm, shot, 0)?;
                        let scale = if req.width > 0 && (req.width as usize) < full.w { req.width as f32 / full.w as f32 } else { 1.0 };
                        let photo = if scale < 1.0 { s.photo(pm, shot, req.width)? } else { full };
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                        let opts = RenderOpts { scale, random: req.random, seed, mess: p.params.mess, pen: p.pen, shot: req.style.unwrap_or(p.shot) };
                        let img = render::render(pm, &photo, &h, &res.pages[pi], &place, &opts).to_image();
                        let q = if req.random { req.quality.saturating_sub((seed % 6) as u8) } else { req.quality };
                        let jpg = io::jpeg(&img, q.clamp(40, 100));
                        if req.formats.iter().any(|f| f == "jpg") {
                            let f = folder.join(file_name(&req.template, &p.name, pi, k + 1, "jpg"));
                            std::fs::write(&f, &jpg).map_err(err)?;
                            files.push(f.to_string_lossy().to_string());
                        }
                        if req.formats.iter().any(|f| f == "png") {
                            let f = folder.join(file_name(&req.template, &p.name, pi, k + 1, "png"));
                            img.save(&f).map_err(err)?;
                            files.push(f.to_string_lossy().to_string());
                        }
                        if req.formats.iter().any(|f| f == "pdf") {
                            jpegs.push(jpg);
                        }
                        done += 1;
                        ctx.progress(done, format!("page {} / shot {}", pi + 1, k + 1));
                    }
                }
            }
            if let Some(elems) = svg_layout.pages.get(pi) {
                if req.formats.iter().any(|f| f == "svg") {
                    let f = folder.join(file_name(&req.template, &p.name, pi, 1, "svg"));
                    std::fs::write(&f, svg::page_svg(&h, elems, &Geometry::default(), ink, true)).map_err(err)?;
                    files.push(f.to_string_lossy().to_string());
                }
                if req.formats.iter().any(|f| f == "gcode") {
                    let f = folder.join(file_name(&req.template, &p.name, pi, 1, "gcode"));
                    let paths = svg::plot_paths(&h, elems, &Geometry::default());
                    std::fs::write(&f, gcode::to_gcode(&paths, &req.gcode.clone().unwrap_or_default())).map_err(err)?;
                    files.push(f.to_string_lossy().to_string());
                }
            }
            if !photos {
                done += 1;
                ctx.progress(done, format!("page {}", pi + 1));
            }
        }
        if !jpegs.is_empty() {
            let f = folder.join(file_name("{name}", &p.name, 0, 0, "pdf"));
            std::fs::write(&f, pdfx::photos_to_pdf(&jpegs).map_err(err)?).map_err(err)?;
            files.push(f.to_string_lossy().to_string());
        }
        Ok(serde_json::json!({ "files": files, "folder": req.folder }))
    }))
}

#[tauri::command]
pub fn cancel_job(s: State<AppState>, id: u64) {
    s.jobs.cancel(id);
}

// =========================================================================== settings / misc
#[tauri::command]
pub fn get_setting(s: State<AppState>, key: String) -> Option<String> {
    s.lib.setting(&key)
}

/// The app was opened on a copy of a library for testing (not the person's own).
#[tauri::command]
pub fn test_copy() -> bool {
    std::env::var("HW_DATA").is_ok()
}

/// Every setting kept in the library (the interface's choices, should its own storage be lost).
#[tauri::command]
pub fn all_settings(s: State<AppState>) -> Res<Vec<(String, String)>> {
    s.lib.settings().map_err(err)
}

#[tauri::command]
pub fn set_setting(s: State<AppState>, key: String, value: String) -> Res<()> {
    s.lib.set_setting(&key, &value).map_err(err)
}

#[tauri::command]
pub fn library_path(s: State<AppState>) -> String {
    s.lib.root.to_string_lossy().to_string()
}

/// Show a folder (or the folder a file is in) in the file manager.
#[tauri::command]
pub fn open_folder(path: String) -> Res<()> {
    let p = PathBuf::from(&path);
    let dir = if p.is_dir() { p } else { p.parent().map(|d| d.to_path_buf()).filter(|d| d.is_dir()).ok_or("this folder is not there any more")? };
    std::process::Command::new("explorer").arg(dir).spawn().map_err(err)?;
    Ok(())
}

#[tauri::command]
pub async fn backup_library(s: State<'_, AppState>, path: String) -> Res<()> {
    s.lib.backup(&PathBuf::from(path)).map_err(err)
}

#[tauri::command]
pub fn paper_sizes() -> Vec<(String, f64, f64)> {
    geom::PAPER_SIZES.iter().map(|p| (p.0.to_string(), p.1, p.2)).collect()
}

#[tauri::command]
pub fn pen_presets() -> Vec<(String, [f32; 3])> {
    hw_core::pen::PRESETS.iter().map(|p| (p.0.to_string(), p.1)).collect()
}

#[cfg(test)]
mod tests {
    use super::page_of;

    #[test]
    fn photos_numbered_as_parts_of_a_page() {
        assert_eq!(page_of("3_1"), ("3".into(), Some(1)));
        assert_eq!(page_of("Rus 7-2"), ("rus 7".into(), Some(2)));
        assert_eq!(page_of("page (2)"), ("page".into(), Some(2)));
        assert_eq!(page_of("IMG2041"), ("img2041".into(), None));
        assert_eq!(page_of("12"), ("12".into(), None));
    }
}
