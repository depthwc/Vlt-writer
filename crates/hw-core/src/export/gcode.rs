//! G-code for pen plotters (GRBL-style), from the page's centre-line paths.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// command to lift the pen, e.g. "M3 S0" (servo) or "G0 Z5" (Z axis)
    pub pen_up: String,
    pub pen_down: String,
    /// mm/min
    pub feed: f32,
    pub travel: f32,
    /// pause after pen moves, seconds
    pub dwell: f32,
    /// flip Y (plotter origin bottom-left)
    pub flip_y: bool,
    pub page_h: f32,
    pub offset: [f32; 2],
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            pen_up: "M3 S0".into(),
            pen_down: "M3 S90".into(),
            feed: 2000.0,
            travel: 4000.0,
            dwell: 0.15,
            flip_y: true,
            page_h: 297.0,
            offset: [0.0, 0.0],
        }
    }
}

/// Paths in page mm. Orders paths greedily by nearest start to cut pen-up travel.
pub fn to_gcode(paths: &[Vec<[f32; 2]>], p: &Profile) -> String {
    let mut left: Vec<&Vec<[f32; 2]>> = paths.iter().filter(|v| v.len() >= 2).collect();
    let mut out = vec!["G21 ; mm".to_string(), "G90 ; absolute".to_string(), p.pen_up.clone()];
    let tr = |q: [f32; 2]| -> [f32; 2] { [q[0] + p.offset[0], if p.flip_y { p.page_h - q[1] } else { q[1] } + p.offset[1]] };
    let mut pos = [0.0f32, 0.0];
    while !left.is_empty() {
        let (i, rev) = left
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let d0 = (v[0][0] - pos[0]).powi(2) + (v[0][1] - pos[1]).powi(2);
                let d1 = (v[v.len() - 1][0] - pos[0]).powi(2) + (v[v.len() - 1][1] - pos[1]).powi(2);
                if d1 < d0 { (i, true, d1) } else { (i, false, d0) }
            })
            .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
            .map(|(i, r, _)| (i, r))
            .unwrap();
        let path = left.swap_remove(i);
        let pts: Vec<[f32; 2]> = if rev { path.iter().rev().copied().collect() } else { path.clone() };
        let s = tr(pts[0]);
        out.push(format!("G0 X{:.2} Y{:.2} F{:.0}", s[0], s[1], p.travel));
        out.push(p.pen_down.clone());
        if p.dwell > 0.0 {
            out.push(format!("G4 P{:.2}", p.dwell));
        }
        for q in &pts[1..] {
            let t = tr(*q);
            out.push(format!("G1 X{:.2} Y{:.2} F{:.0}", t[0], t[1], p.feed));
        }
        out.push(p.pen_up.clone());
        if p.dwell > 0.0 {
            out.push(format!("G4 P{:.2}", p.dwell));
        }
        pos = *pts.last().unwrap();
    }
    out.push("G0 X0 Y0".into());
    out.join("\n") + "\n"
}
