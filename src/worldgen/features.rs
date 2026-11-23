//! Landforms and landmarks placed by design (first pass, from the
//! designer's picks after a map review): two tablelands that link regions,
//! oases in the Northeast's dune sea, the Needles off the South coast, the
//! Terrace Lakes stepping down in waterfalls in the Northwest, and a ruined
//! outpost on the Bridge City's Southeast approach. All in world metres.

use super::noise::{sm, Noise};

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// A raised tableland: ground lower than its top is lifted to it, with an
/// escarpment at the edge.
pub struct Tableland {
    pub x: f64,
    pub z: f64,
    pub r: f64,
    pub top: f64,
    pub seed: f64,
}

impl Tableland {
    pub fn lift(&self, n: &Noise, wx: f64, wz: f64, h: f64) -> f64 {
        let (dx, dz) = (wx - self.x, wz - self.z);
        let d = dx.hypot(dz);
        if d > self.r * 1.25 {
            return h;
        }
        let a = dz.atan2(dx);
        let edge = self.r * (1.0 + 0.16 * n.n2(a.cos() * 1.7 + self.seed, a.sin() * 1.7 - self.seed));
        let w = sm(edge, edge - 4_000.0, d);
        if w <= 0.0 {
            return h;
        }
        let top = self.top + 60.0 * n.fbm(wx / 7_000.0 + self.seed, wz / 7_000.0, 3);
        if top > h {
            h + (top - h) * w
        } else {
            h
        }
    }

    /// How much of the tableland's flat top a spot is on (0–1).
    pub fn on_top(&self, wx: f64, wz: f64) -> f64 {
        sm(self.r * 1.1, self.r * 0.85, (wx - self.x).hypot(wz - self.z))
    }
}

/// An oasis in the dune sea: a hollow with a pool and green around it.
pub struct Oasis {
    pub x: f64,
    pub z: f64,
    /// Radius of the pool.
    pub r: f64,
    /// The hollow's floor, a little above the water.
    pub floor: f64,
}

impl Oasis {
    const BASIN: f64 = 3.4;

    pub fn level(&self) -> f64 {
        self.floor - 1.5
    }

    fn dist(&self, wx: f64, wz: f64) -> f64 {
        (wx - self.x).hypot(wz - self.z)
    }

    pub fn shape(&self, wx: f64, wz: f64, h: f64) -> f64 {
        let d = self.dist(wx, wz);
        let rb = self.r * Self::BASIN;
        if d > rb * 1.5 {
            return h;
        }
        let mut h = lerp(h, self.floor, sm(rb * 1.5, rb * 0.55, d));
        let lv = self.level();
        if d < self.r * 1.15 {
            let shore = lerp(lv + 0.2, self.floor, sm(self.r, self.r * 1.15, d));
            h = h.min(shore);
        }
        if d < self.r {
            h = h.min(lv - 0.5 - 6.0 * (1.0 - (d / self.r).powi(2)));
        }
        h
    }

    /// Palms, grass and reeds around the water (0–1).
    pub fn green(&self, wx: f64, wz: f64) -> f64 {
        sm(self.r * 2.8, self.r * 1.25, self.dist(wx, wz))
    }

    pub fn water(&self, wx: f64, wz: f64) -> Option<f64> {
        (self.dist(wx, wz) < self.r).then(|| self.level())
    }
}

/// One rock spire of the Needles.
pub struct Needle {
    pub x: f64,
    pub z: f64,
    pub r: f64,
    pub top: f64,
}

/// The tallest spire top at a spot, if any (fluted, sheer-sided columns).
pub fn needle_height(needles: &[Needle], wx: f64, wz: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for nd in needles {
        let (dx, dz) = (wx - nd.x, wz - nd.z);
        let d = dx.hypot(dz);
        if d > nd.r * 1.3 {
            continue;
        }
        let a = dz.atan2(dx);
        let re = nd.r * (1.0 + 0.14 * (6.0 * a + nd.top * 0.01).sin() + 0.06 * (13.0 * a).sin());
        let w = sm(re, re * 0.62, d);
        if w > 0.0 {
            let top = nd.top * (0.93 + 0.07 * (1.0 - d / re).max(0.0)) * w;
            best = Some(best.map_or(top, |b: f64| b.max(top)));
        }
    }
    best
}

/// A lake of the Terrace Lakes.
#[derive(Clone, Copy, Debug)]
pub struct TerraceLake {
    pub x: f64,
    pub z: f64,
    pub r: f64,
    pub level: f64,
    /// Radius of the step it sits on (its lip faces the next lake).
    pub step: f64,
}

/// A waterfall from one terrace lake's outflow down to the next lake.
#[derive(Clone, Copy, Debug)]
pub struct Fall {
    /// The lip, and the unit direction the water falls toward.
    pub lip: (f64, f64),
    pub dir: (f64, f64),
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
}

/// Lakes stepping down a broad shoulder of land, each on its own step,
/// joined by short outflow streams that pour over the step's lip into the
/// next lake as a waterfall.
pub struct Terraces {
    pub lakes: Vec<TerraceLake>,
    pub falls: Vec<Fall>,
    pub start: (f64, f64),
    pub dir: (f64, f64),
    pub len: f64,
}

/// From a lake's edge to its step's lip (where the outflow stream runs).
const MARGIN: f64 = 700.0;
const CHANNEL_W: f64 = 38.0;

impl Terraces {
    pub fn new(start: (f64, f64), dir: (f64, f64)) -> Terraces {
        let radii = [1_400.0, 1_900.0, 1_600.0, 2_300.0];
        let levels = [205.0, 152.0, 103.0, 58.0];
        let mut lakes = Vec::new();
        let mut falls = Vec::new();
        let mut c = start;
        for k in 0..4 {
            lakes.push(TerraceLake { x: c.0, z: c.1, r: radii[k], level: levels[k], step: radii[k] + MARGIN });
            if k < 3 {
                let lip = (c.0 + dir.0 * (radii[k] + MARGIN), c.1 + dir.1 * (radii[k] + MARGIN));
                falls.push(Fall { lip, dir, width: 70.0 + 18.0 * k as f64, top: levels[k], bottom: levels[k + 1] });
                let step = radii[k] + MARGIN + radii[k + 1];
                c = (c.0 + dir.0 * step, c.1 + dir.1 * step);
            }
        }
        let last = lakes[3];
        let len = (last.x - start.0) * dir.0 + (last.z - start.1) * dir.1;
        Terraces { lakes, falls, start, dir, len }
    }

    /// Along and across the chain from its first lake.
    fn local(&self, wx: f64, wz: f64) -> (f64, f64) {
        let (dx, dz) = (wx - self.start.0, wz - self.start.1);
        (dx * self.dir.0 + dz * self.dir.1, -dx * self.dir.1 + dz * self.dir.0)
    }

    /// Near enough to change anything.
    pub fn near(&self, wx: f64, wz: f64) -> bool {
        let (u, v) = self.local(wx, wz);
        u > -8_000.0 && u < self.len + 8_000.0 && v.abs() < 10_000.0
    }

    /// The broad shoulder the steps sit on, falling from the top lake to
    /// the bottom one (always below the steps).
    fn ramp(&self, u: f64, v: f64) -> (f64, f64) {
        let t = ((u + 3_000.0) / (self.len + 6_000.0)).clamp(0.0, 1.0);
        let top = lerp(self.lakes[0].level - 70.0, self.lakes[3].level - 40.0, t);
        let w = sm(8_000.0, 3_500.0, v.abs()) * sm(-7_000.0, -2_500.0, u) * (1.0 - sm(self.len + 2_500.0, self.len + 7_000.0, u));
        (top, w)
    }

    /// The ground with the shoulder, the steps, the lakes and the outflow
    /// streams.
    pub fn shape(&self, n: &Noise, wx: f64, wz: f64, h: f64) -> f64 {
        if !self.near(wx, wz) {
            return h;
        }
        let (u, v) = self.local(wx, wz);
        let (rt, rw) = self.ramp(u, v);
        let mut best = if rt > h { h + (rt - h) * rw } else { h };
        let base = best;
        for (k, lk) in self.lakes.iter().enumerate() {
            let (dx, dz) = (wx - lk.x, wz - lk.z);
            let d = dx.hypot(dz);
            if d > lk.step * 1.25 {
                continue;
            }
            // the lip toward the next lake is sheer; elsewhere the step's
            // edge is a rocky slope that wanders a little
            let toward = if k < 3 { ((dx * self.dir.0 + dz * self.dir.1) / d.max(1.0)).max(0.0).powi(6) } else { 0.0 };
            let a = dz.atan2(dx);
            let edge_r = lk.step * (1.0 + 0.1 * (1.0 - toward) * n.n2(a.cos() * 1.3 + k as f64 * 5.0, a.sin() * 1.3));
            let slope = lerp(240.0, 16.0, toward);
            let top = lk.level + 6.0 + 2.0 * n.n2(wx / 400.0, wz / 400.0 + k as f64);
            let w = sm(edge_r, edge_r - slope, d);
            if top > base {
                best = best.max(base + (top - base) * w);
            }
        }
        // the lakes' beds and the outflow streams to the lips
        for (k, lk) in self.lakes.iter().enumerate() {
            if self.behind_lip(k, wx, wz) {
                continue;
            }
            let d = (wx - lk.x).hypot(wz - lk.z);
            if d < lk.r * 1.15 {
                let shore = lerp(lk.level + 0.2, best, sm(lk.r, lk.r * 1.15, d));
                best = best.min(shore);
            }
            if d < lk.r {
                best = best.min(lk.level - 0.6 - 9.0 * (1.0 - (d / lk.r).powi(2)));
            }
            if k < 3 {
                if let Some(bed) = self.channel_bed(k, wx, wz) {
                    best = best.min(bed);
                }
            }
        }
        best
    }

    /// On the upper side of the lip that pours into lake `k` (so on the step
    /// above it, which the lake must not cut into).
    fn behind_lip(&self, k: usize, wx: f64, wz: f64) -> bool {
        k > 0 && {
            let f = &self.falls[k - 1];
            (wx - f.lip.0) * self.dir.0 + (wz - f.lip.1) * self.dir.1 < 0.0
        }
    }

    /// The bed of lake `k`'s outflow stream at a spot, if it is in it.
    fn channel_bed(&self, k: usize, wx: f64, wz: f64) -> Option<f64> {
        let lk = &self.lakes[k];
        let (dx, dz) = (wx - lk.x, wz - lk.z);
        let along = dx * self.dir.0 + dz * self.dir.1;
        let across = (-dx * self.dir.1 + dz * self.dir.0).abs();
        let wob = 14.0 * ((along / 90.0).sin());
        let across = (across - wob).abs();
        (along > lk.r * 0.7 && along < lk.step + 3.0 && across < CHANNEL_W * 1.4).then(|| {
            let t = (across / (CHANNEL_W * 1.4)).min(1.0);
            lk.level - 1.6 * (1.0 - t * t) + 2.0 * sm(0.75, 1.0, t)
        })
    }

    /// The water surface at a spot (a lake, or an outflow stream).
    pub fn water(&self, wx: f64, wz: f64) -> Option<f64> {
        if !self.near(wx, wz) {
            return None;
        }
        for (k, lk) in self.lakes.iter().enumerate() {
            if (wx - lk.x).hypot(wz - lk.z) < lk.r * 1.02 && !self.behind_lip(k, wx, wz) {
                return Some(lk.level);
            }
            if k < 3 && self.channel_bed(k, wx, wz).is_some_and(|b| b < lk.level) {
                return Some(lk.level);
            }
        }
        None
    }

    /// How rocky a spot is: the steps' edges and the cliffs under the falls.
    pub fn rocky(&self, wx: f64, wz: f64) -> f64 {
        if !self.near(wx, wz) {
            return 0.0;
        }
        let mut r: f64 = 0.0;
        for lk in &self.lakes {
            let d = (wx - lk.x).hypot(wz - lk.z);
            r = r.max(sm(lk.step - 320.0, lk.step - 60.0, d) * sm(lk.step + 120.0, lk.step, d));
        }
        r
    }
}

/// The ruined outpost: a square fort, long abandoned, beside the approach
/// to the bridge.
#[derive(Clone, Copy, Debug)]
pub struct Outpost {
    pub x: f64,
    pub z: f64,
    /// The level of the ground it stands on.
    pub y: f64,
    /// Unit direction its gate faces (toward the bridge).
    pub facing: (f64, f64),
}

impl Outpost {
    pub const PAD: f64 = 120.0;

    /// The ground levelled for the fort.
    pub fn pad(&self, wx: f64, wz: f64, h: f64) -> f64 {
        if self.y.is_nan() {
            return h;
        }
        let d = (wx - self.x).hypot(wz - self.z);
        lerp(h, self.y, sm(Self::PAD * 2.4, Self::PAD, d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::noise::Rng;

    #[test]
    fn terraces_step_down_into_each_other() {
        let mut rng = Rng::new(3);
        let n = Noise::new(&mut rng);
        let t = Terraces::new((0.0, 0.0), (0.8, 0.6));
        for (k, f) in t.falls.iter().enumerate() {
            // just above the lip: the upper step; just past it: the next lake
            let before = (f.lip.0 - f.dir.0 * 60.0 - f.dir.1 * 200.0, f.lip.1 - f.dir.1 * 60.0 + f.dir.0 * 200.0);
            let past = (f.lip.0 + f.dir.0 * 40.0, f.lip.1 + f.dir.1 * 40.0);
            let hb = t.shape(&n, before.0, before.1, 25.0);
            let hp = t.shape(&n, past.0, past.1, 25.0);
            assert!(hb > t.lakes[k].level + 2.0, "step {k}: {hb}");
            assert!(hp < t.lakes[k + 1].level, "below fall {k}: {hp}");
            assert_eq!(t.water(past.0, past.1), Some(t.lakes[k + 1].level));
            // the outflow stream reaches the lip
            let ch = (f.lip.0 - f.dir.0 * 20.0, f.lip.1 - f.dir.1 * 20.0);
            assert_eq!(t.water(ch.0, ch.1), Some(f.top), "stream {k}");
        }
        // each lake holds water
        for lk in &t.lakes {
            assert!(t.shape(&n, lk.x, lk.z, 25.0) < lk.level - 5.0);
        }
    }
}
