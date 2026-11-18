//! The island recipe: coastline, the two central peaks, region borders that
//! double as ridgelines, east-coast cliffs and the six regions.
//!
//! Coordinates here are *map pixels* on the 1600 x 1280 design map
//! (centre at CX, CY; +y is south). `grid.rs` converts to metres.

use super::noise::{angd, sm, Noise, Rng};

pub const MW: usize = 1600;
pub const MH: usize = 1280;
pub const CX: f64 = 800.0;
pub const CY: f64 = 640.0;
pub const D2R: f64 = std::f64::consts::PI / 180.0;
pub const SEED: u64 = 20_261_117;

/// Mount Everest and Mount St. Helens, in map pixels.
pub const EX: f64 = CX - 28.0;
pub const EY: f64 = CY - 38.0;
pub const HX: f64 = CX + 62.0;
pub const HY: f64 = CY + 30.0;

/// Region borders (degrees, map space: 0 = east, 90 = south).
pub const BORD: [f64; 6] = [-60.0, 0.0, 60.0, 120.0, 180.0, 240.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionId {
    North,
    Northeast,
    Southeast,
    South,
    Southwest,
    Northwest,
}

pub struct Region {
    pub id: RegionId,
    pub name: &'static str,
    /// The designer's description of the region.
    pub desc: &'static str,
    /// Centre direction in degrees (map space).
    pub centre: f64,
    /// Ground colour (sRGB 0–255) and forest colour.
    pub col: [f64; 3],
    pub fcol: [f64; 3],
    /// Forest density 0–1.
    pub forest: f64,
}

pub const REGIONS: [Region; 6] = [
    Region {
        id: RegionId::North,
        name: "North",
        desc: "Rock mines",
        centre: -90.0,
        col: [160.0, 152.0, 138.0],
        fcol: [84.0, 98.0, 72.0],
        forest: 0.03,
    },
    Region {
        id: RegionId::Northeast,
        name: "Northeast",
        desc: "Lava flows and desert",
        centre: -30.0,
        col: [226.0, 200.0, 144.0],
        fcol: [90.0, 100.0, 60.0],
        forest: 0.0,
    },
    Region {
        id: RegionId::Southeast,
        name: "Southeast",
        desc: "Mid forest with fields",
        centre: 30.0,
        col: [150.0, 176.0, 98.0],
        fcol: [62.0, 102.0, 50.0],
        forest: 0.42,
    },
    Region {
        id: RegionId::South,
        name: "South",
        desc: "Dark forest",
        centre: 90.0,
        col: [84.0, 112.0, 64.0],
        fcol: [34.0, 62.0, 36.0],
        forest: 0.97,
    },
    Region {
        id: RegionId::Southwest,
        name: "Southwest",
        desc: "Grasslands",
        centre: 150.0,
        col: [186.0, 198.0, 110.0],
        fcol: [84.0, 112.0, 58.0],
        forest: 0.03,
    },
    Region {
        id: RegionId::Northwest,
        name: "Northwest",
        desc: "Lakes and swampy forest",
        centre: 210.0,
        col: [104.0, 128.0, 82.0],
        fcol: [50.0, 82.0, 56.0],
        forest: 0.6,
    },
];

/// A point at `deg` degrees and `r` pixels from the map centre.
pub fn pol(deg: f64, r: f64) -> (f64, f64) {
    (CX + r * (deg * D2R).cos(), CY + r * (deg * D2R).sin())
}

/// Direction (degrees, map space) of a map point from the centre.
pub fn to_map_deg(mx: f64, my: f64) -> f64 {
    (my - CY).atan2(mx - CX) / D2R
}

/// Index of the region whose centre direction is nearest `deg`.
pub fn region_index(deg: f64) -> usize {
    let mut best = 0;
    let mut bd = f64::MAX;
    for (k, g) in REGIONS.iter().enumerate() {
        let d = angd(deg - g.centre).abs();
        if d < bd {
            bd = d;
            best = k;
        }
    }
    best
}

pub fn coast_r(deg: f64) -> f64 {
    let a = deg * D2R;
    355.0 + 18.0 * (3.0 * a + 0.7).sin() + 12.0 * (7.0 * a + 1.3).sin() + 7.0 * (13.0 * a).sin()
        - 160.0 * (-((deg - 48.0) / 13.0).powi(2)).exp()
}

/// How much a direction belongs to the east-coast cliff band.
pub fn cliff_w(deg: f64) -> f64 {
    sm(-70.0, -56.0, deg) * (1.0 - sm(14.0, 28.0, deg))
}

/// The seeded island: noise tables plus the derived constants.
pub struct Island {
    pub noise: Noise,
    pub islets: [(f64, f64, f64); 4],
}

impl Island {
    pub fn new() -> Self {
        let mut rng = Rng::new(SEED);
        let noise = Noise::new(&mut rng);
        let mk = |d: f64, r: f64, s: f64| {
            let (x, y) = pol(d, r);
            (x, y, s)
        };
        Island {
            noise,
            islets: [
                mk(172.0, 432.0, 14.0),
                mk(188.0, 452.0, 10.0),
                mk(200.0, 430.0, 15.0),
                mk(158.0, 444.0, 9.0),
            ],
        }
    }

    fn border_off(&self, k: usize, r: f64) -> f64 {
        16.0 * self.noise.n2(r / 70.0, k as f64 * 3.7)
    }

    /// Soft region weights for a direction (sum to 1), with wobbly borders.
    pub fn region_weights(&self, deg: f64, x: f64, y: f64, out: &mut [f64; 6]) {
        let dj = deg + 10.0 * self.noise.n2(x / 110.0, y / 110.0);
        let mut ws = 0.0;
        for (k, g) in REGIONS.iter().enumerate() {
            let w = (1.0 - ((angd(dj - g.centre)).abs() - 22.0) / 16.0).clamp(0.0, 1.0);
            out[k] = w;
            ws += w;
        }
        for w in out.iter_mut() {
            *w /= ws;
        }
    }

    /// "Landness": > 0 on land, < 0 in the sea, roughly distance-like.
    pub fn land_value(&self, x: f64, y: f64, r: f64, deg: f64) -> f64 {
        let n = &self.noise;
        let rr = coast_r(deg);
        let mut l = (rr - r) / rr
            + 0.11 * n.fbm(x / 130.0, y / 130.0, 3)
            + 0.06 * n.fbm(x / 45.0 + 7.0, y / 45.0, 3)
            + 0.022 * n.fbm(x / 11.0, y / 11.0, 2);
        for &(ix, iy, s) in &self.islets {
            let d = (x - ix).hypot(y - iy);
            if d < s * 2.5 {
                let li = (s - d) / 120.0 + 0.04 * n.fbm(x / 14.0 + 50.0, y / 14.0, 2);
                if li > l {
                    l = li;
                }
            }
        }
        l
    }

    pub fn land_at(&self, x: f64, y: f64) -> f64 {
        let (dx, dy) = (x - CX, y - CY);
        self.land_value(x, y, dx.hypot(dy), dy.atan2(dx) / D2R)
    }

    /// Elevation 0..~1.4 for a land point (`l` from `land_value`).
    pub fn elevation(&self, x: f64, y: f64, r: f64, deg: f64, l: f64) -> f64 {
        let n = &self.noise;
        let mut e = 0.26 * (l * 3.2).min(1.0).powf(0.75);
        let (ex, ey) = (x - EX, y - EY);
        let d1 = ex.hypot(ey);
        e += 0.95 * (-(d1 / 120.0).powf(1.25)).exp();
        let d2 = (x - HX).hypot(y - HY);
        e += 0.72 * (-(d2 / 58.0).powf(1.35)).exp() - 0.38 * (-(d2 / 15.0).powf(2.6)).exp();
        // ridges radiating from Everest
        let a1 = ey.atan2(ex) + 0.35 * n.n2(d1 / 90.0, 3.3) + 0.12 * n.n2(x / 40.0, y / 40.0);
        let (ca, sa) = (a1.cos(), a1.sin());
        let (mut rr, mut amp, mut f, mut nm) = (0.0, 0.5, 1.0, 0.0);
        for o in 0..4 {
            let v = 1.0
                - n.n3(ca * 3.6 * f, sa * 3.6 * f, d1 / 120.0 * f + o as f64 * 7.1)
                    .abs();
            rr += amp * v * v;
            nm += amp;
            amp *= 0.5;
            f *= 2.0;
        }
        rr /= nm;
        let inland = (l * 5.0).min(1.0);
        e += (rr - 0.4) * 0.26 * sm(8.0, 70.0, d1) * (-d1 / 340.0).exp() * inland;
        // region borders are ridgelines
        let mut br: f64 = 0.0;
        for k in 0..6 {
            let pd = (angd(deg - BORD[k]) * D2R * r + self.border_off(k, r)).abs();
            br = br.max((-(pd / 26.0) * (pd / 26.0)).exp());
        }
        e += br * 0.18 * sm(110.0, 190.0, r) * inland;
        e += 0.10 * n.fbm(x / 85.0, y / 85.0, 5) + 0.05 * (n.ridged(x / 55.0, y / 55.0, 4) - 0.45);
        let mut wr = [0.0; 6];
        self.region_weights(deg, x, y, &mut wr);
        if wr[0] > 0.0 {
            e += wr[0] * 0.1 * (n.ridged(x / 22.0, y / 22.0, 3) - 0.45) * inland;
        }
        e += cliff_w(deg) * 0.24 * sm(0.0, 0.025, l) * sm(230.0, 300.0, r);
        e.max(0.01)
    }

    /// The elevation recipe without the two peaks and the east-coast cliff
    /// (and without the minimum clamp). The full-size world adds those back
    /// in metres so Mount Everest and Mount St. Helens get their real heights.
    pub fn elevation_rest(&self, x: f64, y: f64, r: f64, deg: f64, l: f64) -> f64 {
        let n = &self.noise;
        let mut e = 0.26 * (l * 3.2).min(1.0).powf(0.75);
        let (ex, ey) = (x - EX, y - EY);
        let d1 = ex.hypot(ey);
        let a1 = ey.atan2(ex) + 0.35 * n.n2(d1 / 90.0, 3.3) + 0.12 * n.n2(x / 40.0, y / 40.0);
        let (ca, sa) = (a1.cos(), a1.sin());
        let (mut rr, mut amp, mut f, mut nm) = (0.0, 0.5, 1.0, 0.0);
        for o in 0..4 {
            let v = 1.0 - n.n3(ca * 3.6 * f, sa * 3.6 * f, d1 / 120.0 * f + o as f64 * 7.1).abs();
            rr += amp * v * v;
            nm += amp;
            amp *= 0.5;
            f *= 2.0;
        }
        rr /= nm;
        let inland = (l * 5.0).min(1.0);
        e += (rr - 0.4) * 0.26 * sm(8.0, 70.0, d1) * (-d1 / 340.0).exp() * inland;
        let mut br: f64 = 0.0;
        for k in 0..6 {
            let pd = (angd(deg - BORD[k]) * D2R * r + self.border_off(k, r)).abs();
            br = br.max((-(pd / 26.0) * (pd / 26.0)).exp());
        }
        e += br * 0.18 * sm(110.0, 190.0, r) * inland;
        e += 0.10 * n.fbm(x / 85.0, y / 85.0, 5) + 0.05 * (n.ridged(x / 55.0, y / 55.0, 4) - 0.45);
        let mut wr = [0.0; 6];
        self.region_weights(deg, x, y, &mut wr);
        if wr[0] > 0.0 {
            e += wr[0] * 0.1 * (n.ridged(x / 22.0, y / 22.0, 3) - 0.45) * inland;
        }
        e
    }

    /// Where a region border runs, at `r` pixels from the centre (degrees).
    pub fn border_angle(&self, k: usize, r: f64) -> f64 {
        BORD[k] - self.border_off(k, r) / r / D2R
    }
}

impl Default for Island {
    fn default() -> Self {
        Self::new()
    }
}
