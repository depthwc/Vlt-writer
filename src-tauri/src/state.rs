//! Shared app state: the library, loaded hands, a small cache of decoded/scaled photos.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use hw_core::hand::Hand;
use hw_core::img::Rgb;
use hw_core::store::Library;
use parking_lot::{Mutex, RwLock};

use crate::jobs::Jobs;

pub struct AppState {
    pub lib: Library,
    pub hands: Mutex<HashMap<String, Arc<RwLock<Hand>>>>,
    photos: Mutex<Vec<((String, usize, u32), Arc<Rgb>)>>,
    pub pdfium_dir: Option<PathBuf>,
    pub jobs: Jobs,
}

pub type Res<T> = Result<T, String>;

pub fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

impl AppState {
    pub fn new(lib: Library, pdfium_dir: Option<PathBuf>) -> Self {
        AppState { lib, hands: Mutex::new(HashMap::new()), photos: Mutex::new(vec![]), pdfium_dir, jobs: Jobs::default() }
    }

    pub fn hand(&self, id: &str) -> Res<Arc<RwLock<Hand>>> {
        if let Some(h) = self.hands.lock().get(id) {
            return Ok(h.clone());
        }
        let mut hand = self.lib.load_hand(id).map_err(err)?;
        // versions made up by earlier versions of the app go: only the person's writing stays;
        // a tutuq belgisi cut stuck to its letter is cut apart
        if hw_core::synth::drop_made(&mut hand) + hand.split_apostrophes() > 0 {
            self.lib.save_hand(&hand).map_err(err)?;
        }
        let h = Arc::new(RwLock::new(hand));
        self.hands.lock().insert(id.to_string(), h.clone());
        Ok(h)
    }

    pub fn save_hand(&self, h: &Hand) -> Res<()> {
        self.lib.save_hand(h).map_err(err)
    }

    pub fn forget_hand(&self, id: &str) {
        self.hands.lock().remove(id);
    }

    /// Photo of a paper shot, scaled to `width` px wide (0 = full size). Cached (LRU of 6).
    pub fn photo(&self, paper: &hw_core::paper::PaperMeta, shot: usize, width: u32) -> Res<Arc<Rgb>> {
        let key = (paper.id.clone(), shot, width);
        {
            let mut c = self.photos.lock();
            if let Some(i) = c.iter().position(|(k, _)| *k == key) {
                let e = c.remove(i);
                let v = e.1.clone();
                c.push(e);
                return Ok(v);
            }
        }
        let full = if width != 0 {
            self.photo(paper, shot, 0)?
        } else {
            Arc::new(self.lib.shot_photo(paper, shot).map_err(err)?)
        };
        let img = if width != 0 && (width as usize) < full.w {
            Arc::new(full.resize(width as usize, full.h * width as usize / full.w))
        } else {
            full
        };
        let mut c = self.photos.lock();
        c.push((key, img.clone()));
        if c.len() > 6 {
            c.remove(0);
        }
        Ok(img)
    }

    pub fn drop_photos(&self, paper: &str) {
        self.photos.lock().retain(|(k, _)| k.0 != paper);
    }
}
