//! The full-size world: the island at the size of Australia, sampled at any
//! point in metres. The shapes, regions and colours come from the design map
//! (`island.rs`); this file scales them up, gives Mount Everest and Mount
//! St. Helens their real heights, and adds the ground-level detail that the
//! map is too coarse to show.
//!
//! World space: metres, +x east, +z south, y up, island centre at (0, 0).

use super::island::*;
use super::noise::{lerp, sm, Rng};

/// Metres per design-map pixel. Chosen so the island's land area equals
/// Australia's (7,688,287 km²): the map island covers 378,277 px².
pub const METRES_PER_PX: f64 = 4508.0;
/// Australia's area, km².
pub const AUSTRALIA_KM2: f64 = 7_688_287.0;
/// Width of a round island with that area (≈ 3,129 km).
pub const ISLAND_WIDTH: f64 = 3_128_700.0;
/// The Cloudwall: three island widths of sea from the shore
/// (mean shore radius ≈ 1,554 km, + 3 × 3,129 km).
pub const CLOUD_R: f64 = 10_940_000.0;
/// The invisible barrier in front of the Cloudwall (until late game).
pub const WALL_R: f64 = CLOUD_R - 40_000.0;
/// Nothing but open sea beyond this distance from the centre.
pub const LAND_R: f64 = 480.0 * METRES_PER_PX;

/// Real heights of the two peaks, metres.
pub const EVEREST_M: f64 = 8849.0;
pub const ST_HELENS_M: f64 = 2549.0;

// Height recipe (see `macro_height`). Tuned so the peaks hit their real
// heights; checked by tests.
const BASE_M: f64 = 1700.0;
const EV_MASSIF_M: f64 = 5600.0;
const EV_MASSIF_R: f64 = 55.0; // px
const EV_HORN_R: f64 = 7.0; // px
const EV_HORN_M: f64 = 2766.0; // summit = EVEREST_M
const HEL_M: f64 = 1649.0; // rim = ST_HELENS_M
const CLIFF_M: f64 = 320.0;

pub fn to_map(wx: f64, wz: f64) -> (f64, f64) {
    (CX + wx / METRES_PER_PX, CY + wz / METRES_PER_PX)
}
pub fn to_world(mx: f64, my: f64) -> (f64, f64) {
    ((mx - CX) * METRES_PER_PX, (my - CY) * METRES_PER_PX)
}

/// Lava painted on the design map: dark crust and glowing core (map pixels).
pub struct Lava {
    pub crust: Vec<f32>,
    pub core: Vec<f32>,
}

impl Lava {
    fn at(arr: &[f32], mx: f64, my: f64) -> f64 {
        // bilinear, so the flows have smooth edges at full size
        let (fx, fy) = (mx - 0.5, my - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let g = |x: i64, y: i64| -> f64 {
            if x < 0 || y < 0 || x >= MW as i64 || y >= MH as i64 {
                0.0
            } else {
                arr[y as usize * MW + x as usize] as f64
            }
        };
        let (x0, y0) = (x0 as i64, y0 as i64);
        lerp(lerp(g(x0, y0), g(x0 + 1, y0), tx), lerp(g(x0, y0 + 1), g(x0 + 1, y0 + 1), tx), ty)
    }
    pub fn crust_at(&self, mx: f64, my: f64) -> f64 {
        Self::at(&self.crust, mx, my)
    }
    pub fn core_at(&self, mx: f64, my: f64) -> f64 {
        Self::at(&self.core, mx, my)
    }
}

fn build_lava(isl: &Island) -> Lava {
    let n = &isl.noise;
    let mut flows: Vec<Vec<(f64, f64)>> = Vec::new();
    let add_flow = |a0: f64, r0: f64, ph: f64, flows: &mut Vec<Vec<(f64, f64)>>| -> usize {
        let mut pts = Vec::new();
        let mut r = r0;
        while r < 460.0 {
            let a = a0 + 7.0 * n.n2(r / 55.0, ph) + 2.0 * n.n2(r / 14.0, ph + 3.0);
            let (x, y) = pol(a, r);
            if isl.land_at(x, y) < 0.035 {
                break;
            }
            pts.push((x, y));
            r += 3.0;
        }
        let len = pts.len();
        if len > 10 {
            flows.push(pts);
        }
        len
    };
    for &(a0, r0, ph) in &[(-53.0, 120.0, 1.3), (-43.0, 135.0, 5.1), (-19.0, 128.0, 8.7), (-8.0, 140.0, 12.2)] {
        let main = add_flow(a0, r0, ph, &mut flows);
        if main > 40 {
            add_flow(a0 + if a0 < -30.0 { 5.0 } else { -5.0 }, r0 + main as f64 * 1.5, ph + 20.0, &mut flows);
        }
    }
    let mut lava = Lava { crust: vec![0.0; MW * MH], core: vec![0.0; MW * MH] };
    let stamp = |arr: &mut Vec<f32>, x: f64, y: f64, rad: f64| {
        let r2 = rad * rad;
        let (y0, y1) = ((y - rad).floor() as i64, (y + rad) as i64);
        let (x0, x1) = ((x - rad).floor() as i64, (x + rad) as i64);
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                let d2 = (xx as f64 - x).powi(2) + (yy as f64 - y).powi(2);
                if d2 > r2 || xx < 0 || yy < 0 || xx >= MW as i64 || yy >= MH as i64 {
                    continue;
                }
                let k = yy as usize * MW + xx as usize;
                let t = (1.0 - d2.sqrt() / rad * 0.5) as f32;
                if t > arr[k] {
                    arr[k] = t;
                }
            }
        }
    };
    for pts in &flows {
        let len = pts.len() as f64;
        for (k, &(x, y)) in pts.iter().enumerate() {
            let kf = k as f64;
            let w = 3.0 + 6.0 * (kf / len) + 2.5 * n.n2(kf / 5.0, x / 50.0);
            stamp(&mut lava.crust, x, y, w.max(2.0));
            if n.n2(kf / 4.0, len) <= 0.28 {
                stamp(&mut lava.core, x, y, 0.8 + 1.4 * (kf / len));
            }
        }
    }
    lava
}

/// A Northwest lake: centre and radius in metres, and its water level.
#[derive(Clone, Copy, Debug)]
pub struct Lake {
    pub x: f64,
    pub z: f64,
    pub r: f64,
    pub level: f64,
}

/// Field patchwork colours for the Southeast.
const FIELD_COLS: [[f64; 3]; 6] = [
    [198.0, 196.0, 110.0],
    [222.0, 204.0, 128.0],
    [148.0, 170.0, 86.0],
    [186.0, 168.0, 100.0],
    [170.0, 190.0, 96.0],
    [210.0, 214.0, 140.0],
];

/// Everything known about one spot of ground.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    /// Ground height, metres (negative = sea floor).
    pub h: f64,
    /// Ground colour, sRGB 0–1.
    pub col: [f32; 3],
    /// Lava glow 0–1, forest cover 0–1, wetness 0–1.
    pub glow: f32,
    pub forest: f32,
    pub wet: f32,
    /// Dominant region (index into `REGIONS`), and whether it is farmland.
    pub region: u8,
    pub field: bool,
    /// Water surface over this spot (sea level 0, or a lake's level).
    pub water: Option<f64>,
    /// Distance to the shore in metres (negative out at sea), roughly.
    pub shore: f64,
}

/// The seeded, full-size world.
pub struct Terrain {
    pub island: Island,
    pub lava: Lava,
    pub lakes: Vec<Lake>,
}

/// Map-space measurements for one spot.
struct MapPt {
    x: f64,
    y: f64,
    r: f64,
    deg: f64,
    l: f64,
}

impl Terrain {
    pub fn new() -> Terrain {
        let island = Island::new();
        let lava = build_lava(&island);
        let mut t = Terrain { island, lava, lakes: Vec::new() };
        // Northwest lakes (same placement as the design map)
        let mut r2 = Rng::new(991);
        let mut tries = 0;
        while tries < 400 && t.lakes.len() < 9 {
            tries += 1;
            let (mx, my) = pol(186.0 + r2.next() * 50.0, 130.0 + r2.next() * 200.0);
            if t.island.land_at(mx, my) < 0.06 {
                continue;
            }
            let (x, z) = to_world(mx, my);
            let r = (5.0 + r2.next() * 8.0) * METRES_PER_PX * 1.1;
            // water level: a little under the lowest point of the shore
            let mut level = f64::MAX;
            for k in 0..24 {
                let a = k as f64 / 24.0 * std::f64::consts::TAU;
                let (sx, sz) = (x + a.cos() * r * 1.3, z + a.sin() * r * 0.8);
                level = level.min(t.height_no_lakes(sx, sz));
            }
            t.lakes.push(Lake { x, z, r, level: level - 2.0 });
        }
        t
    }

    fn map_pt(&self, wx: f64, wz: f64) -> MapPt {
        let (x, y) = to_map(wx, wz);
        let (dx, dy) = (x - CX, y - CY);
        let r = dx.hypot(dy);
        let deg = dy.atan2(dx) / D2R;
        let l = self.island.land_value(x, y, r, deg);
        MapPt { x, y, r, deg, l }
    }

    /// Rough distance to the shore in metres (positive inland).
    fn shore_dist(p: &MapPt) -> f64 {
        p.l * coast_r(p.deg) * METRES_PER_PX
    }

    /// Height from the map's shapes alone (no ground detail), metres.
    fn macro_height(&self, p: &MapPt) -> f64 {
        let d1 = (p.x - EX).hypot(p.y - EY);
        let d2 = (p.x - HX).hypot(p.y - HY);
        let rest = self.island.elevation_rest(p.x, p.y, p.r, p.deg, p.l).max(0.0);
        let mut h = 1.2 + BASE_M * rest.powf(1.3);
        h += EV_MASSIF_M * (-(d1 / EV_MASSIF_R).powf(1.2)).exp() + EV_HORN_M * (-(d1 / EV_HORN_R)).exp();
        let hel = (-(d2 / 58.0).powf(1.35)).exp() - 0.528 * (-(d2 / 15.0).powf(2.6)).exp();
        h += HEL_M * hel.max(0.0);
        let shore = Self::shore_dist(p);
        h += cliff_w(p.deg) * CLIFF_M * sm(0.0, 600.0, shore) * sm(230.0, 300.0, p.r);
        h
    }

    /// Ground-level detail the map is too coarse to show: rolling hills,
    /// rugged high ground, the rocky North, and small bumps underfoot.
    fn detail(&self, p: &MapPt, wx: f64, wz: f64, hm: f64) -> f64 {
        let n = &self.island.noise;
        let shore = sm(0.0, 3000.0, Self::shore_dist(p));
        if shore <= 0.0 {
            return 0.0;
        }
        let d1 = (p.x - EX).hypot(p.y - EY);
        let d2 = (p.x - HX).hypot(p.y - HY);
        let mut d = 55.0 * n.fbm(wx / 9000.0 + 31.7, wz / 9000.0 - 12.3, 4);
        let rug = sm(1500.0, 4500.0, hm) * sm(1.5, 9.0, d1) * sm(10.0, 28.0, d2);
        if rug > 0.0 {
            d += (n.ridged(wx / 14000.0 - 51.1, wz / 14000.0 + 7.7, 5) - 0.45) * 1400.0 * rug;
        }
        let mut wr = [0.0; 6];
        self.island.region_weights(p.deg, p.x, p.y, &mut wr);
        if wr[0] > 0.0 {
            d += wr[0] * (n.ridged(wx / 4000.0 + 77.0, wz / 4000.0, 4) - 0.45) * 260.0;
        }
        d += 5.0 * n.fbm(wx / 700.0 - 3.3, wz / 700.0 + 9.1, 3) + 0.8 * n.fbm(wx / 110.0 + 1.9, wz / 110.0, 2);
        // Everest's summit stays at exactly its real height
        d * shore * sm(0.3, 2.5, d1)
    }

    fn sea_floor(shore: f64) -> f64 {
        let off = -shore; // metres out to sea
        if off < 10_000.0 {
            -(1.0 + off * 0.02)
        } else {
            -(201.0 + (off - 10_000.0) * 0.05).min(4000.0)
        }
    }

    fn height_no_lakes(&self, wx: f64, wz: f64) -> f64 {
        let p = self.map_pt(wx, wz);
        if p.l <= 0.0 {
            return Self::sea_floor(Self::shore_dist(&p));
        }
        let hm = self.macro_height(&p);
        hm + self.detail(&p, wx, wz, hm)
    }

    /// The lake a spot is in (or near), with its scaled distance (1 = shore).
    fn lake_at(&self, wx: f64, wz: f64) -> Option<(&Lake, f64)> {
        for lk in &self.lakes {
            let dd = ((wx - lk.x) / 1.3).hypot((wz - lk.z) / 0.8) / lk.r;
            if dd < 1.15 {
                return Some((lk, dd));
            }
        }
        None
    }

    fn apply_lake(h: f64, lk: &Lake, dd: f64) -> f64 {
        if dd >= 1.0 {
            return h;
        }
        let bed = lk.level - 2.0 - 22.0 * (1.0 - dd * dd);
        let t = sm(1.0, 0.85, dd);
        h.min(lerp(h, bed, t))
    }

    /// Ground height in metres.
    pub fn height(&self, wx: f64, wz: f64) -> f64 {
        let h = self.height_no_lakes(wx, wz);
        match self.lake_at(wx, wz) {
            Some((lk, dd)) => Self::apply_lake(h, lk, dd),
            None => h,
        }
    }

    /// Height, colour and ground type for one spot.
    pub fn sample(&self, wx: f64, wz: f64) -> Sample {
        let p = self.map_pt(wx, wz);
        let n = &self.island.noise;
        let shore = Self::shore_dist(&p);
        let mut s = Sample { shore, ..Default::default() };
        let (mut cr, mut cg, mut cb);
        if p.l > 0.0 {
            let hm = self.macro_height(&p);
            let mut h = hm + self.detail(&p, wx, wz, hm);
            let (x, y) = (p.x, p.y);
            let mut wr = [0.0; 6];
            self.island.region_weights(p.deg, x, y, &mut wr);
            let (mut fd, mut f0, mut f1, mut f2) = (0.0, 0.0, 0.0, 0.0);
            cr = 0.0;
            cg = 0.0;
            cb = 0.0;
            let (mut best, mut bw) = (0usize, 0.0);
            for (q, g) in REGIONS.iter().enumerate() {
                let wq = wr[q];
                if wq == 0.0 {
                    continue;
                }
                cr += wq * g.col[0];
                cg += wq * g.col[1];
                cb += wq * g.col[2];
                f0 += wq * g.fcol[0];
                f1 += wq * g.fcol[1];
                f2 += wq * g.fcol[2];
                fd += wq * g.forest;
                if wq > bw {
                    bw = wq;
                    best = q;
                }
            }
            s.region = best as u8;
            // colour variation at map scale and at walking scale
            let patch = 1.0
                + 0.07 * n.fbm(x / 45.0 + 3.0, y / 45.0, 3)
                + 0.06 * n.fbm(wx / 2500.0 + 11.0, wz / 2500.0, 3)
                + 0.03 * n.n2(wx / 180.0, wz / 180.0 + 4.0);
            cr *= patch;
            cg *= patch;
            cb *= patch;
            macro_rules! mix {
                ($c:expr, $t:expr) => {{
                    let (c, t): ([f64; 3], f64) = ($c, $t);
                    cr += (c[0] - cr) * t;
                    cg += (c[1] - cg) * t;
                    cb += (c[2] - cb) * t;
                }};
            }
            // high ground: tan, then grey rock, then snow (by real height)
            mix!([176.0, 160.0, 118.0], sm(2200.0, 3600.0, h) * 0.85);
            mix!([150.0, 146.0, 140.0], sm(3400.0, 4700.0, h));
            mix!([250.0, 251.0, 255.0], sm(4900.0, 5700.0, h + 300.0 * n.n2(wx / 3000.0, wz / 3000.0)));
            // St. Helens' ash and crater
            let d2 = (x - HX).hypot(y - HY);
            mix!([132.0, 96.0, 72.0], 0.8 * (-(d2 / 46.0).powf(2.2)).exp());
            mix!([70.0, 52.0, 42.0], 0.7 * (-(d2 / 11.0).powi(2)).exp());
            // white cliffs along the east coast
            let cw = cliff_w(p.deg) * sm(230.0, 300.0, p.r) * (1.0 - sm(400.0, 2500.0, shore));
            mix!([226.0, 221.0, 208.0], cw * 0.8);
            // forest: the map's forests, broken up into woods and clearings
            let fine = n.fbm(wx / 3200.0 - 8.0, wz / 3200.0 + 2.0, 3);
            let fm_map = sm(1.0 - fd - 0.07, 1.0 - fd + 0.07, n.fbm(x / 60.0, y / 60.0, 3) * 1.4 + 0.5);
            let fm = (fm_map * sm(-0.35, 0.05, fine + fd * 0.6 - 0.3)) * (1.0 - sm(2600.0, 3400.0, h)) * sm(60.0, 400.0, shore);
            s.forest = fm as f32;
            // Southeast farmland: real-size fields
            let fw = wr[2] * (1.0 - fm) * (1.0 - sm(1200.0, 1800.0, h)) * sm(300.0, 1500.0, shore)
                * sm(-0.25, -0.05, n.fbm(x / 110.0 + 9.0, y / 110.0, 2));
            if fw > 0.01 {
                let u = wx * 0.88 + wz * 0.47;
                let v = -wx * 0.47 + wz * 0.88;
                let (cu, cv) = ((u / 420.0).floor(), (v / 290.0).floor());
                let hv = n.perm((n.perm((cu as i64 & 255) as usize) as usize + (cv as i64 & 255) as usize) & 511) as f64 / 255.0;
                let (fu, fv) = (u / 420.0 - cu, v / 290.0 - cv);
                let fk = sm(0.12, 0.35, fw);
                mix!(FIELD_COLS[((hv * 6.0).floor() as usize) % 6], fk * 0.95);
                if fu < 0.025 || fv < 0.035 {
                    mix!([72.0, 90.0, 48.0], fk * 0.8);
                }
                s.field = fk > 0.5;
            }
            if fm > 0.0 {
                mix!([f0, f1, f2], fm * 0.85);
            }
            // beaches
            let sand = (1.0 - sm(25.0, 220.0, shore)) * (1.0 - sm(3.0, 9.0, h)) * (1.0 - cw);
            mix!([228.0, 216.0, 170.0], sand * 0.8);
            // Northwest marsh: map-scale damp ground and walking-scale pools
            let pw = wr[5] * (1.0 - sm(900.0, 1500.0, h)) * sm(200.0, 2000.0, shore);
            if pw > 0.02 {
                mix!([92.0, 118.0, 96.0], pw * 0.22);
                let pn = n.fbm(wx / 650.0 + 70.0, wz / 650.0, 3);
                let pool = sm(0.22, 0.27, pn) * pw;
                let edge = (sm(0.16, 0.22, pn) - sm(0.22, 0.27, pn)) * pw;
                mix!([70.0, 92.0, 74.0], edge * 0.6);
                mix!([92.0, 128.0, 132.0], pool * 0.92);
                s.wet = s.wet.max(pool as f32);
            }
            // lakes
            if let Some((lk, dd)) = self.lake_at(wx, wz) {
                h = Self::apply_lake(h, lk, dd);
                if dd < 1.05 {
                    s.water = Some(lk.level);
                }
                let t = 1.0 - sm(0.9, 1.08, dd);
                mix!([170.0, 168.0, 130.0], t);
                s.wet = s.wet.max(t as f32);
            }
            // Northeast lava
            let crust = self.lava.crust_at(x, y);
            let core = self.lava.core_at(x, y);
            if crust > 0.0 {
                let edge = sm(0.0, 0.25, crust + 0.08 * n.n2(wx / 900.0, wz / 900.0));
                mix!([52.0, 40.0, 34.0], edge * 0.92);
            }
            if core > 0.0 {
                let c = sm(0.05, 0.45, core + 0.1 * n.n2(wx / 400.0 + 3.0, wz / 400.0));
                mix!([200.0, 90.0, 30.0], c);
                s.glow = c as f32;
            }
            if h < 0.6 && s.water.is_none() {
                s.water = Some(0.0);
            }
            s.h = h;
        } else {
            s.h = Self::sea_floor(shore);
            let t = sm(0.0, 3000.0, -shore);
            cr = lerp(214.0, 120.0, t);
            cg = lerp(204.0, 130.0, t);
            cb = lerp(160.0, 120.0, t);
            s.water = Some(0.0);
        }
        s.col = [(cr / 255.0).clamp(0.0, 1.0) as f32, (cg / 255.0).clamp(0.0, 1.0) as f32, (cb / 255.0).clamp(0.0, 1.0) as f32];
        s
    }

    /// Region name and description for a spot, as shown on the banner.
    pub fn place_name(&self, wx: f64, wz: f64) -> (&'static str, &'static str) {
        let (mx, my) = to_map(wx, wz);
        if self.island.land_at(mx, my) <= 0.0 && wx.hypot(wz) > 100_000.0 {
            return ("Rim Sea", "The bowl of water around the island");
        }
        let h = self.height(wx, wz);
        if (mx - EX).hypot(my - EY) < 40.0 && h > 4500.0 {
            return ("Mount Everest", "The highest peak on the shard");
        }
        if (mx - HX).hypot(my - HY) < 30.0 && h > 1500.0 {
            return ("Mount St. Helens", "The volcano beside Everest");
        }
        let g = &REGIONS[region_index((my - CY).atan2(mx - CX) / D2R)];
        (g.name, g.desc)
    }
}

impl Default for Terrain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_area_matches_australia() {
        let isl = Island::new();
        let mut land = 0usize;
        for y in 0..MH {
            for x in 0..MW {
                if isl.land_at(x as f64 + 0.5, y as f64 + 0.5) > 0.0 {
                    land += 1;
                }
            }
        }
        let km2 = land as f64 * (METRES_PER_PX / 1000.0).powi(2);
        assert!((km2 / AUSTRALIA_KM2 - 1.0).abs() < 0.01, "island area {km2} km²");
    }

    #[test]
    fn peaks_have_real_heights() {
        let t = Terrain::new();
        let (ex, ez) = to_world(EX, EY);
        let ev = t.height(ex, ez);
        assert!((ev - EVEREST_M).abs() < 15.0, "Everest {ev}");
        // nothing on the island is higher than Everest; St. Helens' rim is its real height
        let (hx, hz) = to_world(HX, HY);
        // St. Helens: highest point of the volcano (within 20 map px of its centre)
        let mut hel: f64 = 0.0;
        for j in -40..=40 {
            for i in -40..=40 {
                let (di, dj) = (i as f64 * 0.5, j as f64 * 0.5);
                if di.hypot(dj) <= 20.0 {
                    hel = hel.max(t.height(hx + di * METRES_PER_PX, hz + dj * METRES_PER_PX));
                }
            }
        }
        assert!((hel - ST_HELENS_M).abs() < 60.0, "St. Helens {hel}");
    }

    #[test]
    fn sea_out_to_the_cloudwall() {
        let t = Terrain::new();
        assert!(t.height(CLOUD_R * 0.5, 0.0) < -1000.0);
        assert!(CLOUD_R - 1_554_000.0 > 2.95 * ISLAND_WIDTH);
    }
}
