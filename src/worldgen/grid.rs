//! The playable world as a height grid in metres, with per-vertex ground
//! colour and material hints. Same recipe as the JS prototype's `generate()`.

use super::island::*;
use super::noise::{lerp, sm, Rng};

/// Metres per design-map pixel.
pub const METRES_PER_PX: f64 = 4.0;
/// Grid cells per side, half-extent in metres.
pub const GN: usize = 480;
pub const EXT: f64 = 1920.0;
pub const CELL: f64 = 2.0 * EXT / GN as f64;
pub const GV: usize = GN + 1;
/// Height of elevation 1.0, in metres.
pub const HEIGHT: f64 = 430.0;
/// Radius of the Rim Sea and of the invisible wall before the Cloudwall.
pub const SEA_R: f64 = 2330.0;
pub const WALL_R: f64 = 2150.0;

pub fn to_map(wx: f64, wz: f64) -> (f64, f64) {
    (CX + wx / METRES_PER_PX, CY + wz / METRES_PER_PX)
}
pub fn to_world(mx: f64, my: f64) -> (f64, f64) {
    ((mx - CX) * METRES_PER_PX, (my - CY) * METRES_PER_PX)
}

/// Lava painted in map pixels: dark crust and glowing core.
pub struct Lava {
    pub crust: Vec<f32>,
    pub core: Vec<f32>,
}

impl Lava {
    fn at(arr: &[f32], mx: f64, my: f64) -> f64 {
        let (x, y) = (mx.round() as i64, my.round() as i64);
        if x < 0 || y < 0 || x >= MW as i64 || y >= MH as i64 {
            0.0
        } else {
            arr[y as usize * MW + x as usize] as f64
        }
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
    for &(a0, r0, ph) in &[
        (-53.0, 120.0, 1.3),
        (-43.0, 135.0, 5.1),
        (-19.0, 128.0, 8.7),
        (-8.0, 140.0, 12.2),
    ] {
        let main = add_flow(a0, r0, ph, &mut flows);
        if main > 40 {
            add_flow(
                a0 + if a0 < -30.0 { 5.0 } else { -5.0 },
                r0 + main as f64 * 1.5,
                ph + 20.0,
                &mut flows,
            );
        }
    }
    let mut lava = Lava {
        crust: vec![0.0; MW * MH],
        core: vec![0.0; MW * MH],
    };
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

/// North-west lakes: (x, z, radius) in metres.
fn build_lakes(isl: &Island) -> Vec<(f64, f64, f64)> {
    let mut r2 = Rng::new(991);
    let mut lakes = Vec::new();
    let mut t = 0;
    while t < 400 && lakes.len() < 9 {
        t += 1;
        let (mx, my) = pol(186.0 + r2.next() * 50.0, 130.0 + r2.next() * 200.0);
        if isl.land_at(mx, my) < 0.06 {
            continue;
        }
        let (wx, wz) = to_world(mx, my);
        lakes.push((wx, wz, (5.0 + r2.next() * 8.0) * METRES_PER_PX * 1.1));
    }
    lakes
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

/// The generated world.
pub struct World {
    pub island: Island,
    pub lava: Lava,
    pub lakes: Vec<(f64, f64, f64)>,
    /// Height in metres per grid vertex (GV x GV, row = z).
    pub h: Vec<f32>,
    /// Landness, elevation (0..~1.4).
    pub l: Vec<f32>,
    pub e: Vec<f32>,
    /// Ground colour 0–1, linear in the order r, g, b.
    pub col: Vec<[f32; 3]>,
    /// Lava glow, forest canopy amount, wetness (pools and lakes).
    pub glow: Vec<f32>,
    pub forest: Vec<f32>,
    pub wet: Vec<f32>,
    /// Dominant region index into `REGIONS`.
    pub region: Vec<u8>,
    pub field: Vec<bool>,
}

impl World {
    /// Builds the whole world. `progress` gets 0..1 as rows finish.
    pub fn generate(mut progress: impl FnMut(f32)) -> World {
        let island = Island::new();
        let lava = build_lava(&island);
        let lakes = build_lakes(&island);
        let nv = GV * GV;
        let mut w = World {
            island,
            lava,
            lakes,
            h: vec![0.0; nv],
            l: vec![0.0; nv],
            e: vec![0.0; nv],
            col: vec![[0.0; 3]; nv],
            glow: vec![0.0; nv],
            forest: vec![0.0; nv],
            wet: vec![0.0; nv],
            region: vec![0; nv],
            field: vec![false; nv],
        };
        for j in 0..GV {
            for i in 0..GV {
                let k = j * GV + i;
                let v = vertex(&w.island, &w.lava, &w.lakes, i, j);
                w.h[k] = v.h;
                w.l[k] = v.l;
                w.e[k] = v.e;
                w.col[k] = v.col;
                w.glow[k] = v.glow;
                w.forest[k] = v.forest;
                w.wet[k] = v.wet;
                w.region[k] = v.region;
                w.field[k] = v.field;
            }
            if j % 24 == 0 {
                progress(j as f32 / GV as f32);
            }
        }
        progress(1.0);
        w
    }
}

/// Everything computed for one grid vertex.
struct Vert {
    h: f32,
    l: f32,
    e: f32,
    col: [f32; 3],
    glow: f32,
    forest: f32,
    wet: f32,
    region: u8,
    field: bool,
}

fn vertex(isl: &Island, lava: &Lava, lakes: &[(f64, f64, f64)], i: usize, j: usize) -> Vert {
    let mut out = Vert {
        h: 0.0,
        l: 0.0,
        e: 0.0,
        col: [0.0; 3],
        glow: 0.0,
        forest: 0.0,
        wet: 0.0,
        region: 0,
        field: false,
    };
    let wx = -EXT + i as f64 * CELL;
    let wz = -EXT + j as f64 * CELL;
    let (x, y) = to_map(wx, wz);
    let (dx, dy) = (x - CX, y - CY);
    let r = dx.hypot(dy);
    let deg = dy.atan2(dx) / D2R;
    let n = &isl.noise;
    let l = isl.land_value(x, y, r, deg);
    out.l = l as f32;
    let (mut cr, mut cg, mut cb);
    let (mut em, mut fo, mut wet) = (0.0f64, 0.0f64, 0.0f64);
    if l > 0.0 {
        let e = isl.elevation(x, y, r, deg, l);
        out.e = e as f32;
        let mut h = 1.2 + e * HEIGHT;
        let mut wr = [0.0; 6];
        isl.region_weights(deg, x, y, &mut wr);
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
        out.region = best as u8;
        let patch = 1.0 + 0.07 * n.fbm(x / 45.0 + 3.0, y / 45.0, 3);
        cr *= patch;
        cg *= patch;
        cb *= patch;
        let mx = |c: [f64; 3], t: f64, cr: &mut f64, cg: &mut f64, cb: &mut f64| {
            *cr += (c[0] - *cr) * t;
            *cg += (c[1] - *cg) * t;
            *cb += (c[2] - *cb) * t;
        };
        macro_rules! mix {
            ($c:expr, $t:expr) => {
                mx($c, $t, &mut cr, &mut cg, &mut cb)
            };
        }
        mix!([176.0, 160.0, 118.0], sm(0.66, 0.95, e) * 0.85);
        mix!([190.0, 183.0, 168.0], sm(0.95, 1.12, e));
        let d1 = (x - EX).hypot(y - EY);
        let d2 = (x - HX).hypot(y - HY);
        if d1 < 190.0 {
            mix!(
                [250.0, 250.0, 247.0],
                sm(0.98, 1.12, e) * (1.0 - sm(110.0, 175.0, d1))
            );
        }
        mix!([132.0, 96.0, 72.0], 0.8 * (-(d2 / 46.0).powf(2.2)).exp());
        mix!([70.0, 52.0, 42.0], 0.7 * (-(d2 / 11.0).powi(2)).exp());
        let cw = cliff_w(deg) * (1.0 - sm(0.025, 0.07, l));
        mix!([226.0, 221.0, 208.0], cw * 0.75);
        let fm = sm(
            1.0 - fd - 0.07,
            1.0 - fd + 0.07,
            n.fbm(x / 60.0, y / 60.0, 3) * 1.4 + 0.5,
        ) * (1.0 - sm(0.55, 0.75, e))
            * sm(0.01, 0.04, l);
        fo = fm;
        // Southeast fields
        let fw = wr[2]
            * (1.0 - fm)
            * (1.0 - sm(0.45, 0.6, e))
            * sm(0.02, 0.06, l)
            * sm(-0.25, -0.05, n.fbm(x / 110.0 + 9.0, y / 110.0, 2));
        if fw > 0.01 {
            let u = x * 0.88 + y * 0.47;
            let v = -x * 0.47 + y * 0.88;
            let (cu, cv) = ((u / 26.0).floor(), (v / 18.0).floor());
            let hv = n.perm(
                (n.perm((cu as i64 & 255) as usize) as usize + (cv as i64 & 255) as usize) & 511,
            ) as f64
                / 255.0;
            let (fu, fv) = (u / 26.0 - cu, v / 18.0 - cv);
            let fc = FIELD_COLS[((hv * 6.0).floor() as usize) % 6];
            let fk = sm(0.12, 0.35, fw);
            mix!(fc, fk * 0.95);
            if fu < 0.06 || fv < 0.08 {
                mix!([72.0, 90.0, 48.0], fk * 0.8);
            }
            if fk > 0.5 {
                out.field = true;
            }
        }
        if fm > 0.0 {
            mix!([f0, f1, f2], fm * 0.85);
        }
        mix!(
            [228.0, 216.0, 170.0],
            (1.0 - sm(0.004, 0.016, l)) * 0.7 * (1.0 - cliff_w(deg))
        );
        // Northwest pools and lakes
        let pw = wr[5] * (1.0 - sm(0.4, 0.58, e)) * sm(0.03, 0.08, l);
        if pw > 0.02 {
            let pn = n.fbm(x / 26.0 + 70.0, y / 26.0, 3);
            mix!([92.0, 118.0, 96.0], pw * 0.22);
            let pool = sm(0.17, 0.21, pn) * pw;
            let edge = (sm(0.13, 0.17, pn) - sm(0.17, 0.21, pn)) * pw;
            mix!([70.0, 92.0, 74.0], edge * 0.6);
            mix!([92.0, 128.0, 132.0], pool * 0.92);
            wet = wet.max(pool);
        }
        for &(lx, lz, lr) in lakes {
            let dd = ((wx - lx) / 1.3).hypot((wz - lz) / 0.8) / lr;
            if dd < 1.15 {
                let t = 1.0 - sm(0.85, 1.05, dd);
                mix!([88.0, 128.0, 140.0], t);
                wet = wet.max(t);
                if dd < 1.0 {
                    h -= 1.5 * t;
                }
            }
        }
        // Northeast lava
        let crust = lava.crust_at(x, y);
        let core = lava.core_at(x, y);
        if crust > 0.0 {
            mix!([52.0, 40.0, 34.0], crust * 0.92);
        }
        if core > 0.0 {
            mix!([200.0, 90.0, 30.0], core);
            em = core;
        }
        out.h = h as f32;
    } else {
        out.h = (-1.0 + 170.0 * l).max(-60.0) as f32;
        let t = sm(0.0, -0.08, l);
        cr = lerp(214.0, 120.0, t);
        cg = lerp(204.0, 130.0, t);
        cb = lerp(160.0, 120.0, t);
    }
    out.col = [
        (cr / 255.0) as f32,
        (cg / 255.0) as f32,
        (cb / 255.0) as f32,
    ];
    out.glow = em as f32;
    out.forest = fo as f32;
    out.wet = wet as f32;
    out
}

impl World {
    /// Ground height in metres at a world position (bilinear).
    pub fn height_at(&self, wx: f64, wz: f64) -> f64 {
        let gx = (wx + EXT) / CELL;
        let gz = (wz + EXT) / CELL;
        if gx < 0.0 || gz < 0.0 || gx >= GN as f64 || gz >= GN as f64 {
            return -60.0;
        }
        let (i, j) = (gx.floor() as usize, gz.floor() as usize);
        let (fx, fz) = (gx - i as f64, gz - j as f64);
        let h = |a: usize| self.h[a] as f64;
        let a = h(j * GV + i);
        let b = h(j * GV + i + 1);
        let c = h((j + 1) * GV + i);
        let d = h((j + 1) * GV + i + 1);
        lerp(lerp(a, b, fx), lerp(c, d, fx), fz)
    }

    /// Nearest grid vertex for a world position, if inside the grid.
    pub fn grid_index(&self, wx: f64, wz: f64) -> Option<usize> {
        let i = ((wx + EXT) / CELL).round();
        let j = ((wz + EXT) / CELL).round();
        if i < 0.0 || j < 0.0 || i > GN as f64 || j > GN as f64 {
            None
        } else {
            Some(j as usize * GV + i as usize)
        }
    }

    /// Region name and description for a spot, as shown on the banner.
    pub fn place_name(&self, wx: f64, wz: f64) -> (&'static str, &'static str) {
        let r = wx.hypot(wz);
        let h = self.height_at(wx, wz);
        if h < -0.5 && r > 200.0 {
            return ("Rim Sea", "The bowl of water around the island");
        }
        let (mx, my) = to_map(wx, wz);
        if (mx - EX).hypot(my - EY) < 70.0 && h > 300.0 {
            return ("Mount Everest", "The highest peak on the shard");
        }
        if (mx - HX).hypot(my - HY) < 45.0 && h > 200.0 {
            return ("Mount St. Helens", "The volcano beside Everest");
        }
        let g = &REGIONS[region_index((my - CY).atan2(mx - CX) / D2R)];
        (g.name, g.desc)
    }
}
