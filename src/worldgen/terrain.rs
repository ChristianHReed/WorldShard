//! The full-size world: the island at the size of Australia, sampled at any
//! point in metres. The shapes, regions and colours come from the design map
//! (`island.rs`); this file scales them up, gives Mount Everest and Mount
//! St. Helens their real heights, and adds the ground-level detail that the
//! map is too coarse to show.
//!
//! World space: metres, +x east, +z south, y up, island centre at (0, 0).

use super::borders::{BorderPath, BridgeSite, Canyon, River, ABUTMENT_BLEND, ABUTMENT_R, DECK_HALF_W};
use super::island::*;
use super::noise::{angd, lerp, sm, Rng};

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

/// The two central peaks: 25 times their real heights (8,849 m and
/// 2,549 m), so they tower over an island the size of Australia and can be
/// seen from anywhere on it.
pub const PEAK_SCALE: f64 = 25.0;
pub const EVEREST_M: f64 = PEAK_SCALE * 8849.0;
pub const ST_HELENS_M: f64 = PEAK_SCALE * 2549.0;

// Shapes of the giants, metres. The spire and St. Helens' cone are scaled
// when the world is built so the summits land exactly on the heights above.
pub const EV_SPIRE_R: f64 = 60_000.0;
pub const EV_MASSIF_M: f64 = 85_000.0;
pub const EV_MASSIF_R: f64 = 220_000.0;
/// Shape of Everest's body: higher = steeper edge, less long apron.
pub const EV_MASSIF_P: f64 = 1.6;
pub const HEL_R: f64 = 55_000.0;
const HEL_CRATER_R: f64 = 11_000.0;
/// The range of lesser mountains around the giants.
const RANGE_R: f64 = 260_000.0;
const CLIFF_M: f64 = 600.0;

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

/// The lava flows (the designer's choice: they pour from Mount St. Helens).
/// They leave through the crater's open north side as one flow, then split
/// and run north-east across the Northeast to the sea, staying north of the
/// Great Canyon.
fn build_lava(isl: &Island) -> Lava {
    let n = &isl.noise;
    let mut flows: Vec<Vec<(f64, f64)>> = Vec::new();
    let p0 = (HX, HY - 5.0);
    let p1 = (HX + 4.0, HY - 45.0);
    for (k, &(a0, ph)) in [(-56.0, 1.3), (-44.0, 5.1), (-30.0, 8.7), (-17.0, 12.2)].iter().enumerate() {
        let p2 = pol(a0, 210.0);
        let p3 = pol(a0, 470.0);
        let bez = |t: f64| -> (f64, f64) {
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
        };
        let mut pts = Vec::new();
        let mut t = 0.0;
        let mut last = bez(0.0);
        while t < 1.0 {
            // step about 3 px along the curve
            let mut t2 = t + 0.002;
            while t2 < 1.0 && {
                let q = bez(t2);
                (q.0 - last.0).hypot(q.1 - last.1) < 3.0
            } {
                t2 += 0.002;
            }
            t = t2;
            let (x, y) = bez(t.min(1.0));
            // the flows wander, more as they spread out
            let (dx, dy) = (x - last.0, y - last.1);
            let dl = dx.hypot(dy).max(1e-9);
            let wob = (4.0 * n.n2(t * 9.0, ph) + 1.5 * n.n2(t * 40.0, ph + 3.0)) * sm(0.03, 0.2, t);
            let (wx, wy) = (x - dy / dl * wob, y + dx / dl * wob);
            last = (x, y);
            if t > 0.15 && isl.land_at(wx, wy) < 0.035 {
                break;
            }
            pts.push((wx, wy));
        }
        if pts.len() > 10 {
            flows.push(pts);
        }
        let _ = k;
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

/// The giants' range (the designer's choice: peaks just under each giant).
/// Each entry: which giant it stands around (0 Everest, 1 St. Helens),
/// bearing from that giant in degrees (0 = east, 90 = south), distance
/// in km, and the summit height in km. None stand between the two giants,
/// so each giant keeps a clear view of the other.
const RANGE_PEAKS: [(usize, f64, f64, f64); 10] = [
    (0, 95.0, 330.0, 176.0),
    (0, 145.0, 300.0, 192.0),
    (0, 195.0, 360.0, 160.0),
    (0, 245.0, 310.0, 199.0),
    (0, 295.0, 380.0, 168.0),
    (0, 345.0, 320.0, 154.0),
    (1, 330.0, 125.0, 51.0),
    (1, 5.0, 140.0, 54.0),
    (1, 60.0, 115.0, 47.0),
    (1, 120.0, 135.0, 41.0),
];
/// Ridges joining the range peaks (indexes into RANGE_PEAKS): a chain around
/// each giant, and a long, lower one from Everest's chain to St. Helens'.
const RANGE_RIDGES: [(usize, usize); 9] = [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (6, 7), (7, 8), (8, 9), (0, 9)];

/// One peak of the giants' range: a steep spire on a broad body.
#[derive(Clone, Copy, Debug)]
pub struct RangePeak {
    pub x: f64,
    pub z: f64,
    /// target summit height, metres
    pub target: f64,
    pub spire_m: f64,
    pub spire_r: f64,
    pub body_m: f64,
    pub body_r: f64,
}

/// A ridge line between two range peaks: high at the ends, dipping to a
/// saddle in the middle.
#[derive(Clone, Copy, Debug)]
pub struct Ridge {
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub ha: f64,
    pub hb: f64,
    pub saddle: f64,
}

impl Ridge {
    fn height_at(&self, x: f64, z: f64) -> f64 {
        let (dx, dz) = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let t = (((x - self.a.0) * dx + (z - self.a.1) * dz) / (dx * dx + dz * dz)).clamp(0.0, 1.0);
        let (cx, cz) = (self.a.0 + dx * t, self.a.1 + dz * t);
        let d = (x - cx).hypot(z - cz);
        let along = self.ha + (self.hb - self.ha) * t;
        let hr = self.saddle + (along - self.saddle) * (2.0 * t - 1.0).powi(2);
        let w = (0.45 * hr).max(4_000.0);
        if d > 4.0 * w {
            return 0.0;
        }
        hr * (-(d / w).powf(1.4)).exp()
    }
}

/// Where the range peaks stand (world metres) and their summit heights:
/// fixed by the design, so level-of-detail code can use them without a
/// `Terrain`.
pub fn range_peak_spots() -> [(f64, f64, f64); 10] {
    let giants = [to_world(EX, EY), to_world(HX, HY)];
    RANGE_PEAKS.map(|(g, bearing, km, top_km)| {
        let a = bearing.to_radians();
        (giants[g].0 + a.cos() * km * 1000.0, giants[g].1 + a.sin() * km * 1000.0, top_km * 1000.0)
    })
}

/// Samples along the light's path past each peak when finding its shadow
/// (the terrain shader uses the same number).
pub const SHADOW_STEPS: usize = 14;

/// A big peak's simple shape: a spire on a body.
#[derive(Clone, Copy, Debug)]
pub struct ShadowPeak {
    pub x: f64,
    pub z: f64,
    pub spire_m: f64,
    pub spire_r: f64,
    pub body_m: f64,
    pub body_r: f64,
    pub body_p: f64,
}

impl ShadowPeak {
    pub fn top(&self, d: f64) -> f64 {
        self.spire_m * (-d / self.spire_r).exp() + self.body_m * (-(d / self.body_r).powf(self.body_p)).exp()
    }
    /// Beyond this distance from its axis the peak is too low to matter.
    pub fn reach(&self) -> f64 {
        (3.0 * self.spire_r).max(if self.body_m > 0.0 { 1.6 * self.body_r } else { 0.0 })
    }
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

/// Makes region weights change over a shorter distance (power `p`), so
/// each region's landform and colour stay distinct up to its border.
fn sharpen(w: &mut [f64; 6], p: i32) {
    let mut sum = 0.0;
    for v in w.iter_mut() {
        *v = v.powi(p);
        sum += *v;
    }
    if sum > 0.0 {
        for v in w.iter_mut() {
            *v /= sum;
        }
    }
}

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
    /// On a Southeast field edge (hedgerows grow there).
    pub hedge: bool,
    /// Slow patchiness (about −1..1, a few hundred metres across), for
    /// grouping things: boulder fields, copses, flower drifts, mushroom rings.
    pub patch: f32,
    /// Inside the Great Canyon (dry, though far below the sea).
    pub canyon: bool,
    /// In a river.
    pub river: bool,
    /// How sandy and how rocky the ground is (0–1), for the ground detail
    /// drawn close up (grass where it is neither).
    pub sand: f32,
    pub rock: f32,
}

/// The seeded, full-size world.
pub struct Terrain {
    pub island: Island,
    pub lava: Lava,
    pub lakes: Vec<Lake>,
    /// The summits in world metres, and the heights of Everest's spire and
    /// St. Helens' cone (set so the summits are exactly EVEREST_M / ST_HELENS_M).
    pub everest: (f64, f64),
    pub helens: (f64, f64),
    pub ev_spire_m: f64,
    pub hel_m: f64,
    /// The giants' range: peaks just under each giant, joined by ridges.
    pub peaks: Vec<RangePeak>,
    pub ridges: Vec<Ridge>,
    /// The deep canyon between the Northeast and Southeast.
    pub canyon: Canyon,
    /// Rivers on the lowland borders.
    pub rivers: Vec<River>,
    /// The bridge city across the canyon.
    pub bridge: BridgeSite,
    /// The bridge city's buildings, sorted along the deck.
    pub city: Vec<crate::city::Lot>,
    /// Off while the world is being set up (the border features are
    /// measured from the land without them).
    features: bool,
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
        let canyon = Canyon::new(&island, CANYON_BORDER);
        let mut t = Terrain {
            island,
            lava,
            lakes: Vec::new(),
            everest: to_world(EX, EY),
            helens: to_world(HX, HY),
            ev_spire_m: 0.0,
            hel_m: 0.0,
            peaks: Vec::new(),
            ridges: Vec::new(),
            canyon,
            rivers: Vec::new(),
            bridge: BridgeSite { centre: (0.0, 0.0), axis: (1.0, 0.0), half_len: 0.0, span_half: 0.0, half_w: DECK_HALF_W, deck_y: 0.0, spring_half: 0.0, spring_y: 0.0 },
            features: false,
            city: Vec::new(),
        };
        t.rivers = RIVER_BORDERS.iter().map(|&k| River::new(&t.island, k)).collect();
        // the giants' range: place the peaks and ridges, then size each
        // peak's spire so its summit lands near its target height
        t.peaks = range_peak_spots()
            .iter()
            .map(|&(x, z, target)| RangePeak { x, z, target, spire_m: 0.0, spire_r: 0.25 * target, body_m: 0.25 * target, body_r: 0.7 * target })
            .collect();
        t.ridges = RANGE_RIDGES
            .iter()
            .map(|&(i, j)| {
                let (a, b) = (t.peaks[i], t.peaks[j]);
                Ridge { a: (a.x, a.z), b: (b.x, b.z), ha: 0.5 * a.target, hb: 0.5 * b.target, saddle: 0.3 * a.target.min(b.target) }
            })
            .collect();
        // size the giants so their summits hit the target heights (Everest
        // and its range lean on each other a little, so settle them together)
        let (ex, ez) = t.everest;
        for _ in 0..3 {
            t.size_range_peaks(0..6);
            t.ev_spire_m += EVEREST_M - t.height_no_lakes(ex, ez);
        }
        let rim: Vec<(f64, f64)> = (0..48)
            .flat_map(|a| (0..20).map(move |k| (a, k)))
            .map(|(a, k)| {
                let ang = a as f64 / 48.0 * std::f64::consts::TAU;
                let d = 5_000.0 + k as f64 * 4_000.0;
                (t.helens.0 + ang.cos() * d, t.helens.1 + ang.sin() * d)
            })
            .collect();
        let (mut lo, mut hi) = (0.0, 200_000.0);
        for _ in 0..24 {
            t.hel_m = (lo + hi) / 2.0;
            let top = rim.iter().map(|&(x, z)| t.height_no_lakes(x, z)).fold(f64::MIN, f64::max);
            if top < ST_HELENS_M {
                lo = t.hel_m;
            } else {
                hi = t.hel_m;
            }
        }
        t.hel_m = (lo + hi) / 2.0;
        t.size_range_peaks(6..10);
        t.size_range_peaks(6..10);
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
        // where the land really meets the sea along the canyon and the rivers
        // (the coastline's noise moves it a few km from the map's outline):
        // the canyon must end, and its natural wall stand, short of the
        // nearest point of the shore anywhere across its width
        let shore = t.real_shore(&t.canyon.path, &[-35_000.0, -17_500.0, 0.0, 17_500.0, 35_000.0]);
        t.canyon.path.coast_s = shore;
        for i in 0..t.rivers.len() {
            let shore = t.real_shore(&t.rivers[i].path, &[0.0]);
            t.rivers[i].path.coast_s = shore;
        }
        // rivers: their water falls steadily from the central range to the sea
        for i in 0..t.rivers.len() {
            let ground: Vec<f64> = t.rivers[i].path.pts.iter().map(|&(x, z)| t.height_no_lakes(x, z)).collect();
            t.rivers[i].set_levels(&ground);
        }
        // the bridge city at the canyon's midway point: its deck is level
        // with the higher rim, and each end stands on flattened ground
        let ((cx, cz), (tx, tz)) = t.canyon.path.at(t.canyon.mid_s);
        let axis = (-tz, tx);
        let span_half = t.canyon.half_width(t.canyon.mid_s);
        let half_len = span_half + 1_500.0;
        let rim = |sgn: f64| t.height_no_lakes(cx + axis.0 * sgn * (span_half + 2_500.0), cz + axis.1 * sgn * (span_half + 2_500.0));
        let deck_y = rim(1.0).max(rim(-1.0)).max(5.0) + 20.0;
        t.bridge = BridgeSite { centre: (cx, cz), axis, half_len, span_half, half_w: DECK_HALF_W, deck_y, spring_half: span_half * 0.8, spring_y: deck_y - 8_000.0 };
        t.features = true;
        // the great arch springs from the walls 8 km below the deck
        let mut x = 0.0;
        while x < span_half {
            if t.height_no_lakes(cx + axis.0 * x, cz + axis.1 * x) >= t.bridge.spring_y {
                break;
            }
            x += 50.0;
        }
        t.bridge.spring_half = x;
        t.city = crate::city::layout(&t.bridge);
        t
    }

    /// Set the spires of some range peaks so their summits are at their targets.
    fn size_range_peaks(&mut self, which: std::ops::Range<usize>) {
        for i in which {
            let pk = self.peaks[i];
            let h = self.height_no_lakes(pk.x, pk.z);
            self.peaks[i].spire_m = (pk.spire_m + pk.target - h).max(0.15 * pk.target);
        }
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

    /// The landform of each region, in metres above the sea (before the
    /// coast, the region-border ridgelines and the central mountains).
    fn region_relief(&self, k: usize, x: f64, z: f64) -> f64 {
        let n = &self.island.noise;
        match k {
            // North, rock mines: high, rugged, knife-edged rock mountains
            // with deep narrow canyons
            0 => {
                let r = n.ridged(x / 16_000.0 + 11.0, z / 16_000.0, 5);
                let crags = n.ridged(x / 3_000.0, z / 3_000.0 + 7.0, 4);
                let canyon = (1.0 - n.n2(x / 9_000.0 - 3.0, z / 9_000.0 + 1.0).abs()).powi(10);
                1200.0 + 3800.0 * r.powf(2.2) + 700.0 * (crags - 0.45) * (0.4 + r) - 900.0 * canyon
            }
            // Northeast, lava flows and desert: a high plateau of stepped
            // mesas and buttes, seas of tall dunes, and volcanic cones
            1 => {
                let m = n.fbm(x / 14_000.0 - 4.0, z / 14_000.0, 4);
                let mesas = sm(0.03, 0.04, m) * 380.0 + sm(0.17, 0.178, m) * 300.0 + sm(0.3, 0.306, m) * 220.0;
                let sand = sm(0.02, -0.08, m);
                let u = x * 0.8 + z * 0.6;
                let wave = (u / 1500.0 + 2.5 * n.n2(x / 5_000.0, z / 5_000.0)) * std::f64::consts::PI;
                let dunes = (1.0 - wave.sin().abs()).powf(1.5) * (130.0 + 60.0 * n.n2(x / 9_000.0, z / 9_000.0)) * sand;
                // smaller dunes riding on the big ones, crests across the wind
                let u2 = x * 0.75 - z * 0.66;
                let wave2 = (u2 / 320.0 + 1.6 * n.n2(x / 1_400.0 + 3.0, z / 1_400.0)) * std::f64::consts::PI;
                let ripples = (1.0 - wave2.sin().abs()).powi(2) * 28.0 * sm(0.03, -0.02, m);
                let c = n.fbm(x / 6_000.0 + 13.0, z / 6_000.0 - 2.0, 2);
                let cones = sm(0.32, 0.52, c).powi(2) * 750.0;
                700.0 + 200.0 * n.fbm(x / 40_000.0, z / 40_000.0 + 3.0, 3) + mesas + dunes + ripples + cones
            }
            // Southeast, mid forest with fields: soft rolling green hills
            2 => 180.0 + 160.0 * n.fbm(x / 8_000.0 + 5.0, z / 8_000.0, 4) + 50.0 * n.fbm(x / 2_000.0, z / 2_000.0 - 9.0, 3),
            // South, dark forest: steep, tall wooded hills and deep ravines
            3 => {
                let r = n.ridged(x / 9_000.0 - 7.0, z / 9_000.0 + 3.0, 4);
                let ravine = (1.0 - n.n2(x / 4_000.0 + 2.0, z / 4_000.0).abs()).powi(6);
                500.0 + 1400.0 * r.powf(1.4) - 300.0 * ravine
            }
            // Southwest, grasslands: flat open plains with lone, steep-sided
            // rock domes rising out of them
            4 => {
                let b = n.fbm(x / 12_000.0 + 2.0, z / 12_000.0, 3);
                let dome = sm(0.42, 0.47, b) * (260.0 + 120.0 * n.n2(x / 3_000.0, z / 3_000.0));
                110.0 + 12.0 * n.fbm(x / 9_000.0 - 1.0, z / 9_000.0, 3) + dome
            }
            // Northwest, lakes and swampy forest: flat, low wetland
            _ => 25.0 + 6.0 * n.fbm(x / 3_000.0 + 8.0, z / 3_000.0, 3),
        }
    }

    /// Ground height on land, metres (before lakes).
    fn land_height(&self, p: &MapPt, wx: f64, wz: f64) -> f64 {
        let n = &self.island.noise;
        let shore = Self::shore_dist(p);
        let (d1, d2) = ((wx - self.everest.0).hypot(wz - self.everest.1), (wx - self.helens.0).hypot(wz - self.helens.1));
        // each region's landform, rising from low coasts
        let inland = sm(0.0, 45_000.0, shore);
        let mut wr = [0.0; 6];
        self.island.region_weights(p.deg, p.x, p.y, &mut wr);
        sharpen(&mut wr, 4);
        let mut h = 1.2;
        for (k, &w) in wr.iter().enumerate() {
            if w > 0.0 {
                h += w * inland * self.region_relief(k, wx, wz);
            }
        }
        // mountain ranges along the North / Northeast and Northwest / North
        // borders, from the central range out to the coast
        if p.r > 95.0 {
            for k in RANGE_BORDERS {
                let pd = (angd(p.deg - self.island.border_angle(k, p.r)) * D2R * p.r).abs();
                if pd < 32.0 {
                    let core = (-(pd / 9.0).powf(1.6)).exp();
                    let crest = 0.55 + 0.45 * n.ridged(wx / 18_000.0 + 7.0 * k as f64, wz / 18_000.0, 4);
                    let peaks = n.ridged(wx / 6_000.0 + 2.0, wz / 6_000.0 - 1.0, 4) - 0.45;
                    h += core * (3_400.0 * crest + 1_400.0 * peaks) * sm(95.0, 130.0, p.r) * sm(0.0, 25_000.0, shore);
                }
            }
        }
        // a range of lesser mountains around the giants
        let mask = ((-(d1 / RANGE_R).powf(1.6)).exp() + 0.35 * (-(d2 / 110_000.0).powf(1.6)).exp()).min(1.0);
        if mask > 0.002 {
            let r = n.ridged(wx / 45_000.0 - 21.0, wz / 45_000.0 + 8.0, 5);
            let range = 2200.0 + 8000.0 * r.powf(1.6) + 1500.0 * (n.ridged(wx / 12_000.0 + 5.0, wz / 12_000.0, 4) - 0.45);
            h += mask * range * sm(6_000.0, 30_000.0, d1) * sm(12_000.0, 70_000.0, d2) * inland;
        }
        // Mount Everest (a broad massif and a steep summit spire) and the
        // peaks of the giants' range around both giants, ribbed with ridges
        let mut big = 0.0;
        let mut summit_fade = sm(1_500.0, 12_000.0, d1);
        if d1 < 900_000.0 {
            big += self.ev_spire_m * (-(d1 / EV_SPIRE_R)).exp() + EV_MASSIF_M * (-(d1 / EV_MASSIF_R).powf(EV_MASSIF_P)).exp();
        }
        let mut peaks = 0.0;
        for pk in &self.peaks {
            let d = (wx - pk.x).hypot(wz - pk.z);
            if d < 5.0 * pk.body_r {
                peaks += pk.spire_m * (-(d / pk.spire_r)).exp() + pk.body_m * (-(d / pk.body_r).powf(1.6)).exp();
                summit_fade = summit_fade.min(sm(1_000.0, 0.2 * pk.spire_r, d));
            }
        }
        // ridges join the range peaks; where a peak stands higher, the peak wins
        let mut ridge: f64 = 0.0;
        if peaks > 0.0 || d1 < 1_200_000.0 {
            let (wob_x, wob_z) = (n.n2(wx / 90_000.0 + 7.0, wz / 90_000.0), n.n2(wx / 90_000.0, wz / 90_000.0 - 3.0));
            for r in &self.ridges {
                let wob = 0.5 * r.saddle;
                ridge = ridge.max(r.height_at(wx + wob * wob_x, wz + wob * wob_z));
            }
            if ridge > 0.0 {
                ridge *= 0.82 + 0.36 * (n.ridged(wx / 14_000.0 + 2.0, wz / 14_000.0 - 5.0, 4) - 0.45);
            }
        }
        big += peaks.max(ridge);
        if big > 0.0 {
            h += big + (n.ridged(wx / 9_000.0 + 40.0, wz / 9_000.0, 5) - 0.45) * 0.06 * big * summit_fade;
        }
        // Mount St. Helens: a volcanic cone with its crater breached to the north
        if d2 < 400_000.0 {
            let cone = (-(d2 / HEL_R)).exp();
            let crater = (-(d2 / HEL_CRATER_R).powf(2.6)).exp();
            let ang = (wz - self.helens.1).atan2(wx - self.helens.0).to_degrees();
            let north = (-(angd(ang + 90.0) / 35.0).powi(2)).exp();
            let breach = north * (-(d2 / (2.4 * HEL_CRATER_R)).powi(2)).exp();
            h += self.hel_m * (cone - 0.4 * crater - 0.25 * breach).max(0.0);
        }
        // white cliffs along the east coast: sheer walls of varying height
        let cw = cliff_w(p.deg) * sm(230.0, 300.0, p.r);
        if cw > 0.0 {
            let tall = CLIFF_M * (1.0 + 0.9 * n.fbm(wx / 30_000.0 + 4.0, wz / 30_000.0, 3)).clamp(0.3, 1.7);
            h += cw * tall * sm(0.0, 140.0, shore + 60.0 * n.n2(wx / 900.0, wz / 900.0));
        }
        // lava flows run in shallow channels
        let crust = self.lava.crust_at(p.x, p.y);
        if crust > 0.0 {
            h -= 18.0 * sm(0.0, 0.3, crust);
        }
        // small bumps underfoot (none at the summits)
        h += (5.0 * n.fbm(wx / 700.0 - 3.3, wz / 700.0 + 9.1, 3) + 0.8 * n.fbm(wx / 110.0 + 1.9, wz / 110.0, 2))
            * sm(0.0, 3000.0, shore)
            * sm(300.0, 3000.0, d1);
        if self.features {
            h = self.border_features(p, wx, wz, h);
        }
        h
    }

    /// The canyon (with its sea wall and the bridge's flattened ends) and
    /// the rivers, cut into the land.
    fn border_features(&self, p: &MapPt, wx: f64, wz: f64, h: f64) -> f64 {
        let n = &self.island.noise;
        let mut h = h;
        let b = &self.bridge;
        for (ex, ez) in b.ends() {
            let d = (wx - ex).hypot(wz - ez);
            if d < ABUTMENT_BLEND {
                h = lerp(h, b.deck_y - 1.0, sm(ABUTMENT_BLEND, ABUTMENT_R, d));
            }
        }
        if let Some((d, s)) = self.canyon.path.nearest(wx, wz, p.r) {
            h += self.canyon.sea_wall(d, s);
            h -= self.canyon.carve(n, wx, wz, d, s);
        }
        for rv in &self.rivers {
            if let Some((d, s)) = rv.locate(n, wx, wz, p.r) {
                h = rv.cut(h, d, s);
            }
        }
        h
    }

    /// The first distance along `path` where the ground (without the
    /// canyon and rivers) drops into the sea, nearest of the lines run
    /// `offsets` metres to either side of the path.
    fn real_shore(&self, path: &BorderPath, offsets: &[f64]) -> f64 {
        let end = *path.s.last().unwrap_or(&path.coast_s);
        let mut best = end;
        for &off in offsets {
            let mut s = (path.coast_s - 60_000.0).max(0.0);
            let mut wet = 0;
            while s < end {
                let ((x, z), (dx, dz)) = path.at(s);
                if self.height_no_lakes(x - dz * off, z + dx * off) < 0.0 {
                    wet += 1;
                    if wet == 3 {
                        best = best.min(s - 500.0);
                        break;
                    }
                } else {
                    wet = 0;
                }
                s += 250.0;
            }
        }
        best
    }

    /// How far the canyon has cut down here (0 outside it).
    pub fn canyon_cut(&self, wx: f64, wz: f64) -> f64 {
        let p = self.map_pt(wx, wz);
        match self.canyon.path.nearest(wx, wz, p.r) {
            Some((d, s)) => self.canyon.carve(&self.island.noise, wx, wz, d, s),
            None => 0.0,
        }
    }

    /// The river water level here, if a river runs here.
    fn river_water(&self, p: &MapPt, wx: f64, wz: f64) -> Option<f64> {
        for rv in &self.rivers {
            if let Some((d, s)) = rv.locate(&self.island.noise, wx, wz, p.r) {
                if d < rv.half_width(s) * 1.3 && s < rv.path.coast_s + 3_000.0 {
                    return Some(rv.level_at(s));
                }
            }
        }
        None
    }

    /// A sea stack (a rock pillar standing in the sea off the cliffs): its
    /// height here, if there is one.
    fn sea_stack(&self, p: &MapPt, wx: f64, wz: f64, shore: f64) -> Option<f64> {
        let cw = cliff_w(p.deg) * sm(230.0, 300.0, p.r);
        let off = -shore;
        if cw <= 0.0 || !(60.0..4_000.0).contains(&off) {
            return None;
        }
        const CELL: f64 = 700.0;
        let (ci, cj) = ((wx / CELL).floor() as i64, (wz / CELL).floor() as i64);
        let mut best: Option<f64> = None;
        for dj in -1..=1 {
            for di in -1..=1 {
                let mut r = Rng::new(((ci + di) as u64).wrapping_mul(73_856_093) ^ ((cj + dj) as u64).wrapping_mul(19_349_663) ^ 0x5eed);
                if r.next() > 0.28 {
                    continue;
                }
                let sx = ((ci + di) as f64 + 0.2 + 0.6 * r.next()) * CELL;
                let sz = ((cj + dj) as f64 + 0.2 + 0.6 * r.next()) * CELL;
                let rad = 50.0 + 100.0 * r.next();
                let top = CLIFF_M * (0.25 + 0.75 * r.next()) * cw;
                let d = (wx - sx).hypot(wz - sz);
                let v = top * sm(rad * 1.2, rad * 0.8, d) - 2.0;
                if v > 0.0 {
                    best = Some(best.map_or(v, |b: f64| b.max(v)));
                }
            }
        }
        best
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
            let shore = Self::shore_dist(&p);
            let floor = Self::sea_floor(shore);
            return match self.sea_stack(&p, wx, wz, shore) {
                Some(top) => top.max(floor),
                None => floor,
            };
        }
        self.land_height(&p, wx, wz)
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
    /// How much the two giant peaks shade the point (x, y, z) from a light
    /// in direction `l` (toward the light): 0 = lit, 1 = fully shaded. The
    /// same sum as the terrain shader (haze.wgsl), so the player's own light
    /// matches the shadows drawn on the land.
    pub fn giant_shadow(&self, p: [f64; 3], l: [f64; 3]) -> f64 {
        let len = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt().max(1e-9);
        let l = [l[0] / len, l[1] / len, l[2] / len];
        let lh2 = (l[0] * l[0] + l[2] * l[2]).max(1e-6);
        let lhl = lh2.sqrt();
        let mut shade: f64 = 0.0;
        for sp in self.shadow_peaks() {
            let t_mid = ((sp.x - p[0]) * l[0] + (sp.z - p[2]) * l[2]) / lh2;
            if t_mid <= 0.0 {
                continue;
            }
            // how far the light's path passes beside the peak
            let (ax, az) = (p[0] + l[0] * t_mid - sp.x, p[2] + l[2] * t_mid - sp.z);
            if ax.hypot(az) > sp.reach() {
                continue;
            }
            let r = sp.reach() / lhl;
            let (t0, t1) = ((t_mid - r).max(3000.0), t_mid + r);
            let mut worst = f64::MIN;
            for i in 0..SHADOW_STEPS {
                let t = t0 + (t1 - t0) * (i as f64 + 0.5) / SHADOW_STEPS as f64;
                let q = [p[0] + l[0] * t, p[1] + l[1] * t, p[2] + l[2] * t];
                worst = worst.max(sp.top((q[0] - sp.x).hypot(q[2] - sp.z)) - q[1]);
            }
            let soft = 400.0 + 0.004 * t_mid;
            shade = shade.max(sm(-soft, soft, worst));
        }
        shade
    }

    /// Simple shapes of every big peak (the giants and their range), for
    /// working out the long shadows they cast. The terrain shader gets the
    /// same list.
    pub fn shadow_peaks(&self) -> Vec<ShadowPeak> {
        let mut v = vec![
            ShadowPeak { x: self.everest.0, z: self.everest.1, spire_m: self.ev_spire_m, spire_r: EV_SPIRE_R, body_m: EV_MASSIF_M, body_r: EV_MASSIF_R, body_p: EV_MASSIF_P },
            ShadowPeak { x: self.helens.0, z: self.helens.1, spire_m: self.hel_m, spire_r: HEL_R, body_m: 0.0, body_r: 1.0, body_p: 1.0 },
        ];
        v.extend(self.peaks.iter().map(|pk| ShadowPeak { x: pk.x, z: pk.z, spire_m: pk.spire_m, spire_r: pk.spire_r, body_m: pk.body_m, body_r: pk.body_r, body_p: 1.6 }));
        v
    }

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
            let mut h = self.land_height(&p, wx, wz);
            let (x, y) = (p.x, p.y);
            let mut wr = [0.0; 6];
            self.island.region_weights(p.deg, x, y, &mut wr);
            sharpen(&mut wr, 2);
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
            s.patch = n.fbm(wx / 220.0 + 41.0, wz / 220.0 - 17.0, 2) as f32;
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
            // the North's highlands are bare grey rock (rock mines), in
            // layered bands, with lichen on the lower slopes
            mix!([128.0, 125.0, 122.0], wr[0] * sm(700.0, 1600.0, h) * 0.75);
            if wr[0] > 0.05 && h > 600.0 {
                let band = (h / 140.0 + 3.0 * n.n2(wx / 4_000.0, wz / 4_000.0 + 6.0)).sin();
                mix!(if band > 0.0 { [150.0, 146.0, 140.0] } else { [98.0, 96.0, 96.0] }, wr[0] * 0.22 * band.abs());
                mix!([120.0, 128.0, 92.0], wr[0] * 0.3 * sm(0.25, 0.5, s.patch as f64) * (1.0 - sm(2_200.0, 3_200.0, h)));
            }
            // the Northeast's mesas are red sandstone in stripes
            if wr[1] > 0.05 && h > 850.0 {
                mix!([186.0, 108.0, 68.0], wr[1] * 0.55 * sm(900.0, 1_150.0, h));
                let strata = (h / 32.0 + 2.0 * n.n2(wx / 3_000.0 + 9.0, wz / 3_000.0)).sin();
                mix!(if strata > 0.0 { [214.0, 150.0, 100.0] } else { [140.0, 78.0, 52.0] }, wr[1] * 0.3 * strata.abs() * sm(900.0, 1_150.0, h));
            }
            // the Southwest: golden dry patches among the green
            mix!([196.0, 182.0, 104.0], wr[4] * 0.35 * sm(-0.1, 0.25, n.fbm(wx / 1_800.0 + 5.0, wz / 1_800.0, 2)));
            // the Northwest: dark peat and mud
            mix!([70.0, 66.0, 48.0], wr[5] * 0.3 * sm(0.1, 0.35, s.patch as f64) * (1.0 - sm(900.0, 1_500.0, h)));
            // high ground: tan, then grey rock, then snow (by height)
            mix!([176.0, 160.0, 118.0], sm(2200.0, 3600.0, h) * 0.85);
            mix!([150.0, 146.0, 140.0], sm(3400.0, 4700.0, h));
            // on the giants and their range: darker rock higher up, snow in
            // broad fields broken by bare rock ribs, whiter toward the tops
            let mut snow = sm(4900.0, 5700.0, h + 300.0 * n.n2(wx / 3000.0, wz / 3000.0));
            if h > 6_000.0 {
                mix!([112.0, 108.0, 108.0], 0.55 * sm(8_000.0, 24_000.0, h));
                let fields = sm(-0.2, 0.15, n.fbm(wx / 22_000.0 + 31.0, wz / 22_000.0, 3) + (h - 6_000.0) / 45_000.0);
                let ribs = sm(0.6, 0.8, n.ridged(wx / 6_000.0 + 17.0, wz / 6_000.0, 3)) * (1.0 - sm(40_000.0, 90_000.0, h));
                snow *= fields * (1.0 - 0.85 * ribs);
            }
            mix!([236.0, 241.0, 250.0], snow);
            // St. Helens' ash and crater
            let d2 = (x - HX).hypot(y - HY);
            mix!([132.0, 96.0, 72.0], 0.8 * (-(d2 / 46.0).powf(2.2)).exp());
            mix!([70.0, 52.0, 42.0], 0.7 * (-(d2 / 11.0).powi(2)).exp());
            // white cliffs along the east coast, layered
            let cw = cliff_w(p.deg) * sm(230.0, 300.0, p.r) * (1.0 - sm(400.0, 2500.0, shore));
            if cw > 0.0 {
                let layer = (h / 45.0 + n.n2(wx / 2_000.0, wz / 2_000.0)).sin();
                let c = if layer > 0.6 { [196.0, 190.0, 176.0] } else { [230.0, 226.0, 214.0] };
                mix!(c, cw * 0.85);
                s.rock = s.rock.max((cw * sm(30.0, 90.0, h)) as f32);
            }
            // ground detail: sand, rock (grass where neither)
            s.sand = s.sand.max((wr[1] * (1.0 - sm(850.0, 1_150.0, h))) as f32);
            s.rock = s.rock.max((wr[0] * sm(700.0, 1_600.0, h) * 0.8 + wr[1] * sm(900.0, 1_150.0, h)).min(1.0) as f32);
            s.rock = s.rock.max((sm(3_400.0, 4_700.0, h) * (1.0 - snow)) as f32);
            // shores: beaches of each region's sand or stones, darker where wet
            let beach = sm(320.0 + 200.0 * n.n2(wx / 1_500.0, wz / 1_500.0 + 3.0), 40.0, shore) * sm(14.0, 4.0, h) * (1.0 - cw);
            if beach > 0.0 {
                const SHORE: [[f64; 3]; 6] = [
                    [150.0, 146.0, 138.0],
                    [74.0, 68.0, 66.0],
                    [224.0, 206.0, 150.0],
                    [120.0, 112.0, 96.0],
                    [232.0, 218.0, 172.0],
                    [108.0, 102.0, 76.0],
                ];
                let mut bc = [0.0; 3];
                for (k, w) in wr.iter().enumerate() {
                    for c in 0..3 {
                        bc[c] += w * SHORE[k][c];
                    }
                }
                mix!(bc, beach);
                mix!([bc[0] * 0.68, bc[1] * 0.68, bc[2] * 0.7], beach * sm(1.6, 0.4, h));
                s.sand = s.sand.max((beach * (wr[1] + wr[2] + wr[4])) as f32);
                s.rock = s.rock.max((beach * (wr[0] + wr[3]) * 0.7) as f32);
            }
            // the Great Canyon: walls of coloured rock in bands, dark old
            // rock in the inner gorge
            let cut = if self.features { self.canyon_cut(wx, wz) } else { 0.0 };
            if cut > 0.0 {
                s.canyon = true;
                const STRATA: [[f64; 3]; 5] = [
                    [184.0, 104.0, 70.0],
                    [214.0, 190.0, 150.0],
                    [166.0, 92.0, 64.0],
                    [198.0, 140.0, 96.0],
                    [150.0, 120.0, 104.0],
                ];
                let band = ((h / 1_150.0) + 0.6 * n.n2(wx / 20_000.0, wz / 20_000.0 + 2.0)).rem_euclid(5.0) as usize;
                let c = if h < -32_000.0 { [86.0, 78.0, 84.0] } else { STRATA[band.min(4)] };
                let t = sm(0.0, 300.0, cut);
                mix!(c, t);
                s.rock = s.rock.max(t as f32);
            }
            // rivers: gravel banks at the water's edge
            if self.features {
                if let Some(lv) = self.river_water(&p, wx, wz) {
                    mix!([150.0, 140.0, 112.0], 0.6 * sm(lv + 3.0, lv + 0.5, h));
                    if h < lv {
                        s.water = Some(lv);
                        s.river = true;
                    }
                }
            }
            // forest: the map's forests, broken up into woods and clearings
            let fine = n.fbm(wx / 3200.0 - 8.0, wz / 3200.0 + 2.0, 3);
            let fm_map = sm(1.0 - fd - 0.07, 1.0 - fd + 0.07, n.fbm(x / 60.0, y / 60.0, 3) * 1.4 + 0.5);
            let fm = (fm_map * sm(-0.35, 0.05, fine + fd * 0.6 - 0.3)) * (1.0 - sm(2600.0, 3400.0, h)) * sm(60.0, 400.0, shore) * (1.0 - sm(0.0, 300.0, cut));
            s.forest = fm as f32;
            // Southeast farmland: real-size fields
            let fw = wr[2] * (1.0 - fm) * (1.0 - sm(1200.0, 1800.0, h)) * sm(300.0, 1500.0, shore)
                * sm(-0.25, -0.05, n.fbm(x / 110.0 + 9.0, y / 110.0, 2))
                * (1.0 - sm(0.0, 300.0, cut));
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
                    s.hedge = fk > 0.5;
                }
                s.field = fk > 0.5;
            }
            if fm > 0.0 {
                mix!([f0, f1, f2], fm * 0.85);
                // the South's forest floor: patches of fallen leaves
                mix!([78.0, 62.0, 40.0], wr[3] * fm * 0.3 * sm(0.0, 0.35, s.patch as f64));
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
            if h < 0.6 && s.water.is_none() && !s.canyon {
                s.water = Some(0.0);
            }
            s.h = h;
        } else {
            s.h = Self::sea_floor(shore);
            let stack = self.sea_stack(&p, wx, wz, shore).filter(|&top| top > s.h);
            if let Some(top) = stack.filter(|&top| top > 0.5) {
                // a sea stack: the cliffs' white rock, in layers
                s.h = top;
                let layer = (top / 45.0).sin();
                let c = if layer > 0.6 { [196.0, 190.0, 176.0] } else { [230.0, 226.0, 214.0] };
                (cr, cg, cb) = (c[0], c[1], c[2]);
                s.rock = 1.0;
            } else {
                if let Some(top) = stack {
                    s.h = top;
                }
                let t = sm(0.0, 3000.0, -shore);
                cr = lerp(214.0, 120.0, t);
                cg = lerp(204.0, 130.0, t);
                cb = lerp(160.0, 120.0, t);
                s.water = Some(0.0);
            }
        }
        s.col = [(cr / 255.0).clamp(0.0, 1.0) as f32, (cg / 255.0).clamp(0.0, 1.0) as f32, (cb / 255.0).clamp(0.0, 1.0) as f32];
        s
    }

    /// How much each region (index into `REGIONS`) owns a spot; sums to 1.
    pub fn region_mix(&self, wx: f64, wz: f64) -> [f64; 6] {
        let p = self.map_pt(wx, wz);
        let mut wr = [0.0; 6];
        self.island.region_weights(p.deg, p.x, p.y, &mut wr);
        sharpen(&mut wr, 2);
        wr
    }

    /// Region name and description for a spot, as shown on the banner.
    pub fn place_name(&self, wx: f64, wz: f64) -> (&'static str, &'static str) {
        let (mx, my) = to_map(wx, wz);
        if self.island.land_at(mx, my) <= 0.0 && wx.hypot(wz) > 100_000.0 {
            return ("Rim Sea", "The bowl of water around the island");
        }
        let h = self.height(wx, wz);
        if (mx - EX).hypot(my - EY) < 40.0 && h > 40_000.0 {
            return ("Mount Everest", "The highest peak on the shard");
        }
        if (mx - HX).hypot(my - HY) < 30.0 && h > 15_000.0 {
            return ("Mount St. Helens", "The volcano beside Everest");
        }
        if let Some(dh) = self.bridge.deck_at(wx, wz) {
            if dh > h - 50.0 {
                return ("The Bridge City", "A city stretched across the canyon");
            }
        }
        if self.canyon_cut(wx, wz) > 3_000.0 {
            return ("The Great Canyon", "Between the Northeast and the Southeast");
        }
        let deg = (my - CY).atan2(mx - CX) / D2R;
        let g = &REGIONS[self.island.region_at(deg, mx, my)];
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
    fn everest_shades_the_land_behind_it() {
        let t = Terrain::new();
        let (ex, ez) = t.everest;
        // low evening sun in the west: east of Everest is in its shadow,
        // far west of it (beyond its range) is lit, and the summit is always lit
        let sun = [-1.0, 0.15, 0.0];
        let east = [ex + 300_000.0, t.height(ex + 300_000.0, ez) + 2.0, ez];
        let west = [ex - 900_000.0, t.height(ex - 900_000.0, ez) + 2.0, ez];
        let top = [ex, EVEREST_M + 10.0, ez];
        assert!(t.giant_shadow(east, sun) > 0.9);
        assert!(t.giant_shadow(west, sun) < 0.1);
        assert!(t.giant_shadow(top, sun) < 0.1);
        // high noon: no long shadows
        assert!(t.giant_shadow(east, [0.0, 1.0, 0.1]) < 0.1);
    }

    #[test]
    fn peaks_have_their_heights() {
        let t = Terrain::new();
        let (ex, ez) = to_world(EX, EY);
        let ev = t.height(ex, ez);
        assert!((ev - EVEREST_M).abs() < 15.0, "Everest {ev}");
        // Everest is the highest point anywhere (its range included)
        let mut top: f64 = 0.0;
        for j in -90..=90 {
            for i in -90..=90 {
                top = top.max(t.height(ex + i as f64 * 8000.0, ez + j as f64 * 8000.0));
            }
        }
        assert!(top <= EVEREST_M + 1.0, "higher than Everest: {top}");
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
        assert!((hel - ST_HELENS_M).abs() < 300.0, "St. Helens {hel}");
    }

    #[test]
    fn range_peaks_just_under_the_giants() {
        let t = Terrain::new();
        for (k, pk) in t.peaks.iter().enumerate() {
            // the highest point near each range peak
            let mut top: f64 = 0.0;
            for j in -6..=6 {
                for i in -6..=6 {
                    top = top.max(t.height(pk.x + i as f64 * 2_000.0, pk.z + j as f64 * 2_000.0));
                }
            }
            if k < 6 {
                assert!((150_000.0..=EVEREST_M - 15_000.0).contains(&top), "Everest's range peak {k}: {top}");
            } else {
                assert!((40_000.0..ST_HELENS_M - 5_000.0).contains(&top), "St. Helens' range peak {k}: {top}");
            }
        }
    }

    #[test]
    fn sea_out_to_the_cloudwall() {
        let t = Terrain::new();
        assert!(t.height(CLOUD_R * 0.5, 0.0) < -1000.0);
        assert!(CLOUD_R - 1_554_000.0 > 2.95 * ISLAND_WIDTH);
    }
}
