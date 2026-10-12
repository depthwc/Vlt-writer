//! 1-D helpers: Gaussian smoothing, peak finding, robust statistics, polynomial fit.

pub fn gauss1d(s: &[f32], sigma: f32) -> Vec<f32> {
    if sigma <= 0.05 || s.is_empty() {
        return s.to_vec();
    }
    let r = (3.0 * sigma).ceil() as isize;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let n = s.len() as isize;
    (0..n)
        .map(|i| {
            let mut a = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let t = (i + j as isize - r).clamp(0, n - 1) as usize;
                a += kv * s[t];
            }
            a / ks
        })
        .collect()
}

/// Local maxima with minimum distance and prominence (like scipy.signal.find_peaks).
/// Returns (index, prominence), sorted by index.
pub fn find_peaks(s: &[f32], distance: usize, min_prominence: f32) -> Vec<(usize, f32)> {
    let n = s.len();
    let mut cands: Vec<usize> = (1..n.saturating_sub(1)).filter(|&i| s[i] > s[i - 1] && s[i] >= s[i + 1]).collect();
    // prominence: height above the higher of the two minima down to the nearest higher peak
    let prom = |i: usize| -> f32 {
        let h = s[i];
        let mut lmin = h;
        let mut j = i;
        while j > 0 {
            j -= 1;
            if s[j] > h {
                break;
            }
            lmin = lmin.min(s[j]);
        }
        let mut rmin = h;
        let mut j = i;
        while j + 1 < n {
            j += 1;
            if s[j] > h {
                break;
            }
            rmin = rmin.min(s[j]);
        }
        h - lmin.max(rmin)
    };
    let mut withp: Vec<(usize, f32)> = cands.drain(..).map(|i| (i, prom(i))).filter(|&(_, p)| p >= min_prominence).collect();
    // enforce distance: keep higher peaks first
    withp.sort_by(|a, b| s[b.0].partial_cmp(&s[a.0]).unwrap());
    let mut kept: Vec<(usize, f32)> = Vec::new();
    for (i, p) in withp {
        if kept.iter().all(|&(k, _)| (k as isize - i as isize).unsigned_abs() >= distance) {
            kept.push((i, p));
        }
    }
    kept.sort_by_key(|k| k.0);
    kept
}

pub fn median(v: &[f32]) -> f32 {
    percentile(v, 50.0)
}

pub fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s: Vec<f32> = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = (p / 100.0).clamp(0.0, 1.0) * (s.len() - 1) as f32;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    s[lo] + (s[hi] - s[lo]) * (pos - lo as f32)
}

/// Median absolute deviation scaled to a Gaussian sigma.
pub fn mad_sigma(v: &[f32]) -> f32 {
    let m = median(v);
    let dev: Vec<f32> = v.iter().map(|x| (x - m).abs()).collect();
    1.4826 * median(&dev)
}

pub fn mean(v: &[f32]) -> f32 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 }
}

pub fn std(v: &[f32]) -> f32 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
}

/// Least-squares polynomial fit, coefficients highest degree first (like numpy.polyfit).
pub fn polyfit(x: &[f64], y: &[f64], deg: usize) -> Vec<f64> {
    let n = x.len();
    let m = deg + 1;
    let a = nalgebra::DMatrix::from_fn(n, m, |i, j| x[i].powi((deg - j) as i32));
    let b = nalgebra::DVector::from_column_slice(y);
    let svd = a.svd(true, true);
    match svd.solve(&b, 1e-12) {
        Ok(c) => c.iter().copied().collect(),
        Err(_) => vec![0.0; m],
    }
}

pub fn polyval(c: &[f64], x: f64) -> f64 {
    c.iter().fold(0.0, |acc, &v| acc * x + v)
}

/// Linear interpolation of y(x) over sorted xs (clamped at the ends).
pub fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    if x <= xs[0] {
        return ys[0];
    }
    if x >= xs[xs.len() - 1] {
        return ys[ys.len() - 1];
    }
    let i = xs.partition_point(|&v| v <= x).max(1) - 1;
    let t = (x - xs[i]) / (xs[i + 1] - xs[i]).max(1e-12);
    ys[i] + t * (ys[i + 1] - ys[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_and_fit() {
        let s: Vec<f32> = (0..200).map(|i| ((i as f32) / 10.0 * std::f32::consts::PI).sin().max(0.0)).collect();
        let p = find_peaks(&s, 5, 0.5);
        assert_eq!(p.len(), 10);
        let x: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| 2.0 * v * v - 3.0 * v + 1.0).collect();
        let c = polyfit(&x, &y, 2);
        assert!((c[0] - 2.0).abs() < 1e-6 && (c[1] + 3.0).abs() < 1e-6 && (c[2] - 1.0).abs() < 1e-6);
        assert!((percentile(&[1.0, 2.0, 3.0, 4.0], 50.0) - 2.5).abs() < 1e-6);
    }
}
