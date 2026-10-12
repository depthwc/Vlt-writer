//! Thinning ink to 1-px centre lines and tracing them into ordered polylines (pen paths).

use std::collections::{HashMap, HashSet};

/// Zhang-Suen thinning of a binary mask.
pub fn thin(mask: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut m = mask.to_vec();
    if w < 3 || h < 3 {
        return m;
    }
    let idx = |x: usize, y: usize| y * w + x;
    loop {
        let mut changed = false;
        for pass in 0..2 {
            let mut kill = Vec::new();
            for y in 1..h - 1 {
                for x in 1..w - 1 {
                    if !m[idx(x, y)] {
                        continue;
                    }
                    let p = [
                        m[idx(x, y - 1)], m[idx(x + 1, y - 1)], m[idx(x + 1, y)], m[idx(x + 1, y + 1)],
                        m[idx(x, y + 1)], m[idx(x - 1, y + 1)], m[idx(x - 1, y)], m[idx(x - 1, y - 1)],
                    ];
                    let b = p.iter().filter(|&&v| v).count();
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let a = (0..8).filter(|&i| !p[i] && p[(i + 1) % 8]).count();
                    if a != 1 {
                        continue;
                    }
                    let (p2, p4, p6, p8) = (p[0], p[2], p[4], p[6]);
                    let ok = if pass == 0 { !(p2 && p4 && p6) && !(p4 && p6 && p8) } else { !(p2 && p4 && p8) && !(p2 && p6 && p8) };
                    if ok {
                        kill.push(idx(x, y));
                    }
                }
            }
            if !kill.is_empty() {
                changed = true;
                for i in kill {
                    m[i] = false;
                }
            }
        }
        if !changed {
            return m;
        }
    }
}

/// Ordered polylines from a 1-px skeleton: paths between endpoints/junctions, then loops.
pub fn trace(sk: &[bool], w: usize, h: usize) -> Vec<Vec<[f32; 2]>> {
    let pts: HashSet<(i32, i32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| sk[y * w + x]).map(|(x, y)| (x as i32, y as i32)).collect();
    const NB: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];
    let neigh = |p: (i32, i32)| -> Vec<(i32, i32)> { NB.iter().map(|d| (p.0 + d.0, p.1 + d.1)).filter(|q| pts.contains(q)).collect() };
    let deg: HashMap<(i32, i32), usize> = pts.iter().map(|&p| (p, neigh(p).len())).collect();
    let key = |a: (i32, i32), b: (i32, i32)| if a < b { (a, b) } else { (b, a) };
    let mut used: HashSet<((i32, i32), (i32, i32))> = HashSet::new();
    let mut paths: Vec<Vec<(i32, i32)>> = Vec::new();
    let walk = |start: (i32, i32), next: (i32, i32), used: &mut HashSet<_>| -> Vec<(i32, i32)> {
        let mut path = vec![start, next];
        used.insert(key(start, next));
        let (mut prev, mut cur) = (start, next);
        while deg[&cur] == 2 {
            let cand: Vec<_> = neigh(cur).into_iter().filter(|&q| q != prev && !used.contains(&key(cur, q))).collect();
            let Some(&n) = cand.first() else { break };
            used.insert(key(cur, n));
            prev = cur;
            cur = n;
            path.push(cur);
        }
        path
    };
    let mut starts: Vec<(i32, i32)> = pts.iter().copied().filter(|p| deg[p] == 1).collect();
    starts.extend(pts.iter().copied().filter(|p| deg[p] > 2));
    starts.sort();
    for s in starts {
        for q in neigh(s) {
            if !used.contains(&key(s, q)) {
                paths.push(walk(s, q, &mut used));
            }
        }
    }
    let mut rest: Vec<(i32, i32)> = pts.iter().copied().collect();
    rest.sort();
    for p in rest {
        for q in neigh(p) {
            if !used.contains(&key(p, q)) {
                paths.push(walk(p, q, &mut used));
            }
        }
    }
    paths
        .into_iter()
        .filter(|p| p.len() >= 3)
        .map(|p| simplify(&p.iter().map(|&(x, y)| [x as f32, y as f32]).collect::<Vec<_>>(), 0.8))
        .collect()
}

/// Ramer-Douglas-Peucker.
pub fn simplify(pts: &[[f32; 2]], eps: f32) -> Vec<[f32; 2]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (mut imax, mut dmax) = (0, 0.0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len;
        if d > dmax {
            dmax = d;
            imax = i;
        }
    }
    if dmax > eps {
        let mut l = simplify(&pts[..=imax], eps);
        let r = simplify(&pts[imax..], eps);
        l.pop();
        l.extend(r);
        l
    } else {
        vec![a, b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_bar_to_line() {
        let (w, h) = (40, 12);
        let mut m = vec![false; w * h];
        for y in 4..8 {
            for x in 5..35 {
                m[y * w + x] = true;
            }
        }
        let t = thin(&m, w, h);
        assert!(t.iter().filter(|&&v| v).count() < 40);
        let p = trace(&t, w, h);
        assert_eq!(p.len(), 1);
        assert!(p[0].len() >= 2);
    }
}
