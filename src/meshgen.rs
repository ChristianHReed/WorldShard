//! Engine-independent mesh data for the world: terrain, lava, the Rim Sea,
//! trees / rocks / mines (merged into map chunks) and the Cloudwall.
//! Any renderer (Bevy, wgpu, WebGL) can upload these arrays directly.
//!
//! Colours are *linear* RGBA (sRGB values raised to 2.2) so they can go
//! straight into a vertex colour attribute.

use crate::worldgen::grid::*;
use crate::worldgen::island::{to_map_deg, EX, EY, REGIONS};
use crate::worldgen::noise::{sm, Rng};
use crate::worldgen::World;

#[derive(Clone, Default, Debug)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

#[inline]
fn lin(c: f32) -> f32 {
    c.max(0.0).powf(2.2)
}
#[inline]
fn lin3(c: [f32; 3]) -> [f32; 4] {
    [lin(c[0]), lin(c[1]), lin(c[2]), 1.0]
}
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

// ---------------------------------------------------------------------
// Terrain
// ---------------------------------------------------------------------

/// The island and sea floor as one indexed grid mesh.
pub fn terrain_mesh(w: &World) -> MeshData {
    let mut m = MeshData::default();
    let n = &w.island.noise;
    for j in 0..GV {
        for i in 0..GV {
            let k = j * GV + i;
            let x = (-EXT + i as f64 * CELL) as f32;
            let z = (-EXT + j as f64 * CELL) as f32;
            m.positions.push([x, w.h[k], z]);
            let hl = w.h[j * GV + i.saturating_sub(1)];
            let hr = w.h[j * GV + (i + 1).min(GN)];
            let hd = w.h[j.saturating_sub(1) * GV + i];
            let hu = w.h[(j + 1).min(GN) * GV + i];
            let (nx, ny, nz) = (hl - hr, 2.0 * CELL as f32, hd - hu);
            let l = (nx * nx + ny * ny + nz * nz).sqrt();
            let nrm = [nx / l, ny / l, nz / l];
            m.normals.push(nrm);
            // colour: base + gentle variation, forest canopy, steep rock
            let mut c = w.col[k];
            let v = 0.92 + 0.12 * n.n2(x as f64 * 0.02, z as f64 * 0.02) as f32 + 0.06 * n.n2(x as f64 * 0.11 + 5.0, z as f64 * 0.11) as f32;
            c = [c[0] * v, c[1] * v, c[2] * v];
            let fo = w.forest[k];
            if fo > 0.0 {
                let d = 1.0 - 0.28 * fo;
                c = [c[0] * d, c[1] * d, c[2] * d];
            }
            if w.h[k] > 0.0 {
                let steep = 1.0 - sm(0.55, 0.8, nrm[1] as f64) as f32;
                c = mix3(c, [0.47, 0.44, 0.40], steep * 0.55 * (1.0 - w.glow[k]));
            }
            m.colors.push(lin3(c));
        }
    }
    for j in 0..GN {
        for i in 0..GN {
            let a = (j * GV + i) as u32;
            let b = a + 1;
            let c = a + GV as u32;
            let d = c + 1;
            m.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    m
}

/// Glowing lava channels: a thin skin just above the ground wherever the
/// lava core is. Draw it unlit (vertex colours are bright, > 1).
pub fn lava_mesh(w: &World) -> MeshData {
    let mut m = MeshData::default();
    let glow_col = |g: f32| -> [f32; 4] { [3.2 * g, 0.85 * g, 0.12 * g, 1.0] };
    for j in 0..GN {
        for i in 0..GN {
            let ks = [j * GV + i, j * GV + i + 1, (j + 1) * GV + i, (j + 1) * GV + i + 1];
            if ks.iter().all(|&k| w.glow[k] < 0.2) {
                continue;
            }
            let base = m.positions.len() as u32;
            for (q, &k) in ks.iter().enumerate() {
                let (ii, jj) = (i + (q & 1), j + (q >> 1));
                let x = (-EXT + ii as f64 * CELL) as f32;
                let z = (-EXT + jj as f64 * CELL) as f32;
                m.positions.push([x, w.h[k] + 0.35, z]);
                m.normals.push([0.0, 1.0, 0.0]);
                m.colors.push(glow_col(w.glow[k]));
            }
            m.indices.extend_from_slice(&[base, base + 2, base + 1, base + 1, base + 2, base + 3]);
        }
    }
    m
}

// ---------------------------------------------------------------------
// Rim Sea
// ---------------------------------------------------------------------

/// The sea surface (y = 0), coloured by depth, cut off at the bowl's rim.
pub fn water_mesh(w: &World) -> MeshData {
    const WN: usize = 220;
    let we = SEA_R + 20.0;
    let step = 2.0 * we / WN as f64;
    let mut m = MeshData::default();
    for j in 0..=WN {
        for i in 0..=WN {
            let x = -we + i as f64 * step;
            let z = -we + j as f64 * step;
            let d = (-w.height_at(x, z)).max(0.0) as f32;
            let r = x.hypot(z);
            let shallow = [0.50, 0.80, 0.78];
            let mid = [0.15, 0.47, 0.62];
            let deep = [0.05, 0.20, 0.38];
            let mut c = mix3(shallow, mid, sm(0.0, 6.0, d as f64) as f32);
            c = mix3(c, deep, sm(6.0, 40.0, d as f64) as f32);
            c = mix3(c, [0.035, 0.14, 0.27], sm(SEA_R - 400.0, SEA_R, r) as f32);
            c = mix3(c, [0.93, 0.96, 0.95], (1.0 - sm(0.15, 1.4, d as f64) as f32) * 0.7);
            m.positions.push([x as f32, 0.0, z as f32]);
            m.normals.push([0.0, 1.0, 0.0]);
            m.colors.push(lin3(c));
        }
    }
    let row = (WN + 1) as u32;
    for j in 0..WN {
        for i in 0..WN {
            let cx = -we + (i as f64 + 0.5) * step;
            let cz = -we + (j as f64 + 0.5) * step;
            if cx.hypot(cz) > SEA_R {
                continue;
            }
            let a = j as u32 * row + i as u32;
            let (b, c) = (a + 1, a + row);
            let d = c + 1;
            m.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    m
}

// ---------------------------------------------------------------------
// Props: trees, bushes, rocks, mine entrances
// ---------------------------------------------------------------------

/// Flat-shaded triangle soup with a per-vertex "take the instance tint" flag.
#[derive(Default, Clone)]
pub struct Geo {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    c: Vec<[f32; 3]>,
    tint: Vec<bool>,
}

impl Geo {
    fn tri(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], col: [f32; 3], tint: bool) {
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut nn = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let l = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2]).sqrt().max(1e-9);
        nn = [nn[0] / l, nn[1] / l, nn[2] / l];
        for p in [a, b, c] {
            self.p.push(p);
            self.n.push(nn);
            self.c.push(col);
            self.tint.push(tint);
        }
    }
    pub fn triangle_count(&self) -> usize {
        self.p.len() / 3
    }
}

fn scale3(c: [f32; 3], s: f32) -> [f32; 3] {
    [c[0] * s, c[1] * s, c[2] * s]
}

fn cone(g: &mut Geo, y0: f32, y1: f32, r: f32, sides: usize, col: [f32; 3], tint: bool, jit: f32, rng: &mut Rng) {
    let mut ring = Vec::with_capacity(sides);
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let rr = r * (1.0 + (rng.next() as f32 - 0.5) * jit);
        ring.push([a.cos() * rr, y0 + (rng.next() as f32 - 0.5) * jit * r * 0.3, a.sin() * rr]);
    }
    let apex = [(rng.next() as f32 - 0.5) * jit * r * 0.3, y1, (rng.next() as f32 - 0.5) * jit * r * 0.3];
    let base = [0.0, y0, 0.0];
    for i in 0..sides {
        let (a, b) = (ring[i], ring[(i + 1) % sides]);
        let sh = 0.9 + rng.next() as f32 * 0.2;
        g.tri(a, apex, b, scale3(col, sh), tint);
        g.tri(a, b, base, scale3(col, 0.7), tint);
    }
}

fn cylinder(g: &mut Geo, y0: f32, y1: f32, r0: f32, r1: f32, sides: usize, col: [f32; 3], tint: bool) {
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / sides as f32 * std::f32::consts::TAU;
        let p0 = [a.cos() * r0, y0, a.sin() * r0];
        let p1 = [b.cos() * r0, y0, b.sin() * r0];
        let p2 = [b.cos() * r1, y1, b.sin() * r1];
        let p3 = [a.cos() * r1, y1, a.sin() * r1];
        g.tri(p0, p3, p1, col, tint);
        g.tri(p1, p3, p2, col, tint);
    }
}

fn icosphere(subdiv: u32) -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut v: Vec<[f32; 3]> = [
        [-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0],
        [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t],
        [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0],
    ]
    .iter()
    .map(|p| {
        let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        [p[0] / l, p[1] / l, p[2] / l]
    })
    .collect();
    let mut f: Vec<[usize; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    for _ in 0..subdiv {
        let mut cache = std::collections::HashMap::new();
        let mut nf = Vec::with_capacity(f.len() * 4);
        let mut mid = |a: usize, b: usize, v: &mut Vec<[f32; 3]>| -> usize {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&i) = cache.get(&key) {
                return i;
            }
            let m = [(v[a][0] + v[b][0]) / 2.0, (v[a][1] + v[b][1]) / 2.0, (v[a][2] + v[b][2]) / 2.0];
            let l = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
            v.push([m[0] / l, m[1] / l, m[2] / l]);
            cache.insert(key, v.len() - 1);
            v.len() - 1
        };
        for &[a, b, c] in &f {
            let ab = mid(a, b, &mut v);
            let bc = mid(b, c, &mut v);
            let ca = mid(c, a, &mut v);
            nf.extend_from_slice(&[[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
        }
        f = nf;
    }
    (v, f)
}

#[allow(clippy::too_many_arguments)]
fn blob(g: &mut Geo, cx: f32, cy: f32, cz: f32, rad: f32, sy: f32, col: [f32; 3], tint: bool, jit: f32, rng: &mut Rng, subdiv: u32) {
    let (v, f) = icosphere(subdiv);
    let vv: Vec<[f32; 3]> = v
        .iter()
        .map(|p| {
            let k = 1.0 + (rng.next() as f32 - 0.5) * jit;
            [cx + p[0] * rad * k, cy + p[1] * rad * sy * k, cz + p[2] * rad * k]
        })
        .collect();
    for &[a, b, c] in &f {
        let sh = 0.88 + rng.next() as f32 * 0.24;
        g.tri(vv[a], vv[b], vv[c], scale3(col, sh), tint);
    }
}

#[allow(clippy::too_many_arguments)]
fn cuboid(g: &mut Geo, cx: f32, cy: f32, cz: f32, sx: f32, sy: f32, sz: f32, col: [f32; 3], tint: bool) {
    let (x0, x1, y0, y1, z0, z1) = (cx - sx / 2.0, cx + sx / 2.0, cy - sy / 2.0, cy + sy / 2.0, cz - sz / 2.0, cz + sz / 2.0);
    let mut q = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| {
        g.tri(a, b, c, col, tint);
        g.tri(a, c, d, col, tint);
    };
    q([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]);
    q([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]);
    q([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]);
    q([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]);
    q([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]);
    q([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Pine,
    Broad,
    Dead,
    Bush,
    Rock,
    Mine,
}

/// The six prop shapes (placeholder art until real assets go in).
pub struct PropKinds {
    pub pine: Geo,
    pub broad: Geo,
    pub dead: Geo,
    pub bush: Geo,
    pub rock: Geo,
    pub mine: Geo,
}

impl PropKinds {
    pub fn build() -> Self {
        let mut r = Rng::new(4242);
        let white = [1.0, 1.0, 1.0];
        let bark = [0.36, 0.25, 0.17];
        let grey = [0.42, 0.40, 0.37];
        let wood = [0.42, 0.29, 0.18];
        let mut pine = Geo::default();
        cylinder(&mut pine, 0.0, 3.0, 0.35, 0.22, 6, bark, false);
        cone(&mut pine, 2.0, 7.0, 2.6, 7, white, true, 0.15, &mut r);
        cone(&mut pine, 4.2, 9.5, 2.0, 7, white, true, 0.15, &mut r);
        cone(&mut pine, 6.5, 11.5, 1.3, 6, white, true, 0.15, &mut r);
        let mut broad = Geo::default();
        cylinder(&mut broad, 0.0, 4.0, 0.42, 0.28, 6, bark, false);
        blob(&mut broad, 0.0, 6.2, 0.0, 3.4, 0.85, white, true, 0.25, &mut r, 1);
        blob(&mut broad, 1.3, 5.2, 0.8, 2.0, 0.8, scale3(white, 0.92), true, 0.25, &mut r, 0);
        let mut dead = Geo::default();
        cylinder(&mut dead, 0.0, 7.0, 0.35, 0.12, 5, grey, false);
        cuboid(&mut dead, 0.9, 4.5, 0.0, 1.8, 0.18, 0.18, grey, false);
        cuboid(&mut dead, -0.7, 5.6, 0.3, 1.4, 0.16, 0.16, grey, false);
        blob(&mut dead, 0.0, 7.0, 0.0, 0.9, 0.6, white, true, 0.4, &mut r, 0);
        let mut bush = Geo::default();
        blob(&mut bush, 0.0, 0.7, 0.0, 1.3, 0.75, white, true, 0.3, &mut r, 0);
        let mut rock = Geo::default();
        blob(&mut rock, 0.0, 0.35, 0.0, 1.6, 0.65, white, true, 0.45, &mut r, 1);
        let mut mine = Geo::default();
        blob(&mut mine, 0.0, 1.0, -5.5, 8.0, 0.75, [0.95, 0.95, 0.95], true, 0.35, &mut r, 1);
        cuboid(&mut mine, -2.1, 2.0, 0.6, 0.45, 4.0, 0.45, wood, false);
        cuboid(&mut mine, 2.1, 2.0, 0.6, 0.45, 4.0, 0.45, wood, false);
        cuboid(&mut mine, 0.0, 4.15, 0.6, 5.2, 0.5, 0.6, wood, false);
        cuboid(&mut mine, 0.0, 1.9, 0.1, 3.75, 3.8, 0.4, [0.04, 0.035, 0.03], false);
        cuboid(&mut mine, -0.8, 0.05, 5.0, 0.12, 0.1, 9.0, [0.3, 0.3, 0.32], false);
        cuboid(&mut mine, 0.8, 0.05, 5.0, 0.12, 0.1, 9.0, [0.3, 0.3, 0.32], false);
        for i in 0..6 {
            cuboid(&mut mine, 0.0, 0.0, 1.5 + i as f32 * 1.4, 2.2, 0.12, 0.3, wood, false);
        }
        cuboid(&mut mine, 2.6, 0.7, 6.0, 1.6, 1.0, 2.2, [0.33, 0.30, 0.28], false);
        PropKinds { pine, broad, dead, bush, rock, mine }
    }
    fn get(&self, k: PropKind) -> &Geo {
        match k {
            PropKind::Pine => &self.pine,
            PropKind::Broad => &self.broad,
            PropKind::Dead => &self.dead,
            PropKind::Bush => &self.bush,
            PropKind::Rock => &self.rock,
            PropKind::Mine => &self.mine,
        }
    }
}

/// One placed prop.
#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub kind: PropKind,
    pub pos: [f32; 3],
    pub scale: f32,
    pub rot: f32,
    pub tint: [f32; 3],
}

/// Mine entrances in the North (world metres).
pub fn mine_sites() -> Vec<(f64, f64)> {
    [(-112.0, 190.0), (-74.0, 175.0), (-100.0, 305.0), (-66.0, 300.0)]
        .iter()
        .map(|&(d, r)| {
            let (mx, my) = crate::worldgen::island::pol(d, r);
            to_world(mx, my)
        })
        .collect()
}

/// Where a new game starts: the Southwest grasslands, looking at the peaks.
pub fn spawn_point() -> (f64, f64) {
    let (mx, my) = crate::worldgen::island::pol(150.0, 232.0);
    to_world(mx, my)
}

/// Places trees, bushes, rocks and mines by region, as in the prototype:
/// South dark forest, Northwest swampy forest, Southeast forest patches and
/// open fields, Southwest open grassland, North rocks and mines, Northeast
/// desert rocks and a few dead trees.
pub fn scatter(w: &World) -> Vec<Prop> {
    let mut r = Rng::new(777);
    let mut out = Vec::new();
    let mines = mine_sites();
    let near_mine = |x: f64, z: f64| mines.iter().any(|&(a, b)| (a - x).hypot(b - z) < 22.0);
    let (evx, evz) = to_world(EX, EY);
    let (spx, spz) = spawn_point();
    const STEP: f64 = 7.0;
    let mut z = -EXT + 4.0;
    while z < EXT - 4.0 {
        let mut x = -EXT + 4.0;
        while x < EXT - 4.0 {
            let jx = x + (r.next() - 0.5) * STEP;
            let jz = z + (r.next() - 0.5) * STEP;
            x += STEP;
            let Some(k) = w.grid_index(jx, jz) else { continue };
            let (l, e, h) = (w.l[k], w.e[k], w.h[k]);
            if l < 0.02 || h < 2.0 || w.wet[k] > 0.3 || near_mine(jx, jz) || (jx - spx).hypot(jz - spz) < 10.0 {
                continue;
            }
            let (mx, my) = to_map(jx, jz);
            if w.lava.crust_at(mx, my) > 0.0 {
                continue;
            }
            let mut add = |kind: PropKind, s: f64, tint: [f32; 3], r: &mut Rng| {
                let y = w.height_at(jx, jz) - 0.15;
                let v = 0.9 + r.next() as f32 * 0.2;
                out.push(Prop {
                    kind,
                    pos: [jx as f32, y as f32, jz as f32],
                    scale: s as f32,
                    rot: (r.next() * std::f64::consts::TAU) as f32,
                    tint: scale3(tint, v),
                });
            };
            let q = r.next();
            let d_ev = (mx - EX).hypot(my - EY);
            if e > 0.72 || d_ev < 95.0 {
                if q < 0.02 {
                    let s = 1.0 + r.next() * 2.5;
                    add(PropKind::Rock, s, [0.62, 0.6, 0.57], &mut r);
                }
                continue;
            }
            let slope = (w.height_at(jx + 3.0, jz) - w.height_at(jx - 3.0, jz)).abs() + (w.height_at(jx, jz + 3.0) - w.height_at(jx, jz - 3.0)).abs();
            if slope > 7.0 {
                if q < 0.05 {
                    let s = 0.8 + r.next() * 2.0;
                    add(PropKind::Rock, s, [0.55, 0.52, 0.48], &mut r);
                }
                continue;
            }
            let fm = w.forest[k] as f64;
            match w.region[k] {
                0 => {
                    // North: rock mines
                    if q < 0.035 {
                        let s = 0.7 + r.next() * 2.2;
                        add(PropKind::Rock, s, [0.62, 0.6, 0.56], &mut r);
                    } else if q < 0.045 {
                        let s = 0.7 + r.next() * 0.4;
                        add(PropKind::Pine, s, [0.24, 0.34, 0.24], &mut r);
                    }
                }
                1 => {
                    // Northeast: lava flows and desert
                    if q < 0.012 {
                        let s = 0.6 + r.next() * 1.8;
                        add(PropKind::Rock, s, [0.3, 0.26, 0.24], &mut r);
                    } else if q < 0.02 {
                        let s = 0.5 + r.next() * 1.2;
                        add(PropKind::Rock, s, [0.74, 0.62, 0.45], &mut r);
                    } else if q < 0.023 {
                        let s = 0.6 + r.next() * 0.4;
                        add(PropKind::Dead, s, [0.55, 0.5, 0.42], &mut r);
                    }
                }
                2 => {
                    // Southeast: mid forest with fields
                    if w.field[k] {
                        if q < 0.004 {
                            let s = 0.8 + r.next() * 0.4;
                            add(PropKind::Broad, s, [0.36, 0.5, 0.22], &mut r);
                        }
                    } else if fm > 0.4 && q < 0.26 * fm {
                        let kind = if r.next() < 0.75 { PropKind::Broad } else { PropKind::Pine };
                        let s = 0.8 + r.next() * 0.5;
                        add(kind, s, [0.27, 0.44, 0.2], &mut r);
                    } else if q < 0.015 {
                        let s = 0.7 + r.next() * 0.6;
                        add(PropKind::Bush, s, [0.32, 0.46, 0.22], &mut r);
                    }
                }
                3 => {
                    // South: dark forest
                    if q < 0.22 * fm.max(0.5) {
                        let kind = if r.next() < 0.65 { PropKind::Pine } else { PropKind::Broad };
                        let s = 0.9 + r.next() * 0.6;
                        let tint = if r.next() < 0.5 { [0.11, 0.21, 0.13] } else { [0.15, 0.25, 0.14] };
                        add(kind, s, tint, &mut r);
                    } else if q < 0.26 {
                        let s = 0.6 + r.next() * 0.5;
                        add(PropKind::Bush, s, [0.17, 0.27, 0.15], &mut r);
                    }
                }
                4 => {
                    // Southwest: grasslands
                    if q < 0.004 {
                        let s = 0.8 + r.next() * 0.5;
                        add(PropKind::Broad, s, [0.38, 0.5, 0.22], &mut r);
                    } else if q < 0.016 {
                        let s = 0.5 + r.next() * 0.6;
                        add(PropKind::Bush, s, [0.42, 0.52, 0.24], &mut r);
                    } else if q < 0.019 {
                        let s = 0.4 + r.next();
                        add(PropKind::Rock, s, [0.62, 0.6, 0.55], &mut r);
                    }
                }
                _ => {
                    // Northwest: lakes and swampy forest
                    if fm > 0.3 && q < 0.2 * fm {
                        let p = r.next();
                        let kind = if p < 0.6 { PropKind::Broad } else if p < 0.85 { PropKind::Dead } else { PropKind::Pine };
                        let s = 0.75 + r.next() * 0.45;
                        add(kind, s, [0.30, 0.38, 0.22], &mut r);
                    } else if q < 0.03 {
                        let s = 0.6 + r.next() * 0.6;
                        add(PropKind::Bush, s, [0.32, 0.4, 0.24], &mut r);
                    } else if q < 0.036 {
                        let s = 0.6 + r.next() * 0.4;
                        add(PropKind::Dead, s, [0.45, 0.43, 0.38], &mut r);
                    }
                }
            }
        }
        z += STEP;
    }
    for (x, z) in mines {
        let face = (x - evx).atan2(z - evz); // face away from Everest
        let y = w.height_at(x, z) - 0.3;
        out.push(Prop { kind: PropKind::Mine, pos: [x as f32, y as f32, z as f32], scale: 1.0, rot: -face as f32, tint: [0.6, 0.58, 0.55] });
    }
    out
}

/// Side length of a prop chunk in metres, and chunks per side.
pub const CHUNK: f64 = 240.0;
pub const NCHUNK: usize = 16;

/// A merged block of props for one map chunk.
pub struct PropChunk {
    /// Centre of the chunk (x, z) in metres.
    pub centre: [f32; 2],
    pub mesh: MeshData,
}

/// Merges all props into one mesh per chunk (few draw calls, easy culling).
pub fn prop_chunks(props: &[Prop], kinds: &PropKinds) -> Vec<PropChunk> {
    let mut chunks: Vec<MeshData> = vec![MeshData::default(); NCHUNK * NCHUNK];
    for p in props {
        let ci = (((p.pos[0] as f64 + EXT) / CHUNK).floor() as isize).clamp(0, NCHUNK as isize - 1) as usize;
        let cj = (((p.pos[2] as f64 + EXT) / CHUNK).floor() as isize).clamp(0, NCHUNK as isize - 1) as usize;
        let m = &mut chunks[cj * NCHUNK + ci];
        let g = kinds.get(p.kind);
        let (c, s) = (p.rot.cos(), p.rot.sin());
        let base = m.positions.len() as u32;
        for v in 0..g.p.len() {
            let q = g.p[v];
            let (x, y, z) = (q[0] * p.scale, q[1] * p.scale, q[2] * p.scale);
            m.positions.push([p.pos[0] + c * x - s * z, p.pos[1] + y, p.pos[2] + s * x + c * z]);
            let nv = g.n[v];
            m.normals.push([c * nv[0] - s * nv[2], nv[1], s * nv[0] + c * nv[2]]);
            let col = if g.tint[v] { [g.c[v][0] * p.tint[0], g.c[v][1] * p.tint[1], g.c[v][2] * p.tint[2]] } else { g.c[v] };
            m.colors.push(lin3(col));
            m.indices.push(base + v as u32);
        }
    }
    chunks
        .into_iter()
        .enumerate()
        .filter(|(_, m)| !m.is_empty())
        .map(|(i, mesh)| {
            let (ci, cj) = (i % NCHUNK, i / NCHUNK);
            PropChunk { centre: [(-EXT + (ci as f64 + 0.5) * CHUNK) as f32, (-EXT + (cj as f64 + 0.5) * CHUNK) as f32], mesh }
        })
        .collect()
}

// ---------------------------------------------------------------------
// Cloudwall
// ---------------------------------------------------------------------

/// Cloud puffs around the rim: (centre, radius).
pub fn cloud_puffs() -> Vec<([f32; 3], f32)> {
    let mut r = Rng::new(5150);
    let mut out = Vec::new();
    for layer in 0..7 {
        let y = -20.0 + layer as f64 * 70.0;
        let n = if layer < 5 { 120 } else { 80 - layer * 6 };
        for i in 0..n {
            let a = (i as f64 + r.next() * 0.8) / n as f64 * std::f64::consts::TAU;
            for ring in 0..2 {
                let rr = 2280.0 + ring as f64 * 150.0 + (r.next() - 0.5) * 120.0 + 40.0 * (a * 5.0 + layer as f64).sin();
                let size = (if layer < 5 { 110.0 } else { 85.0 }) + r.next() * 70.0 - ring as f64 * 10.0;
                let yy = y + (r.next() - 0.5) * 40.0 + if ring == 1 { 20.0 } else { 0.0 };
                out.push(([(a.cos() * rr) as f32, yy as f32, (a.sin() * rr) as f32], size as f32));
            }
        }
    }
    out
}

/// The Cloudwall as `sectors` merged meshes of squashed, lumpy spheres,
/// lighter on top and greyer underneath.
pub fn cloud_meshes(sectors: usize) -> Vec<MeshData> {
    let mut r = Rng::new(616);
    let (sv, sf) = icosphere(1);
    let mut out: Vec<MeshData> = vec![MeshData::default(); sectors];
    for (c, rad) in cloud_puffs() {
        let a = (c[2].atan2(c[0]) as f64).rem_euclid(std::f64::consts::TAU);
        let s = ((a / std::f64::consts::TAU * sectors as f64) as usize).min(sectors - 1);
        let m = &mut out[s];
        let base = m.positions.len() as u32;
        let bright = 0.9 + r.next() as f32 * 0.12;
        for p in &sv {
            let k = 1.0 + (r.next() as f32 - 0.5) * 0.25;
            m.positions.push([c[0] + p[0] * rad * k, c[1] + p[1] * rad * 0.62 * k, c[2] + p[2] * rad * k]);
            m.normals.push(*p);
            let shade = (0.72 + 0.28 * (p[1] * 0.5 + 0.5)) * bright;
            m.colors.push(lin3([shade, shade, shade * 1.02]));
        }
        for f in &sf {
            m.indices.extend_from_slice(&[base + f[0] as u32, base + f[1] as u32, base + f[2] as u32]);
        }
    }
    out
}

/// Region index for a world position (for HUD / gameplay).
pub fn region_at(wx: f64, wz: f64) -> &'static str {
    let (mx, my) = to_map(wx, wz);
    REGIONS[crate::worldgen::island::region_index(to_map_deg(mx, my))].name
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn icosphere_faces() {
        assert_eq!(icosphere(0).1.len(), 20);
        assert_eq!(icosphere(1).1.len(), 80);
    }
}
