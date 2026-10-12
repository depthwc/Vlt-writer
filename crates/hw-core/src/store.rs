//! The library on disk: one SQLite file for metadata, plain image files for glyphs and photos.
//!
//! root/library.db
//! root/hands/<id>/<glyph>.png
//! root/papers/<id>/<shot>.jpg
//! root/projects/<id>/out/*.jpg|pdf|svg|gcode

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::hand::{self, Glyph, Hand, HandMeta};
use crate::img::Rgb;
use crate::paper::{self, PaperMeta};
use crate::project::Project;

pub struct Library {
    pub root: PathBuf,
    db: Mutex<Connection>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub created: f64,
    pub count: usize,
}

impl Library {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        for d in ["hands", "papers", "projects"] {
            std::fs::create_dir_all(root.join(d))?;
        }
        let db = Connection::open(root.join("library.db"))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS hands(id TEXT PRIMARY KEY, name TEXT, created REAL, meta TEXT, glyphs TEXT);
             CREATE TABLE IF NOT EXISTS papers(id TEXT PRIMARY KEY, name TEXT, created REAL, meta TEXT);
             CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY, name TEXT, created REAL, updated REAL, meta TEXT);
             CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY, v TEXT);
             CREATE TABLE IF NOT EXISTS sheet_pages(code INTEGER PRIMARY KEY, created REAL, page TEXT);",
        )?;
        Ok(Library { root: root.to_path_buf(), db: Mutex::new(db) })
    }

    // ---------------------------------------------------------------- settings
    pub fn setting(&self, k: &str) -> Option<String> {
        self.db.lock().query_row("SELECT v FROM settings WHERE k=?1", [k], |r| r.get(0)).optional().ok().flatten()
    }
    /// Every setting kept.
    pub fn settings(&self) -> anyhow::Result<Vec<(String, String)>> {
        let db = self.db.lock();
        let mut st = db.prepare("SELECT k, v FROM settings")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_setting(&self, k: &str, v: &str) -> anyhow::Result<()> {
        self.db.lock().execute("INSERT INTO settings(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=?2", [k, v])?;
        Ok(())
    }

    // ---------------------------------------------------------------- printed pages
    /// Keep pages that cannot be rebuilt from their code (sheets of missing letters): each
    /// gets the next free stored code, oldest ones are reused after a full cycle.
    pub fn store_pages(&self, pages: &mut [crate::sheet::Page]) -> anyhow::Result<()> {
        let mut next: u32 = self.setting("sheet_next").and_then(|v| v.parse().ok()).unwrap_or(0);
        for p in pages.iter_mut() {
            p.code = crate::sheet::stored_code(next);
            self.db.lock().execute(
                "INSERT INTO sheet_pages(code,created,page) VALUES(?1,?2,?3) ON CONFLICT(code) DO UPDATE SET created=?2, page=?3",
                params![next % crate::sheet::STORED_MAX, hand::now(), serde_json::to_string(p)?],
            )?;
            next = (next + 1) % crate::sheet::STORED_MAX;
        }
        self.set_setting("sheet_next", &next.to_string())
    }

    pub fn stored_page(&self, code: crate::sheet::Code) -> Option<crate::sheet::Page> {
        if !crate::sheet::is_stored(code) {
            return None;
        }
        let n = (0..crate::sheet::STORED_MAX).find(|&n| crate::sheet::stored_code(n) == code)?;
        let s: String = self.db.lock().query_row("SELECT page FROM sheet_pages WHERE code=?1", [n], |r| r.get(0)).optional().ok().flatten()?;
        serde_json::from_str(&s).ok()
    }

    // ---------------------------------------------------------------- hands
    pub fn hands(&self) -> anyhow::Result<Vec<Summary>> {
        let db = self.db.lock();
        let mut st = db.prepare("SELECT id, name, created, glyphs FROM hands ORDER BY created")?;
        let rows = st.query_map([], |r| {
            let g: String = r.get(3)?;
            let count = serde_json::from_str::<Vec<serde_json::Value>>(&g).map(|v| v.iter().filter(|x| x["src"] != "auto").count()).unwrap_or(0);
            Ok(Summary { id: r.get(0)?, name: r.get(1)?, created: r.get(2)?, count })
        })?;
        Ok(rows.flatten().collect())
    }

    pub fn hand_dir(&self, id: &str) -> PathBuf {
        self.root.join("hands").join(id)
    }

    pub fn create_hand(&self, name: &str) -> anyhow::Result<Hand> {
        let id = hand::new_id();
        let h = Hand::new(HandMeta::new(&id, name), vec![], self.hand_dir(&id));
        std::fs::create_dir_all(&h.dir)?;
        self.save_hand(&h)?;
        Ok(h)
    }

    pub fn load_hand(&self, id: &str) -> anyhow::Result<Hand> {
        let (meta, glyphs): (String, String) = self.db.lock().query_row("SELECT meta, glyphs FROM hands WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let meta: HandMeta = serde_json::from_str(&meta)?;
        let glyphs: Vec<Glyph> = serde_json::from_str(&glyphs)?;
        Ok(Hand::new(meta, glyphs, self.hand_dir(id)))
    }

    pub fn save_hand(&self, h: &Hand) -> anyhow::Result<()> {
        self.db.lock().execute(
            "INSERT INTO hands(id,name,created,meta,glyphs) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET name=?2, meta=?4, glyphs=?5",
            params![h.meta.id, h.meta.name, h.meta.created, serde_json::to_string(&h.meta)?, serde_json::to_string(&h.glyphs)?],
        )?;
        Ok(())
    }

    pub fn delete_hand(&self, id: &str) -> anyhow::Result<()> {
        self.db.lock().execute("DELETE FROM hands WHERE id=?1", [id])?;
        let _ = std::fs::remove_dir_all(self.hand_dir(id));
        Ok(())
    }

    // ---------------------------------------------------------------- papers
    pub fn paper_dir(&self, id: &str) -> PathBuf {
        self.root.join("papers").join(id)
    }

    pub fn papers(&self) -> anyhow::Result<Vec<PaperMeta>> {
        let db = self.db.lock();
        let mut st = db.prepare("SELECT meta FROM papers ORDER BY created")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().filter_map(|m| serde_json::from_str(&m).ok()).collect())
    }

    pub fn load_paper(&self, id: &str) -> anyhow::Result<PaperMeta> {
        let m: String = self.db.lock().query_row("SELECT meta FROM papers WHERE id=?1", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&m)?)
    }

    pub fn save_paper(&self, p: &PaperMeta) -> anyhow::Result<()> {
        self.db.lock().execute(
            "INSERT INTO papers(id,name,created,meta) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=?2, meta=?4",
            params![p.id, p.name, p.created, serde_json::to_string(p)?],
        )?;
        Ok(())
    }

    pub fn delete_paper(&self, id: &str) -> anyhow::Result<()> {
        self.db.lock().execute("DELETE FROM papers WHERE id=?1", [id])?;
        let _ = std::fs::remove_dir_all(self.paper_dir(id));
        Ok(())
    }

    fn save_shot_file(&self, pid: &str, bytes: &[u8]) -> anyhow::Result<(String, Rgb)> {
        let img = crate::io::decode_photo(bytes, 4600)?;
        let dir = self.paper_dir(pid);
        std::fs::create_dir_all(&dir)?;
        let file = format!("{}.jpg", hand::new_id());
        std::fs::write(dir.join(&file), crate::io::jpeg(&img, 95))?;
        Ok((file, Rgb::from_image(&img)))
    }

    /// New paper from a photo: decode, store, measure.
    pub fn add_paper(&self, name: &str, bytes: &[u8]) -> anyhow::Result<PaperMeta> {
        let id = hand::new_id();
        let (file, rgb) = self.save_shot_file(&id, bytes)?;
        let (shot, size) = paper::measure_shot(&rgb, &file, None, None);
        let p = PaperMeta::new(&id, name, shot, &size);
        self.save_paper(&p)?;
        Ok(p)
    }

    /// Another photo of the same paper (more variety in exports).
    pub fn add_shot(&self, pid: &str, bytes: &[u8]) -> anyhow::Result<PaperMeta> {
        let mut p = self.load_paper(pid)?;
        let (file, rgb) = self.save_shot_file(pid, bytes)?;
        let (shot, _) = paper::measure_shot(&rgb, &file, None, Some(&p.size));
        p.shots.push(shot);
        self.save_paper(&p)?;
        Ok(p)
    }

    pub fn shot_photo(&self, p: &PaperMeta, shot: usize) -> anyhow::Result<Rgb> {
        let s = &p.shots[shot.min(p.shots.len() - 1)];
        let img = image::open(self.paper_dir(&p.id).join(&s.file))?.to_rgb8();
        Ok(Rgb::from_image(&img))
    }

    // ---------------------------------------------------------------- hand files & backups
    /// `.hand` file: zip with hand.json, glyphs.json and every glyph PNG.
    pub fn export_hand(&self, id: &str, path: &Path) -> anyhow::Result<()> {
        let h = self.load_hand(id)?;
        let mut z = zip::ZipWriter::new(std::fs::File::create(path)?);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file("hand.json", opt)?;
        std::io::Write::write_all(&mut z, serde_json::to_string(&h.meta)?.as_bytes())?;
        z.start_file("glyphs.json", opt)?;
        std::io::Write::write_all(&mut z, serde_json::to_string(&h.glyphs)?.as_bytes())?;
        for g in &h.glyphs {
            if let Ok(b) = std::fs::read(h.png_path(&g.id)) {
                z.start_file(format!("glyphs/{}.png", g.id), opt)?;
                std::io::Write::write_all(&mut z, &b)?;
            }
        }
        z.finish()?;
        Ok(())
    }

    /// Import a `.hand` file as a new hand (new id, so it never overwrites).
    pub fn import_hand(&self, path: &Path) -> anyhow::Result<String> {
        let mut z = zip::ZipArchive::new(std::fs::File::open(path)?)?;
        let read = |z: &mut zip::ZipArchive<std::fs::File>, n: &str| -> anyhow::Result<Vec<u8>> {
            let mut f = z.by_name(n)?;
            let mut v = vec![];
            std::io::Read::read_to_end(&mut f, &mut v)?;
            Ok(v)
        };
        let mut meta: HandMeta = serde_json::from_slice(&read(&mut z, "hand.json")?)?;
        let glyphs: Vec<Glyph> = serde_json::from_slice(&read(&mut z, "glyphs.json")?)?;
        let id = hand::new_id();
        meta.id = id.clone();
        let dir = self.hand_dir(&id);
        std::fs::create_dir_all(&dir)?;
        for g in &glyphs {
            if let Ok(b) = read(&mut z, &format!("glyphs/{}.png", g.id)) {
                std::fs::write(dir.join(format!("{}.png", g.id)), b)?;
            }
        }
        self.save_hand(&Hand::new(meta, glyphs, dir))?;
        Ok(id)
    }

    /// Whole library (database + images) into one zip.
    pub fn backup(&self, path: &Path) -> anyhow::Result<()> {
        self.db.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        let mut z = zip::ZipWriter::new(std::fs::File::create(path)?);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut stack = vec![self.root.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let rel = p.strip_prefix(&self.root)?.to_string_lossy().replace('\\', "/");
                if rel.ends_with("-wal") || rel.ends_with("-shm") {
                    continue;
                }
                z.start_file(rel, opt)?;
                std::io::Write::write_all(&mut z, &std::fs::read(&p)?)?;
            }
        }
        z.finish()?;
        Ok(())
    }

    // ---------------------------------------------------------------- projects
    pub fn project_dir(&self, id: &str) -> PathBuf {
        self.root.join("projects").join(id)
    }

    pub fn projects(&self) -> anyhow::Result<Vec<Summary>> {
        let db = self.db.lock();
        let mut st = db.prepare("SELECT id, name, created FROM projects ORDER BY updated DESC")?;
        let rows = st.query_map([], |r| Ok(Summary { id: r.get(0)?, name: r.get(1)?, created: r.get(2)?, count: 0 }))?;
        Ok(rows.flatten().collect())
    }

    pub fn create_project(&self, name: &str) -> anyhow::Result<Project> {
        let p = Project::new(&hand::new_id(), name);
        std::fs::create_dir_all(self.project_dir(&p.id).join("out"))?;
        self.save_project(&p)?;
        Ok(p)
    }

    pub fn load_project(&self, id: &str) -> anyhow::Result<Project> {
        let m: String = self.db.lock().query_row("SELECT meta FROM projects WHERE id=?1", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&m)?)
    }

    pub fn save_project(&self, p: &Project) -> anyhow::Result<()> {
        self.db.lock().execute(
            "INSERT INTO projects(id,name,created,updated,meta) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET name=?2, updated=?4, meta=?5",
            params![p.id, p.name, p.created, hand::now(), serde_json::to_string(p)?],
        )?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> anyhow::Result<()> {
        self.db.lock().execute("DELETE FROM projects WHERE id=?1", [id])?;
        let _ = std::fs::remove_dir_all(self.project_dir(id));
        Ok(())
    }
}
