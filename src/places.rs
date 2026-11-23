//! Places you can fast-travel to, and the island map drawn for the
//! in-game map screen.

use crate::meshgen::spawn_point;
use crate::worldgen::island::{pol, REGIONS};
use crate::worldgen::terrain::Terrain;

/// A named place on the map.
#[derive(Clone, Debug)]
pub struct Place {
    pub name: String,
    pub sub: String,
    pub x: f64,
    pub z: f64,
}

/// The places list: the start, each region, and the two giants' summits.
pub fn places(t: &Terrain) -> Vec<Place> {
    let mut out = Vec::new();
    let (sx, sz) = spawn_point();
    out.push(Place { name: "Start".into(), sub: "Southwest grasslands, facing the peaks".into(), x: sx, z: sz });
    for (k, g) in REGIONS.iter().enumerate() {
        if let Some((x, z)) = region_spot(t, k, g.centre) {
            out.push(Place { name: g.name.into(), sub: g.desc.into(), x, z });
        }
    }
    out.push(Place { name: "Mount Everest".into(), sub: "The summit".into(), x: t.everest.0, z: t.everest.1 });
    let (hx, hz) = helens_rim(t);
    out.push(Place { name: "Mount St. Helens".into(), sub: "The crater rim".into(), x: hx, z: hz });
    let (bx, bz) = t.bridge.from_deck(0.0, 0.0);
    out.push(Place { name: "The Bridge City".into(), sub: "Midway across the Great Canyon".into(), x: bx, z: bz });
    out
}

/// A walkable spot well inside a region: along its centre line, on dry
/// land that belongs to it, not too high or steep.
fn region_spot(t: &Terrain, k: usize, centre_deg: f64) -> Option<(f64, f64)> {
    let mut fallback = None;
    for step in 0..40 {
        // from the middle of the region outward, then inward
        let r = if step % 2 == 0 { 210.0 + step as f64 * 4.0 } else { 210.0 - step as f64 * 4.0 };
        for off in [0.0, 6.0, -6.0, 12.0, -12.0] {
            let (mx, my) = pol(centre_deg + off, r);
            let (x, z) = crate::worldgen::terrain::to_world(mx, my);
            let s = t.sample(x, z);
            if s.water.is_some() || s.h < 5.0 || s.glow > 0.0 || s.region as usize != k || s.shore < 20_000.0 {
                continue;
            }
            let slope = (t.height(x + 20.0, z) - t.height(x - 20.0, z)).abs() + (t.height(x, z + 20.0) - t.height(x, z - 20.0)).abs();
            if s.h < 3_000.0 && slope < 12.0 {
                return Some((x, z));
            }
            fallback.get_or_insert((x, z));
        }
    }
    fallback
}

/// The highest point of St. Helens' crater rim.
fn helens_rim(t: &Terrain) -> (f64, f64) {
    let (cx, cz) = t.helens;
    let mut best = (f64::MIN, cx, cz);
    for a in 0..72 {
        let ang = a as f64 / 72.0 * std::f64::consts::TAU;
        for k in 0..12 {
            let d = 4_000.0 + k as f64 * 1_500.0;
            let (x, z) = (cx + ang.cos() * d, cz + ang.sin() * d);
            let h = t.height(x, z);
            if h > best.0 {
                best = (h, x, z);
            }
        }
    }
    (best.1, best.2)
}

/// The island map for the map screen: `size` x `size` pixels covering
/// `half` metres either side of the centre, drawn a few rows at a time.
pub struct MapImage {
    pub size: usize,
    pub half: f64,
    pub rgba: Vec<u8>,
    rows_done: usize,
    prev: Vec<f64>,
}

impl MapImage {
    pub fn new(size: usize, half: f64) -> MapImage {
        MapImage { size, half, rgba: vec![0; size * size * 4], rows_done: 0, prev: vec![0.0; size] }
    }

    pub fn done(&self) -> bool {
        self.rows_done >= self.size
    }

    /// World position of a pixel's centre.
    pub fn world(&self, i: usize, j: usize) -> (f64, f64) {
        let step = 2.0 * self.half / self.size as f64;
        (-self.half + (i as f64 + 0.5) * step, -self.half + (j as f64 + 0.5) * step)
    }

    /// Draw up to `rows` more rows.
    pub fn build_rows(&mut self, t: &Terrain, rows: usize) {
        let n = self.size;
        let step = 2.0 * self.half / n as f64;
        let end = (self.rows_done + rows).min(n);
        for j in self.rows_done..end {
            let mut left = 0.0;
            for i in 0..n {
                let (x, z) = self.world(i, j);
                let s = t.sample(x, z);
                // the canyon keeps its depth so its walls are shaded
                let h = if s.canyon { s.h } else { s.h.max(0.0) };
                let (hl, hu) = (if i == 0 { h } else { left }, if j == 0 { h } else { self.prev[i] });
                left = h;
                self.prev[i] = h;
                let mut c = s.col;
                if let Some(level) = s.water {
                    // water: deeper is darker
                    let depth = ((level - s.h) / 1500.0).clamp(0.0, 1.0) as f32;
                    c = [0.16 - 0.08 * depth, 0.38 - 0.16 * depth, 0.55 - 0.15 * depth];
                } else {
                    // light from the north-west; the scale keeps the giants readable
                    let slope = ((h - hl) + (h - hu)) / step;
                    let mut shade = (1.0 + slope.clamp(-4.0, 4.0) * 0.45).clamp(0.5, 1.45) as f32;
                    if s.canyon {
                        shade *= 1.0 - 0.45 * (-s.h / 45_000.0).clamp(0.0, 1.0) as f32;
                    }
                    c = [c[0] * shade, c[1] * shade, c[2] * shade];
                }
                let o = (j * n + i) * 4;
                for k in 0..3 {
                    self.rgba[o + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
                }
                self.rgba[o + 3] = 255;
            }
        }
        self.rows_done = end;
        if self.done() {
            self.overlay(t);
        }
    }

    /// Lines too fine to show up pixel by pixel: the rivers and the bridge.
    fn overlay(&mut self, t: &Terrain) {
        for rv in &t.rivers {
            let p = &rv.path;
            for k in 1..p.pts.len() {
                if p.s[k] > p.coast_s {
                    break;
                }
                self.line(p.pts[k - 1], p.pts[k], [0.22, 0.46, 0.66], 0.85);
            }
        }
        let [a, b] = t.bridge.ends();
        self.line(a, b, [0.95, 0.92, 0.84], 1.0);
    }

    fn line(&mut self, a: (f64, f64), b: (f64, f64), c: [f32; 3], alpha: f32) {
        let step = 2.0 * self.half / self.size as f64;
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / (step * 0.4)).ceil().max(1.0) as usize;
        for i in 0..=n {
            let f = i as f64 / n as f64;
            let (x, z) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
            let (pi, pj) = (((x + self.half) / step).floor(), ((z + self.half) / step).floor());
            if pi < 0.0 || pj < 0.0 || pi >= self.size as f64 || pj >= self.size as f64 {
                continue;
            }
            let o = (pj as usize * self.size + pi as usize) * 4;
            for k in 0..3 {
                let old = self.rgba[o + k] as f32 / 255.0;
                self.rgba[o + k] = ((old + (c[k] - old) * alpha).clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_region_has_a_place_on_dry_land() {
        let t = Terrain::new();
        let ps = places(&t);
        assert_eq!(ps.len(), 1 + REGIONS.len() + 3, "{:?}", ps.iter().map(|p| &p.name).collect::<Vec<_>>());
        for p in &ps[1..1 + REGIONS.len()] {
            let s = t.sample(p.x, p.z);
            assert!(s.water.is_none() && s.h > 0.0, "{} at {:.0},{:.0}", p.name, p.x, p.z);
        }
    }

    #[test]
    fn map_draws_all_rows() {
        let t = Terrain::new();
        let mut m = MapImage::new(16, 1_750_000.0);
        while !m.done() {
            m.build_rows(&t, 5);
        }
        assert!(m.rgba.chunks(4).all(|p| p[3] == 255));
    }
}
