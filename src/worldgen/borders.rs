//! What runs along the region borders (the designer's choices): the deep
//! canyon between the Northeast and Southeast with its natural sea wall and
//! the bridge city at its midway point, rivers between the lowland regions,
//! and mountain ranges between the highland ones.

use super::island::{pol, Island};
use super::noise::{sm, Noise};
use super::terrain::to_world;

/// Map pixels of radius between path points.
const PATH_DR: f64 = 1.5;

/// A line along a region border, from the central range out past the
/// coast (world metres), with the distance along it.
pub struct BorderPath {
    pub k: usize,
    pub pts: Vec<(f64, f64)>,
    pub s: Vec<f64>,
    r0: f64,
    /// Distance along the path where it reaches the coast.
    pub coast_s: f64,
}

impl BorderPath {
    pub fn new(isl: &Island, k: usize, r0: f64) -> BorderPath {
        Self::along(isl, k, r0, |r| isl.border_angle(k, r))
    }

    /// A path running outward from `r0` map pixels at the direction
    /// `angle(r)` (degrees). `k` tells paths apart (it seeds the meanders).
    pub fn along(isl: &Island, k: usize, r0: f64, angle: impl Fn(f64) -> f64) -> BorderPath {
        let (mut pts, mut s): (Vec<(f64, f64)>, Vec<f64>) = (Vec::new(), Vec::new());
        let (mut acc, mut coast_s) = (0.0, None::<f64>);
        let mut r = r0;
        loop {
            let (mx, my) = pol(angle(r), r);
            let w = to_world(mx, my);
            if let Some(&(lx, lz)) = pts.last() {
                acc += (w.0 - lx).hypot(w.1 - lz);
            }
            if coast_s.is_none() && isl.land_at(mx, my) <= 0.0 {
                coast_s = Some(acc);
            }
            pts.push(w);
            s.push(acc);
            if r > 560.0 || coast_s.is_some_and(|c| acc > c + 60_000.0) {
                break;
            }
            r += PATH_DR;
        }
        BorderPath { k, pts, s, r0, coast_s: coast_s.unwrap_or(acc) }
    }

    /// Distance from the path and distance along it, for a spot `r_px` map
    /// pixels from the centre. None when the spot is well before the start.
    pub fn nearest(&self, wx: f64, wz: f64, r_px: f64) -> Option<(f64, f64)> {
        if r_px < self.r0 - 12.0 {
            return None;
        }
        let n = self.pts.len() as i64;
        let k = ((r_px - self.r0) / PATH_DR).round() as i64;
        let lo = (k - 9).clamp(0, n - 2) as usize;
        let hi = (k + 9).clamp(0, n - 2) as usize;
        let mut best = (f64::MAX, 0.0);
        for i in lo..=hi {
            let (ax, az) = self.pts[i];
            let (bx, bz) = self.pts[i + 1];
            let (dx, dz) = (bx - ax, bz - az);
            let l2 = (dx * dx + dz * dz).max(1e-9);
            let t = (((wx - ax) * dx + (wz - az) * dz) / l2).clamp(0.0, 1.0);
            let d = (wx - ax - dx * t).hypot(wz - az - dz * t);
            if d < best.0 {
                best = (d, self.s[i] + t * (self.s[i + 1] - self.s[i]));
            }
        }
        Some(best)
    }

    /// The point and the direction (unit) at distance `s` along the path.
    pub fn at(&self, s: f64) -> ((f64, f64), (f64, f64)) {
        let i = self.s.partition_point(|&v| v < s).clamp(1, self.s.len() - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let len = (self.s[i] - self.s[i - 1]).max(1e-9);
        let t = ((s - self.s[i - 1]) / len).clamp(0.0, 1.0);
        let (dx, dz) = (b.0 - a.0, b.1 - a.1);
        let dl = dx.hypot(dz).max(1e-9);
        ((a.0 + dx * t, a.1 + dz * t), (dx / dl, dz / dl))
    }
}

/// The canyon between the Northeast and Southeast: Grand Canyon x25,
/// about 45 km deep and 60 km across, its floor far below the sea, which a
/// natural wall at the coast holds back. It narrows to a slot gorge at its
/// midway point, where the bridge city crosses.
pub struct Canyon {
    pub path: BorderPath,
    pub mid_s: f64,
}

pub const CANYON_DEPTH: f64 = 45_000.0;
pub const CANYON_HALF_W: f64 = 30_000.0;
/// Thickness of the natural wall between the canyon's end and the sea.
pub const SEA_WALL: f64 = 8_000.0;

impl Canyon {
    pub fn new(isl: &Island, k: usize) -> Canyon {
        let path = BorderPath::new(isl, k, 105.0);
        let mid_s = path.coast_s * 0.5;
        Canyon { path, mid_s }
    }

    /// Half-width at the rim.
    pub fn half_width(&self, s: f64) -> f64 {
        let narrow = 1.0 - 0.75 * (-((s - self.mid_s) / 70_000.0).powi(2)).exp();
        CANYON_HALF_W * narrow * (0.35 + 0.65 * sm(0.0, 80_000.0, s))
    }

    /// Depth below the rim, `d` metres from the canyon's line; zero beyond
    /// the natural wall's inner face, which curves round the canyon's end.
    pub fn depth(&self, s: f64, d: f64) -> f64 {
        let hw = self.half_width(s).max(1.0);
        let end = self.path.coast_s - SEA_WALL - 9_000.0 * (d / hw).min(1.2).powi(2);
        CANYON_DEPTH * sm(0.0, 140_000.0, s) * (1.0 - sm(end - 2_500.0, end, s))
    }

    /// How far the canyon cuts down at a spot `d` metres from its line and
    /// `s` along it: stepped walls (cliff, then bench, seven times) down to a
    /// narrow floor.
    pub fn carve(&self, n: &Noise, wx: f64, wz: f64, d: f64, s: f64) -> f64 {
        let depth = self.depth(s, d);
        if depth <= 0.0 {
            return 0.0;
        }
        let hw = self.half_width(s);
        if d > hw * 1.15 {
            return 0.0;
        }
        let fw = 0.12 * hw;
        let wob = 0.04 * n.n2(wx / 7_000.0 + 5.0, wz / 7_000.0);
        let u = ((d - fw) / (hw - fw) + wob).clamp(0.0, 1.0);
        let t = u * 7.0;
        let terr = (t.floor() + sm(0.0, 0.45, t.fract())) / 7.0;
        depth * (1.0 - terr)
    }

    /// The natural wall: the land between the canyon's end and the sea,
    /// raised into a rampart across the canyon's mouth.
    pub fn sea_wall(&self, d: f64, s: f64) -> f64 {
        let c = self.path.coast_s - SEA_WALL * 0.55;
        let across = sm(CANYON_HALF_W * 1.4, CANYON_HALF_W * 0.9, d);
        1_400.0 * (-((s - c) / 3_500.0).powi(2)).exp() * across
    }
}

/// A river along a region border, from the central range to the sea.
pub struct River {
    pub path: BorderPath,
    /// Water level at each path point (falls toward the sea).
    pub levels: Vec<f64>,
    /// Size: 1 for the border rivers, smaller for streams.
    pub scale: f64,
}

impl River {
    pub fn new(isl: &Island, k: usize) -> River {
        River { path: BorderPath::new(isl, k, 118.0), levels: Vec::new(), scale: 1.0 }
    }

    /// A stream along `angle(r)` (degrees) from `r0` map pixels out.
    pub fn stream(isl: &Island, k: usize, r0: f64, scale: f64, angle: impl Fn(f64) -> f64) -> River {
        River { path: BorderPath::along(isl, k, r0, angle), levels: Vec::new(), scale }
    }

    /// Set the water levels from the ground heights along the path: always
    /// falling, a few metres below the ground, down to the sea.
    pub fn set_levels(&mut self, ground: &[f64]) {
        let mut lv = Vec::with_capacity(ground.len());
        let mut last = f64::MAX;
        for (i, &g) in ground.iter().enumerate() {
            let s = self.path.s[i];
            let ds = if i == 0 { 0.0 } else { s - self.path.s[i - 1] };
            // easing down to the sea over the last stretch
            let to_sea = ((self.path.coast_s - s) * 0.002).max(0.0);
            let v = (g - 4.0).min(last - ds * 0.00005).min(to_sea).max(0.0);
            lv.push(v);
            last = v;
        }
        self.levels = lv;
    }

    /// Distance from the river's (meandering) line and distance along it.
    pub fn locate(&self, n: &Noise, wx: f64, wz: f64, r_px: f64) -> Option<(f64, f64)> {
        let k = self.path.k as f64 * 13.0;
        let ox = 2_600.0 * n.n2(wx / 22_000.0 + k, wz / 22_000.0) + 700.0 * n.n2(wx / 5_500.0, wz / 5_500.0 + k);
        let oz = 2_600.0 * n.n2(wx / 22_000.0, wz / 22_000.0 - k) + 700.0 * n.n2(wx / 5_500.0 - k, wz / 5_500.0);
        self.path.nearest(wx + ox, wz + oz, r_px)
    }

    pub fn level_at(&self, s: f64) -> f64 {
        let i = self.path.s.partition_point(|&v| v < s).clamp(1, self.path.s.len() - 1);
        let (s0, s1) = (self.path.s[i - 1], self.path.s[i]);
        let t = ((s - s0) / (s1 - s0).max(1e-9)).clamp(0.0, 1.0);
        self.levels[i - 1] + (self.levels[i] - self.levels[i - 1]) * t
    }

    fn along(&self, s: f64) -> f64 {
        (s / self.path.coast_s).clamp(0.0, 1.0)
    }

    /// Half-width of the water: a stream at the source, wide at the mouth.
    pub fn half_width(&self, s: f64) -> f64 {
        (40.0 + 360.0 * self.along(s)) * self.scale
    }

    /// Half-width of the valley the river has worn.
    pub fn valley(&self, s: f64) -> f64 {
        (2_500.0 + 4_500.0 * self.along(s)) * self.scale.powf(0.6)
    }

    /// The ground with the river's valley and channel cut into it.
    pub fn cut(&self, h: f64, d: f64, s: f64) -> f64 {
        if s > self.path.coast_s + 3_000.0 {
            return h;
        }
        let vw = self.valley(s);
        if d >= vw {
            return h;
        }
        let lv = self.level_at(s);
        let w = self.half_width(s);
        let mut h = h;
        // a broad, gently rising floor either side of the water
        let floor = lv + 1.5 + (d - w).max(0.0) * 0.004;
        if h > floor {
            h = floor + (h - floor) * sm(w * 1.5, vw, d);
        }
        // the channel
        if d < w * 1.25 {
            let depth = (3.0 + 7.0 * self.along(s)) * self.scale.sqrt();
            let bed = lv - depth * (1.0 - (d / (w * 1.25)).powi(2));
            h = h.min(bed);
        }
        h
    }
}

/// Where the bridge city crosses the canyon, at its midway point.
#[derive(Clone, Copy, Debug)]
pub struct BridgeSite {
    pub centre: (f64, f64),
    /// Unit vector along the deck (across the canyon).
    pub axis: (f64, f64),
    /// From the centre to each end of the deck (on the abutments).
    pub half_len: f64,
    /// Half-width of the gorge at the rim.
    pub span_half: f64,
    /// Half-width of the deck.
    pub half_w: f64,
    /// Height of the deck's top.
    pub deck_y: f64,
    /// Where the great arch meets the canyon walls: distance from the
    /// centre and height.
    pub spring_half: f64,
    pub spring_y: f64,
}

pub const DECK_HALF_W: f64 = 70.0;
/// The flat ground each end of the bridge stands on.
pub const ABUTMENT_R: f64 = 2_000.0;
pub const ABUTMENT_BLEND: f64 = 5_000.0;

impl BridgeSite {
    /// Deck coordinates: `u` along the deck from the centre, `v` across it.
    pub fn to_deck(&self, x: f64, z: f64) -> (f64, f64) {
        let (dx, dz) = (x - self.centre.0, z - self.centre.1);
        (dx * self.axis.0 + dz * self.axis.1, -dx * self.axis.1 + dz * self.axis.0)
    }

    pub fn from_deck(&self, u: f64, v: f64) -> (f64, f64) {
        (self.centre.0 + u * self.axis.0 - v * self.axis.1, self.centre.1 + u * self.axis.1 + v * self.axis.0)
    }

    /// The deck's height at a spot, if the spot is on the deck.
    pub fn deck_at(&self, x: f64, z: f64) -> Option<f64> {
        let (u, v) = self.to_deck(x, z);
        (u.abs() <= self.half_len && v.abs() <= self.half_w).then_some(self.deck_y)
    }

    /// The two ends of the deck (centres of the abutments).
    pub fn ends(&self) -> [(f64, f64); 2] {
        [self.from_deck(-self.half_len, 0.0), self.from_deck(self.half_len, 0.0)]
    }
}
