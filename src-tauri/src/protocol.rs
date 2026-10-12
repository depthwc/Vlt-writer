//! `hw://` protocol (http://hw.localhost/... on Windows): images for the UI, rendered on
//! worker threads, never base64 over IPC.
//!
//!   /glyph/<hand>/<gid>.png          letter matte as dark ink on transparent
//!   /shot/<paper>/<shot>.jpg?w=      paper photo, scaled
//!   /thumb/<paper>/<shot>.jpg?w=     flattened paper with detected lines
//!   /preview/<project>/<paper>/<page>.jpg?w=&random=&seed=&shot=
//!   /svg/<project>/<page>.svg

use std::collections::HashMap;

use hw_core::layout::Geometry;
use hw_core::render::{self, RenderOpts};
use hw_core::{export::svg, io, paper};
use tauri::http::{Request, Response};
use tauri::{Manager, UriSchemeResponder};

use crate::commands::{project_layout, sheets, NOTEBOOK, RANDOM};
use crate::state::{err, AppState, Res};

fn query(q: Option<&str>) -> HashMap<String, String> {
    q.unwrap_or("")
        .split('&')
        .filter_map(|kv| {
            let mut it = kv.splitn(2, '=');
            Some((it.next()?.to_string(), it.next().unwrap_or("").to_string()))
        })
        .collect()
}

fn reply(r: Res<(Vec<u8>, &'static str)>) -> Response<Vec<u8>> {
    match r {
        Ok((body, mime)) => Response::builder()
            .header("Content-Type", mime)
            .header("Cache-Control", "no-store")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)
            .unwrap(),
        Err(e) => Response::builder().status(404).header("Content-Type", "text/plain").body(e.into_bytes()).unwrap(),
    }
}

fn handle(s: &AppState, path: &str, q: &HashMap<String, String>) -> Res<(Vec<u8>, &'static str)> {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let num = |k: &str, d: u32| q.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    match parts.as_slice() {
        ["glyph", hand, file] => {
            let gid = file.trim_end_matches(".png");
            let h = s.hand(hand)?;
            let a = h.read().alpha(gid);
            let mut img = image::RgbaImage::new(a.w as u32, a.h as u32);
            for (i, p) in img.pixels_mut().enumerate() {
                *p = image::Rgba([28, 30, 44, (a.d[i].clamp(0.0, 1.0) * 255.0) as u8]);
            }
            Ok((io::png(&image::DynamicImage::ImageRgba8(img)), "image/png"))
        }
        ["shot", pid, file] => {
            let p = s.lib.load_paper(pid).map_err(err)?;
            let shot: usize = file.trim_end_matches(".jpg").parse().unwrap_or(0);
            let img = s.photo(&p, shot, num("w", 1200))?;
            Ok((io::rgb_jpeg(&img, 85), "image/jpeg"))
        }
        ["thumb", pid, file] => {
            let p = s.lib.load_paper(pid).map_err(err)?;
            let shot: usize = file.trim_end_matches(".jpg").parse().unwrap_or(0);
            let photo = s.photo(&p, shot, 1600)?;
            let k = photo.w as f64 / p.shots[shot].w as f64;
            let mut sh = p.shots[shot].clone();
            sh.corners = sh.corners.map(|c| [c[0] * k, c[1] * k]);
            let lines = if num("lines", 1) == 1 { p.writing_lines(shot) } else { vec![] };
            let img = paper::analysis_thumb(&photo, &sh, &p.size, &lines, num("w", 320) as usize);
            Ok((io::jpeg(&img, 85), "image/jpeg"))
        }
        ["preview", project, pid, file] => {
            let page: usize = file.trim_end_matches(".jpg").parse().unwrap_or(0);
            let pr = s.lib.load_project(project).map_err(err)?;
            // the paper of this page: one paper, or a notebook's left or right page
            let sh = sheets(s, &pr, Some(pid))?;
            let res = project_layout(s, &pr, &sh)?;
            let (pm, mut pl) = sh.get(page % sh.len().max(1)).cloned().ok_or("choose a paper")?;
            if *pid != NOTEBOOK && *pid != RANDOM {
                if let Some(k) = q.get("shot").and_then(|v| v.parse::<usize>().ok()) {
                    if Some(k) != pl.shot {
                        pl.mesh = None;
                    }
                    pl.shot = Some(k);
                }
            }
            let shot = pl.shot.unwrap_or(0).min(pm.shots.len() - 1);
            let w = num("w", 1100);
            let photo = s.photo(&pm, shot, w)?;
            let scale = photo.w as f32 / pm.shots[shot].w as f32;
            let hand = s.hand(pr.hand.as_ref().ok_or("choose a hand")?)?;
            let h = hand.read();
            let empty = vec![];
            let elems = res.pages.get(page).unwrap_or(&empty);
            let opts = RenderOpts { scale, random: num("random", 0) == 1, seed: num("seed", 1) as u64, mess: pr.params.mess, pen: pr.pen, shot: pr.shot };
            let img = render::render(&pm, &photo, &h, elems, &pl, &opts);
            Ok((io::rgb_jpeg(&img, 88), "image/jpeg"))
        }
        ["svg", project, file] => {
            let page: usize = file.trim_end_matches(".svg").parse().unwrap_or(0);
            let pr = s.lib.load_project(project).map_err(err)?;
            let res = project_layout(s, &pr, &[])?;
            let hand = s.hand(pr.hand.as_ref().ok_or("choose a hand")?)?;
            let h = hand.read();
            let empty = vec![];
            let ink = pr.pen.color.unwrap_or(h.meta.ink);
            let svg = svg::page_svg(&h, res.pages.get(page).unwrap_or(&empty), &Geometry::default(), ink, false);
            Ok((svg.into_bytes(), "image/svg+xml"))
        }
        _ => Err(format!("unknown path {path}")),
    }
}

pub fn handler<R: tauri::Runtime>(ctx: tauri::UriSchemeContext<'_, R>, req: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = ctx.app_handle().clone();
    let path = req.uri().path().to_string();
    let q = query(req.uri().query());
    tauri::async_runtime::spawn_blocking(move || {
        let s = app.state::<AppState>();
        responder.respond(reply(handle(&s, &path, &q)));
    });
}
