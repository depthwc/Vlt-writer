#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod jobs;
mod protocol;
mod state;

use std::path::{Path, PathBuf};

use tauri::Manager;

/// The app's earlier id: its folders are the ones to look in when the library is not yet here.
const OLD_ID: &str = "uz.texttohandwrite";

/// Where the library is. The app was called TextToHandwrite: a library kept under that name moves
/// to this one's folder (and its web view caches go); if it cannot move, it is used where it is.
fn library_root(data: &Path, local: &Path) -> PathBuf {
    let root = data.join("library");
    let old = data.with_file_name(OLD_ID).join("library");
    if !root.exists() && old.exists() {
        let moved = std::fs::create_dir_all(data).is_ok() && std::fs::rename(&old, &root).is_ok();
        if !moved {
            return old;
        }
        let _ = std::fs::remove_dir(data.with_file_name(OLD_ID));
        let _ = std::fs::remove_dir_all(local.with_file_name(OLD_ID));
    }
    root
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("hw", protocol::handler)
        .setup(|app| {
            let root = match std::env::var("HW_DATA") {
                Ok(p) => PathBuf::from(p),
                Err(_) => {
                    let (data, local) = (app.path().app_data_dir()?, app.path().app_local_data_dir()?);
                    library_root(&data, &local)
                }
            };
            let lib = hw_core::store::Library::open(&root)?;
            // pdfium.dll ships as a resource; in development it sits in src-tauri/resources
            let pdfium = [app.path().resource_dir().ok(), Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"))]
                .into_iter()
                .flatten()
                .find(|d| d.join("pdfium.dll").exists());
            app.manage(state::AppState::new(lib, pdfium));
            // a small screen (a 1366×768 laptop at 125%): the window fills it rather than spilling off it
            if std::env::var("HW_DATA").is_err() {
                if let Some(w) = app.get_webview_window("main") {
                    if let (Ok(Some(m)), Ok(size)) = (w.current_monitor(), w.outer_size()) {
                        let area = m.work_area().size;
                        if size.width > area.width || size.height > area.height {
                            let _ = w.maximize();
                        }
                    }
                }
            }
            // a copy of a library opened for testing says so, and with HW_OFFSCREEN stays out of
            // the way of the person's own app (off screen, not on the taskbar)
            if std::env::var("HW_DATA").is_ok() {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_title("Vlt-writer — TEST COPY");
                    if std::env::var("HW_OFFSCREEN").is_ok() {
                        let _ = w.set_skip_taskbar(true);
                        let _ = w.set_position(tauri::PhysicalPosition::new(-4000, 0));
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_hands,
            commands::create_hand,
            commands::rename_hand,
            commands::delete_hand,
            commands::get_hand,
            commands::sheet_plan,
            commands::save_sheet_pdf,
            commands::set_hand_langs,
            commands::photo_preview,
            commands::guide_pages,
            commands::import_guide_page,
            commands::remove_import,
            commands::read_text_file,
            commands::photo_text,
            commands::scan_folder,
            commands::import_own_pages,
            commands::import_sheet_photos,
            commands::import_handwriting_photo,
            commands::update_glyph,
            commands::glyph_edits,
            commands::split_glyph,
            commands::delete_glyphs,
            commands::set_glyphs_print,
            commands::set_hand_stats,
            commands::export_hand,
            commands::import_hand,
            commands::list_papers,
            commands::get_paper,
            commands::add_paper,
            commands::add_paper_shot,
            commands::calibrate_paper,
            commands::update_paper,
            commands::delete_paper,
            commands::list_projects,
            commands::create_project,
            commands::get_project,
            commands::save_project,
            commands::delete_project,
            commands::import_document,
            commands::html_to_blocks,
            commands::layout_info,
            commands::cycle_glyph,
            commands::export_project,
            commands::cancel_job,
            commands::get_setting,
            commands::set_setting,
            commands::all_settings,
            commands::test_copy,
            commands::library_path,
            commands::open_folder,
            commands::backup_library,
            commands::paper_sizes,
            commands::pen_presets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Vlt-writer");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_moves_to_the_new_name() {
        let base = std::env::temp_dir().join(format!("vlt-move-{}", std::process::id()));
        let (roaming, local) = (base.join("Roaming"), base.join("Local"));
        let old = roaming.join(OLD_ID).join("library");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("library.db"), b"x").unwrap();
        std::fs::create_dir_all(local.join(OLD_ID).join("EBWebView")).unwrap();
        let root = library_root(&roaming.join("uz.vlt-writer"), &local.join("uz.vlt-writer"));
        assert_eq!(root, roaming.join("uz.vlt-writer").join("library"));
        assert!(root.join("library.db").exists());
        assert!(!roaming.join(OLD_ID).exists() && !local.join(OLD_ID).exists());
        // and next time it is simply there
        assert_eq!(library_root(&roaming.join("uz.vlt-writer"), &local.join("uz.vlt-writer")), root);
        let _ = std::fs::remove_dir_all(&base);
    }
}
