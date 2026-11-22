//! Engine-independent mesh data for the full-size world: terrain chunks
//! (with water and lava), trees / rocks / mines and grass near the player,
//! the Cloudwall and the sky dome. Any renderer can upload these arrays.
//!
//! Colours are *linear* RGBA (sRGB raised to 2.2). Positions are relative to
//! each piece's own origin, so they stay precise in 32-bit floats even
//! thousands of kilometres from the centre.

use crate::lod::{NodeKey, CHUNK_N};
use crate::worldgen::island::{pol, EX, EY};
use crate::worldgen::noise::{sm, Noise, Rng};
use crate::worldgen::terrain::*;

#[derive(Clone, Default, Debug)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    /// Terrain only: how sandy and how rocky each point is (for the ground
    /// detail the terrain shader draws close up). Empty for other meshes.
    pub uvs: Vec<[f32; 2]>,
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    fn quad(&mut self, p: [[f32; 3]; 4], n: [f32; 3], c: [[f32; 4]; 4]) {
        let b = self.positions.len() as u32;
        for k in 0..4 {
            self.positions.push(p[k]);
            self.normals.push(n);
            self.colors.push(c[k]);
        }
        // corners: 0 = (x0,z0), 1 = (x1,z0), 2 = (x0,z1), 3 = (x1,z1)
        self.indices.extend_from_slice(&[b, b + 2, b + 1, b + 1, b + 2, b + 3]);
    }
    /// A flat face with corners `a b c d` in counter-clockwise order as
    /// seen from its front (`c` and `d` may be the same point: a triangle).
    pub fn face(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], col: [f32; 4]) {
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
        let n = norm3([u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]);
        self.quad([a, d, b, c], n, [col; 4]);
    }
}

#[inline]
fn lin(c: f32) -> f32 {
    c.max(0.0).powf(2.2)
}
#[inline]
pub(crate) fn lin3(c: [f32; 3]) -> [f32; 4] {
    [lin(c[0]), lin(c[1]), lin(c[2]), 1.0]
}
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
/// Richer, slightly deeper colours for the land, closer to the lush
/// painted look of Breath of the Wild than the flat map colours.
pub fn ground_grade(c: [f32; 3]) -> [f32; 3] {
    let l = c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11;
    let sat = 1.35;
    let g = [l + (c[0] - l) * sat, l + (c[1] - l) * sat, l + (c[2] - l) * sat];
    // nudge yellow-greens toward green
    let k = (c[1] - c[2]).max(0.0) * 0.25;
    [(g[0] - k * 0.6).max(0.0) * 0.93, g[1] * 0.95, (g[2] - k * 0.2).max(0.0) * 0.92]
}

fn scale3(c: [f32; 3], s: f32) -> [f32; 3] {
    [c[0] * s, c[1] * s, c[2] * s]
}
pub(crate) fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

// ---------------------------------------------------------------------
// Water colours
// ---------------------------------------------------------------------

fn sea_colour(depth: f32) -> [f32; 3] {
    let shallow = [0.42, 0.84, 0.80];
    let mid = [0.13, 0.52, 0.66];
    let deep = [0.04, 0.22, 0.42];
    let mut c = mix3(shallow, mid, sm(0.0, 8.0, depth as f64) as f32);
    c = mix3(c, deep, sm(8.0, 120.0, depth as f64) as f32);
    c = mix3(c, [0.03, 0.14, 0.30], sm(400.0, 3000.0, depth as f64) as f32);
    mix3(c, [0.93, 0.97, 0.96], (1.0 - sm(0.1, 0.9, depth as f64) as f32) * 0.65)
}

fn lake_colour(depth: f32) -> [f32; 3] {
    let c = mix3([0.45, 0.80, 0.70], [0.12, 0.45, 0.50], sm(0.0, 10.0, depth as f64) as f32);
    mix3(c, [0.90, 0.95, 0.90], (1.0 - sm(0.1, 0.7, depth as f64) as f32) * 0.5)
}

// ---------------------------------------------------------------------
// Terrain chunks
// ---------------------------------------------------------------------

/// Meshes for one LOD chunk. Positions are relative to `origin` (x, z).
pub struct ChunkMeshes {
    pub key: NodeKey,
    pub origin: [f64; 2],
    pub terrain: Option<MeshData>,
    pub water: Option<MeshData>,
    /// Glowing lava: draw unlit.
    pub lava: Option<MeshData>,
}

/// Builds one chunk. Open-sea chunks are a single water square.
pub fn build_chunk(t: &Terrain, key: NodeKey) -> ChunkMeshes {
    let (cx, cz) = key.centre();
    let size = key.size();
    let mut out = ChunkMeshes { key, origin: [cx, cz], terrain: None, water: None, lava: None };
    if key.ocean_only() {
        let h = (size / 2.0) as f32;
        let c = lin3(sea_colour(4000.0));
        let mut w = MeshData::default();
        w.quad([[-h, 0.0, -h], [h, 0.0, -h], [-h, 0.0, h], [h, 0.0, h]], [0.0, 1.0, 0.0], [c; 4]);
        out.water = Some(w);
        return out;
    }
    let n = crate::lod::chunk_n(&key);
    let cell = size / n as f64;
    let (x0, z0) = key.corner();
    let g = n + 3; // heights with a one-cell border, for normals
    let mut hb = vec![0f64; g * g];
    let mut samples = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..g {
        for i in 0..g {
            let x = x0 + (i as f64 - 1.0) * cell;
            let z = z0 + (j as f64 - 1.0) * cell;
            let inner = (1..=n + 1).contains(&i) && (1..=n + 1).contains(&j);
            if inner {
                let s = t.sample(x, z);
                hb[j * g + i] = s.h;
                samples.push(s);
            } else {
                hb[j * g + i] = t.height(x, z);
            }
        }
    }
    let v = n + 1;
    let s_at = |i: usize, j: usize| &samples[j * v + i];
    let h_at = |i: usize, j: usize| hb[(j + 1) * g + (i + 1)];
    let local = |i: usize, j: usize| -> [f32; 3] {
        [(x0 + i as f64 * cell - cx) as f32, h_at(i, j) as f32, (z0 + j as f64 * cell - cz) as f32]
    };

    // terrain surface
    let mut m = MeshData::default();
    let mut any_land = false;
    for j in 0..v {
        for i in 0..v {
            let s = s_at(i, j);
            if s.h > -2.0 || s.canyon || s.water.map_or(false, |w| w > 0.0) {
                any_land = true;
            }
            let land = s.h > 0.0 || s.canyon;
            let (hl, hr) = (hb[(j + 1) * g + i], hb[(j + 1) * g + i + 2]);
            let (hu, hd) = (hb[j * g + i + 1], hb[(j + 2) * g + i + 1]);
            let nrm = norm3([(hl - hr) as f32, (2.0 * cell) as f32, (hu - hd) as f32]);
            let mut c = if land && s.glow < 0.05 { ground_grade(s.col) } else { s.col };
            // hollows and valleys darker, crests a little lighter (soft shading)
            if land {
                let lap = ((hl + hr + hu + hd) / 4.0 - s.h) / (cell * 0.5);
                let occl = lap.clamp(-1.0, 1.0) as f32;
                c = scale3(c, 1.0 - 0.32 * occl.max(0.0) + 0.06 * (-occl).max(0.0));
            }
            // less of the sky reaches down into the canyon
            if s.canyon {
                c = scale3(c, 1.0 - 0.38 * (-s.h / 45_000.0).clamp(0.0, 1.0) as f32);
            }
            // steep slopes show rock; flat high ground keeps its grass or
            // snow (the canyon walls and white cliffs have their own rock)
            let mut steep_rock = 0.0f32;
            if s.h > 0.0 && !s.canyon && s.rock < 0.5 {
                // high up, rock shows through the snow on gentler slopes too
                let high = sm(3800.0, 4800.0, s.h) as f32;
                // the rocky North shows bare rock on gentler slopes too
                let steep_low = if s.region == 0 && s.h > 400.0 {
                    1.0 - sm(0.72, 0.93, nrm[1] as f64) as f32
                } else {
                    1.0 - sm(0.62, 0.86, nrm[1] as f64) as f32
                };
                let steep_high = 1.0 - sm(0.80, 0.95, nrm[1] as f64) as f32;
                let steep = steep_low + (steep_high - steep_low) * high;
                let rock = mix3([0.50, 0.47, 0.43], [0.46, 0.46, 0.52], high);
                c = mix3(c, rock, steep * 0.7 * (1.0 - s.glow));
                steep_rock = steep * 0.9;
            } else if s.canyon || s.rock >= 0.5 {
                steep_rock = 1.0 - sm(0.55, 0.85, nrm[1] as f64) as f32;
            }
            m.positions.push(local(i, j));
            m.normals.push(nrm);
            m.colors.push(lin3(c));
            m.uvs.push([s.sand, s.rock.max(steep_rock)]);
        }
    }
    for j in 0..n {
        for i in 0..n {
            let a = (j * v + i) as u32;
            let b = a + 1;
            let c = a + v as u32;
            let d = c + 1;
            m.indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    // skirts hide the cracks between chunks of different detail (deeper
    // on steep mountain chunks, where neighbours differ more)
    let (hmin, hmax) = hb.iter().fold((f64::MAX, f64::MIN), |(a, b), &h| (a.min(h), b.max(h)));
    let skirt = (cell * 2.0 + 4.0 + (hmax - hmin) * 0.12) as f32;
    // they lean outward, under the neighbour, so a crack can't be seen even
    // when the skirt is seen edge-on
    let flare = (cell * 0.5) as f32;
    let edges: [(Vec<(usize, usize)>, [f32; 2]); 4] = [
        ((0..v).map(|i| (i, 0)).collect(), [0.0, -1.0]),
        ((0..v).map(|i| (n, i)).collect(), [1.0, 0.0]),
        ((0..v).rev().map(|i| (i, n)).collect(), [0.0, 1.0]),
        ((0..v).rev().map(|i| (0, i)).collect(), [-1.0, 0.0]),
    ];
    for (edge, out) in &edges {
        for w in edge.windows(2) {
            let (i0, j0) = w[0];
            let (i1, j1) = w[1];
            let (a, b) = ((j0 * v + i0) as u32, (j1 * v + i1) as u32);
            let base = m.positions.len() as u32;
            for &k in &[a, b] {
                let mut p = m.positions[k as usize];
                p[1] -= skirt;
                p[0] += out[0] * flare;
                p[2] += out[1] * flare;
                m.positions.push(p);
                m.normals.push(m.normals[k as usize]);
                // the colour's alpha says how far below its edge this point
                // hangs (1 = on the surface; SKIRT_SCALE metres per unit),
                // so the terrain shader can light the skirt like its edge
                let mut c = m.colors[k as usize];
                c[3] = 1.0 - skirt / SKIRT_SCALE;
                m.colors.push(c);
                m.uvs.push(m.uvs[k as usize]);
            }
            // front face outward, so the side you see is lit like the ground
            // (seen from behind, its normal flips and it goes dark blue)
            m.indices.extend_from_slice(&[a, b, base, b, base + 1, base]);
        }
    }
    if any_land {
        out.terrain = Some(m);
    }

    // water (sea at 0 m, lakes at their own level) and glowing lava
    let mut wm = MeshData::default();
    let mut lava = MeshData::default();
    for j in 0..n {
        for i in 0..n {
            let corners = [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)];
            let level = corners.iter().filter_map(|&(a, b)| s_at(a, b).water).fold(f64::NAN, f64::max);
            if !level.is_nan() && corners.iter().any(|&(a, b)| h_at(a, b) < level) {
                let lake = level > 0.5;
                let mut p = [[0f32; 3]; 4];
                let mut c = [[0f32; 4]; 4];
                for (k, &(a, b)) in corners.iter().enumerate() {
                    let lv = s_at(a, b).water.unwrap_or(level);
                    let mut q = local(a, b);
                    q[1] = lv as f32;
                    p[k] = q;
                    let depth = (lv - h_at(a, b)).max(0.0) as f32;
                    c[k] = lin3(if lake { lake_colour(depth) } else { sea_colour(depth) });
                }
                wm.quad(p, [0.0, 1.0, 0.0], c);
            }
            // glowing lava only on detailed chunks (from afar it is in the ground colour)
            if cell <= 64.0 && corners.iter().any(|&(a, b)| s_at(a, b).glow > 0.2) {
                let lift = (0.4 + cell * 0.02) as f32;
                let mut p = [[0f32; 3]; 4];
                let mut c = [[0f32; 4]; 4];
                for (k, &(a, b)) in corners.iter().enumerate() {
                    let mut q = local(a, b);
                    q[1] += lift;
                    p[k] = q;
                    let gl = s_at(a, b).glow;
                    c[k] = [3.4 * gl, 0.9 * gl, 0.12 * gl, 1.0];
                }
                lava.quad(p, [0.0, 1.0, 0.0], c);
            }
        }
    }
    if !wm.is_empty() {
        out.water = Some(wm);
    }
    if !lava.is_empty() {
        out.lava = Some(lava);
    }
    out
}

/// The ground height exactly as the finest terrain chunks draw it
/// (8 m triangles), so the player stands on the visible surface.
pub fn drawn_height(t: &Terrain, x: f64, z: f64) -> f64 {
    let cell = crate::lod::ROOT_SIZE / (1u64 << crate::lod::MAX_LEVEL) as f64 / CHUNK_N as f64;
    let o = -crate::lod::ROOT_SIZE / 2.0;
    let (gx, gz) = ((x - o) / cell, (z - o) / cell);
    let (i, j) = (gx.floor(), gz.floor());
    let (u, w) = (gx - i, gz - j);
    let (xa, za) = (o + i * cell, o + j * cell);
    let a = t.height(xa, za);
    let b = t.height(xa + cell, za);
    let c = t.height(xa, za + cell);
    let d = t.height(xa + cell, za + cell);
    tri_interp(a, b, c, d, u, w)
}

#[inline]
fn tri_interp(a: f64, b: f64, c: f64, d: f64, u: f64, w: f64) -> f64 {
    // triangles (a, c, b) and (b, c, d), as in `build_chunk`
    if u + w <= 1.0 {
        a + u * (b - a) + w * (c - a)
    } else {
        d + (1.0 - u) * (c - d) + (1.0 - w) * (b - d)
    }
}

// ---------------------------------------------------------------------
// Prop shapes (placeholder art until real assets go in)
// ---------------------------------------------------------------------

/// Triangle soup with per-vertex normals and a "take the instance tint" flag.
#[derive(Default, Clone)]
pub struct Geo {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    c: Vec<[f32; 3]>,
    tint: Vec<bool>,
}

impl Geo {
    fn push(&mut self, p: [f32; 3], n: [f32; 3], c: [f32; 3], tint: bool) {
        self.p.push(p);
        self.n.push(n);
        self.c.push(c);
        self.tint.push(tint);
    }
    fn tri(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], col: [f32; 3], tint: bool) {
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let nn = norm3([u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]);
        for p in [a, b, c] {
            self.push(p, nn, col, tint);
        }
    }
    pub fn triangle_count(&self) -> usize {
        self.p.len() / 3
    }
}

/// Soft, top-lit shading: darker underneath, lighter on top.
fn top_lit(col: [f32; 3], y: f32, y0: f32, y1: f32) -> [f32; 3] {
    let t = ((y - y0) / (y1 - y0).max(0.01)).clamp(0.0, 1.0);
    scale3(col, 0.62 + 0.5 * t)
}

/// A cone with smooth normals (a conifer tier).
#[allow(clippy::too_many_arguments)]
fn cone(g: &mut Geo, y0: f32, y1: f32, r: f32, sides: usize, col: [f32; 3], tint: bool, jit: f32, rng: &mut Rng) {
    let slope = r / (y1 - y0);
    let mut ring = Vec::with_capacity(sides);
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let rr = r * (1.0 + (rng.next() as f32 - 0.5) * jit);
        ring.push(([a.cos() * rr, y0 + (rng.next() as f32 - 0.5) * jit * r * 0.3, a.sin() * rr], norm3([a.cos(), slope, a.sin()])));
    }
    let apex = [0.0, y1, 0.0];
    for i in 0..sides {
        let (a, na) = ring[i];
        let (b, nb) = ring[(i + 1) % sides];
        let nt = norm3([(na[0] + nb[0]) * 0.5, slope * 2.0, (na[2] + nb[2]) * 0.5]);
        g.push(a, na, top_lit(col, a[1], y0, y1), tint);
        g.push(apex, nt, top_lit(col, y1, y0, y1), tint);
        g.push(b, nb, top_lit(col, b[1], y0, y1), tint);
        g.tri(a, b, [0.0, y0 + (y1 - y0) * 0.1, 0.0], scale3(col, 0.5), tint);
    }
}

fn cylinder(g: &mut Geo, y0: f32, y1: f32, r0: f32, r1: f32, sides: usize, col: [f32; 3], tint: bool) {
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let b = (i + 1) as f32 / sides as f32 * std::f32::consts::TAU;
        let (na, nb) = ([a.cos(), 0.0, a.sin()], [b.cos(), 0.0, b.sin()]);
        let p0 = [a.cos() * r0, y0, a.sin() * r0];
        let p1 = [b.cos() * r0, y0, b.sin() * r0];
        let p2 = [b.cos() * r1, y1, b.sin() * r1];
        let p3 = [a.cos() * r1, y1, a.sin() * r1];
        let (c0, c1) = (scale3(col, 0.7), col);
        g.push(p0, na, c0, tint);
        g.push(p3, na, c1, tint);
        g.push(p1, nb, c0, tint);
        g.push(p1, nb, c0, tint);
        g.push(p3, na, c1, tint);
        g.push(p2, nb, c1, tint);
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
    .map(|p| norm3(*p))
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
            v.push(norm3([(v[a][0] + v[b][0]) / 2.0, (v[a][1] + v[b][1]) / 2.0, (v[a][2] + v[b][2]) / 2.0]));
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

/// A lumpy ball with smooth normals (tree canopy, bush) or flat faces (rock).
#[allow(clippy::too_many_arguments)]
fn blob(g: &mut Geo, cx: f32, cy: f32, cz: f32, rad: f32, sy: f32, col: [f32; 3], tint: bool, jit: f32, rng: &mut Rng, subdiv: u32, smooth: bool) {
    let (v, f) = icosphere(subdiv);
    let k: Vec<f32> = v.iter().map(|_| 1.0 + (rng.next() as f32 - 0.5) * jit).collect();
    let pos = |i: usize| [cx + v[i][0] * rad * k[i], cy + v[i][1] * rad * sy * k[i], cz + v[i][2] * rad * k[i]];
    let (y0, y1) = (cy - rad * sy, cy + rad * sy);
    for &[a, b, c] in &f {
        if smooth {
            for i in [a, b, c] {
                let p = pos(i);
                let nrm = norm3([v[i][0], v[i][1] / sy.max(0.2), v[i][2]]);
                g.push(p, nrm, top_lit(col, p[1], y0, y1), tint);
            }
        } else {
            let sh = 0.88 + rng.next() as f32 * 0.24;
            let (pa, pb, pc) = (pos(a), pos(b), pos(c));
            let mid_y = (pa[1] + pb[1] + pc[1]) / 3.0;
            g.tri(pa, pb, pc, scale3(top_lit(col, mid_y, y0, y1), sh), tint);
        }
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
    /// A building of the bridge city: drawn as a box with a roof, sized to it.
    House,
}

/// The six prop shapes.
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
        cone(&mut pine, 2.0, 7.0, 2.6, 8, white, true, 0.15, &mut r);
        cone(&mut pine, 4.2, 9.5, 2.0, 8, white, true, 0.15, &mut r);
        cone(&mut pine, 6.5, 11.5, 1.3, 7, white, true, 0.15, &mut r);
        let mut broad = Geo::default();
        cylinder(&mut broad, 0.0, 4.0, 0.42, 0.28, 6, bark, false);
        blob(&mut broad, 0.0, 6.2, 0.0, 3.4, 0.85, white, true, 0.22, &mut r, 1, true);
        blob(&mut broad, 1.4, 5.3, 0.9, 2.1, 0.8, scale3(white, 0.95), true, 0.22, &mut r, 1, true);
        blob(&mut broad, -1.2, 5.6, -0.7, 1.9, 0.8, scale3(white, 0.97), true, 0.22, &mut r, 0, true);
        let mut dead = Geo::default();
        cylinder(&mut dead, 0.0, 7.0, 0.35, 0.12, 5, grey, false);
        cuboid(&mut dead, 0.9, 4.5, 0.0, 1.8, 0.18, 0.18, grey, false);
        cuboid(&mut dead, -0.7, 5.6, 0.3, 1.4, 0.16, 0.16, grey, false);
        blob(&mut dead, 0.0, 7.0, 0.0, 0.9, 0.6, white, true, 0.4, &mut r, 0, true);
        let mut bush = Geo::default();
        blob(&mut bush, 0.0, 0.7, 0.0, 1.3, 0.75, white, true, 0.3, &mut r, 1, true);
        let mut rock = Geo::default();
        blob(&mut rock, 0.0, 0.35, 0.0, 1.6, 0.65, white, true, 0.45, &mut r, 1, false);
        let mut mine = Geo::default();
        blob(&mut mine, 0.0, 1.0, -5.5, 8.0, 0.75, [0.95, 0.95, 0.95], true, 0.35, &mut r, 1, false);
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
            PropKind::Mine | PropKind::House => &self.mine,
        }
    }
}

/// A distant stand-in for a building: walls and a pitched roof, turned
/// like the real model (front toward its street).
pub(crate) fn house_box(m: &mut MeshData, pos: [f32; 3], rot: f32, w: f32, l: f32, height: f32, tint: Tint) {
    let tower = w < 4.5 && l < 4.5 && height > 20.0;
    let wall_h = if tower { height - 6.8 } else { height - w * 0.55 - 0.5 };
    let walls = match tint {
        Tint::Dry => [0.86, 0.78, 0.62],
        Tint::Dark => [0.74, 0.74, 0.72],
        Tint::Desert => [0.84, 0.70, 0.52],
        _ => [0.88, 0.85, 0.78],
    };
    let roof = match tint {
        Tint::Dark => [0.36, 0.38, 0.44],
        Tint::Desert => [0.62, 0.32, 0.20],
        _ => [0.70, 0.30, 0.20],
    };
    // Bevy's turn about the up axis: x' = x cos + z sin, z' = -x sin + z cos
    let (c, s) = (rot.cos(), rot.sin());
    let tf = |p: [f32; 3]| [pos[0] + c * p[0] + s * p[2], pos[1] + p[1], pos[2] - s * p[0] + c * p[2]];
    let (hx, hz) = (w / 2.0, l / 2.0);
    let mut quad = |a: [f32; 3], b: [f32; 3], cc: [f32; 3], d: [f32; 3], col: [f32; 3]| {
        m.face(tf(a), tf(b), tf(cc), tf(d), lin3(col));
    };
    // four walls
    quad([-hx, 0.0, hz], [hx, 0.0, hz], [hx, wall_h, hz], [-hx, wall_h, hz], walls);
    quad([hx, 0.0, -hz], [-hx, 0.0, -hz], [-hx, wall_h, -hz], [hx, wall_h, -hz], scale3(walls, 0.9));
    quad([hx, 0.0, hz], [hx, 0.0, -hz], [hx, wall_h, -hz], [hx, wall_h, hz], scale3(walls, 0.8));
    quad([-hx, 0.0, -hz], [-hx, 0.0, hz], [-hx, wall_h, hz], [-hx, wall_h, -hz], scale3(walls, 0.85));
    if tower {
        // a pyramid roof
        let top = [0.0, height, 0.0];
        let (ex, ez) = (hx + 0.8, hz + 0.8);
        let e = [[-ex, wall_h, ez], [ex, wall_h, ez], [ex, wall_h, -ez], [-ex, wall_h, -ez]];
        for k in 0..4 {
            quad(e[k], e[(k + 1) % 4], top, top, roof);
        }
    } else {
        // a pitched roof with its ridge along the depth, and gable ends
        let ry = height;
        let (ex, ez) = (hx + 0.55, hz + 0.45);
        quad([ex, wall_h - 0.4, ez], [ex, wall_h - 0.4, -ez], [0.0, ry, -ez], [0.0, ry, ez], roof);
        quad([-ex, wall_h - 0.4, -ez], [-ex, wall_h - 0.4, ez], [0.0, ry, ez], [0.0, ry, -ez], scale3(roof, 0.85));
        quad([-hx, wall_h, hz], [hx, wall_h, hz], [0.0, ry, hz], [0.0, ry, hz], walls);
        quad([hx, wall_h, -hz], [-hx, wall_h, -hz], [0.0, ry, -hz], [0.0, ry, -hz], scale3(walls, 0.9));
    }
}

/// On (or right beside) the bridge city's deck, where nothing grows.
pub fn on_deck(t: &Terrain, x: f64, z: f64) -> bool {
    let b = &t.bridge;
    let (u, v) = b.to_deck(x, z);
    u.abs() < b.half_len + 60.0 && v.abs() < b.half_w + 20.0
}

/// The height the player stands at: the ground, or the bridge deck when
/// they are on it (and not far below it in the canyon).
pub fn stand_height(t: &Terrain, x: f64, z: f64, y: f64) -> f64 {
    let g = drawn_height(t, x, z);
    match t.bridge.deck_at(x, z) {
        Some(d) if y > d - 30.0 => d.max(g),
        _ => g,
    }
}

/// The ground (or the water, or the bridge deck) under someone at height
/// `y`: what "how high up" is measured from. The sea counts as level
/// ground; the canyon goes far below it.
pub fn ground_under(t: &Terrain, x: f64, z: f64, y: f64) -> f64 {
    let g = stand_height(t, x, z, y);
    if g < 0.0 && t.canyon_cut(x, z) > 1.0 {
        g
    } else {
        g.max(0.0)
    }
}

/// Walking on the bridge: the parapets keep you from stepping off its
/// sides where it crosses the canyon.
pub fn keep_on_deck(t: &Terrain, x0: f64, z0: f64, x1: f64, z1: f64, feet_y: f64) -> (f64, f64) {
    let b = &t.bridge;
    if b.deck_at(x0, z0).is_none() || feet_y < b.deck_y - 2.0 {
        return (x1, z1);
    }
    let (u, v) = b.to_deck(x1, z1);
    let lim = b.half_w - crate::bridge::PARAPET_T - 0.4;
    if u.abs() < b.span_half + 600.0 && v.abs() > lim {
        return b.from_deck(u, v.clamp(-lim, lim));
    }
    (x1, z1)
}

/// Mine entrances in the North (world metres).
pub fn mine_sites() -> Vec<(f64, f64)> {
    [(-112.0, 190.0), (-74.0, 175.0), (-100.0, 305.0), (-66.0, 300.0)]
        .iter()
        .map(|&(d, r)| {
            let (mx, my) = pol(d, r);
            to_world(mx, my)
        })
        .collect()
}

/// Where a new game starts: the Southwest grasslands, looking at the peaks.
pub fn spawn_point() -> (f64, f64) {
    let (mx, my) = pol(150.0, 232.0);
    to_world(mx, my)
}

fn cell_seed(ci: i64, cj: i64, salt: i64) -> u64 {
    let h = (ci.wrapping_mul(73_856_093) ^ cj.wrapping_mul(19_349_663) ^ salt.wrapping_mul(83_492_791)).rem_euclid(2_147_483_645);
    h as u64 + 1
}

fn add_geo(m: &mut MeshData, g: &Geo, pos: [f32; 3], scale: f32, rot: f32, tint: [f32; 3]) {
    let (c, s) = (rot.cos(), rot.sin());
    let base = m.positions.len() as u32;
    for v in 0..g.p.len() {
        let q = g.p[v];
        let (x, y, z) = (q[0] * scale, q[1] * scale, q[2] * scale);
        m.positions.push([pos[0] + c * x - s * z, pos[1] + y, pos[2] + s * x + c * z]);
        let nv = g.n[v];
        m.normals.push([c * nv[0] - s * nv[2], nv[1], s * nv[0] + c * nv[2]]);
        let col = if g.tint[v] { [g.c[v][0] * tint[0], g.c[v][1] * tint[1], g.c[v][2] * tint[2]] } else { g.c[v] };
        m.colors.push(lin3(col));
        m.indices.push(base + v as u32);
    }
}

// ---------------------------------------------------------------------
// Real models (Quaternius Stylized Nature MegaKit, CC0) and where they go
// ---------------------------------------------------------------------

/// A model the game loads from `assets/models/<name>.gltf`. `parts` names
/// the material of each mesh part, in order. `height` is the model's own
/// height in metres; `far` is the placeholder drawn for it in the distance.
pub struct ModelInfo {
    pub name: &'static str,
    pub parts: &'static [&'static str],
    pub height: f32,
    pub far: Option<PropKind>,
}

const TREE: &[&str] = &["Bark_NormalTree", "Leaves_NormalTree"];
const PINE: &[&str] = &["Bark_NormalTree", "Leaves_Pine"];
const TWISTED: &[&str] = &["Bark_TwistedTree", "Leaves_TwistedTree"];
const DEAD: &[&str] = &["Bark_DeadTree"];
const ROCK: &[&str] = &["Rocks"];
const LEAVES: &[&str] = &["Leaves"];
const FLOWER: &[&str] = &["Leaves", "Flowers"];
const GRASS: &[&str] = &["Grass"];
const PEBBLE: &[&str] = &["PathRocks"];
/// The bridge city's buildings (parts as written by tools/prepare_village.py).
pub const VILLAGE: &[&str] = &["MI_WoodTrim", "MI_Plaster", "MI_UnevenBrick", "MI_RoundTiles", "MI_WindowGlass", "MI_RockTrim", "MI_Brick", "MI_MetalOrnaments"];

macro_rules! m {
    ($n:expr, $p:expr, $h:expr, $f:expr) => {
        ModelInfo { name: $n, parts: $p, height: $h, far: $f }
    };
}

pub const MODELS: &[ModelInfo] = &[
    m!("CommonTree_1", TREE, 7.26, Some(PropKind::Broad)),
    m!("CommonTree_2", TREE, 7.64, Some(PropKind::Broad)),
    m!("CommonTree_3", TREE, 9.43, Some(PropKind::Broad)),
    m!("CommonTree_4", TREE, 9.44, Some(PropKind::Broad)),
    m!("CommonTree_5", TREE, 7.01, Some(PropKind::Broad)),
    m!("Pine_1", PINE, 7.32, Some(PropKind::Pine)),
    m!("Pine_2", PINE, 7.38, Some(PropKind::Pine)),
    m!("Pine_3", PINE, 7.39, Some(PropKind::Pine)),
    m!("Pine_4", PINE, 10.24, Some(PropKind::Pine)),
    m!("Pine_5", PINE, 8.72, Some(PropKind::Pine)),
    m!("TwistedTree_2", TWISTED, 18.95, Some(PropKind::Broad)),
    m!("TwistedTree_5", TWISTED, 15.66, Some(PropKind::Broad)),
    m!("DeadTree_1", DEAD, 9.5, Some(PropKind::Dead)),
    m!("DeadTree_3", DEAD, 13.28, Some(PropKind::Dead)),
    m!("DeadTree_5", DEAD, 16.44, Some(PropKind::Dead)),
    m!("Bush_Common", &["Leaves_TwistedTree"], 1.58, Some(PropKind::Bush)),
    m!("Bush_Common_Flowers", &["Leaves_NormalTree", "Flowers"], 1.58, Some(PropKind::Bush)),
    m!("Rock_Medium_1", ROCK, 2.26, Some(PropKind::Rock)),
    m!("Rock_Medium_2", ROCK, 1.9, Some(PropKind::Rock)),
    m!("Rock_Medium_3", ROCK, 2.32, Some(PropKind::Rock)),
    m!("Fern_1", LEAVES, 2.69, None),
    m!("Plant_1_Big", LEAVES, 3.76, None),
    m!("Plant_7_Big", LEAVES, 0.25, None),
    m!("Flower_3_Group", FLOWER, 2.05, None),
    m!("Flower_4_Group", FLOWER, 2.49, None),
    m!("Flower_3_Single", FLOWER, 2.07, None),
    m!("Flower_4_Single", FLOWER, 2.42, None),
    m!("Grass_Common_Short", GRASS, 1.33, None),
    m!("Grass_Common_Tall", GRASS, 1.87, None),
    m!("Grass_Wispy_Short", GRASS, 1.07, None),
    m!("Grass_Wispy_Tall", GRASS, 1.67, None),
    m!("Clover_1", LEAVES, 1.14, None),
    m!("Clover_2", LEAVES, 1.26, None),
    m!("Mushroom_Common", &["Mushrooms"], 0.46, None),
    m!("Mushroom_Laetiporus", &["Mushrooms"], 0.77, None),
    m!("Pebble_Round_1", PEBBLE, 0.1, None),
    m!("Pebble_Round_2", PEBBLE, 0.09, None),
    m!("Pebble_Round_3", PEBBLE, 0.1, None),
    m!("Pebble_Square_1", PEBBLE, 0.13, None),
    m!("Pebble_Square_2", PEBBLE, 0.14, None),
    // the bridge city (same order as crate::city::BUILDINGS)
    m!("House_4x6_2", VILLAGE, 9.4, Some(PropKind::House)),
    m!("House_4x8_3", VILLAGE, 12.18, Some(PropKind::House)),
    m!("House_6x6_2", VILLAGE, 10.62, Some(PropKind::House)),
    m!("House_6x8_2", VILLAGE, 10.62, Some(PropKind::House)),
    m!("House_6x8_3", VILLAGE, 13.62, Some(PropKind::House)),
    m!("House_6x10_2", VILLAGE, 10.62, Some(PropKind::House)),
    m!("House_8x8_3", VILLAGE, 14.55, Some(PropKind::House)),
    m!("House_8x12_2", VILLAGE, 11.55, Some(PropKind::House)),
    m!("Tower_4x4_6", VILLAGE, 24.79, Some(PropKind::House)),
    m!("Tower_4x4_9", VILLAGE, 33.79, Some(PropKind::House)),
    m!("Prop_Wagon", VILLAGE, 1.53, None),
    m!("Prop_Crate", VILLAGE, 1.06, None),
];

/// Indexes into `MODELS`.
pub mod model {
    pub const COMMON_TREE: u16 = 0; // 5 variants
    pub const PINE: u16 = 5; // 5
    pub const TWISTED: u16 = 10; // 2
    pub const DEAD: u16 = 12; // 3
    pub const BUSH: u16 = 15;
    pub const BUSH_FLOWERS: u16 = 16;
    pub const ROCK: u16 = 17; // 3
    pub const FERN: u16 = 20;
    pub const PLANT_BIG: u16 = 21;
    pub const LILY: u16 = 22;
    pub const FLOWER: u16 = 23; // 4
    pub const GRASS: u16 = 27; // 4: common short, common tall, wispy short, wispy tall
    pub const CLOVER: u16 = 31; // 2
    pub const MUSHROOM: u16 = 33; // 2
    pub const PEBBLE: u16 = 35; // 5
    /// The bridge city's buildings (crate::city::BUILDINGS, in order).
    pub const BUILDING: u16 = 40; // 12
    /// Not a model: a mine entrance, drawn from the placeholder shapes.
    pub const MINE: u16 = u16::MAX;
}

/// Colour variations of the same material, by where a plant grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tint {
    Normal,
    /// South dark forest
    Dark,
    /// Northwest swamp woods
    Swamp,
    /// Dry grassland and desert edges
    Dry,
    /// Northeast sandstone
    Desert,
}

/// One placed model. `pos` is relative to its cell's centre.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub model: u16,
    pub pos: [f32; 3],
    pub rot: f32,
    pub scale: f32,
    pub tint: Tint,
}

/// Skirt depth encoding in the terrain's vertex colour alpha (see haze.wgsl).
pub const SKIRT_SCALE: f32 = 100_000.0;

/// Side of a prop cell in metres (trees and rocks are streamed in cells).
pub const PROP_CELL: f64 = 64.0;
/// Side of a small-plant cell in metres (flowers, ferns, grass clumps).
pub const DETAIL_CELL: f64 = 32.0;

/// Picks a tree, bush or rock for a spot, by region.
fn choose_prop(s: &Sample, q: f64, pick: f64, size: f64) -> Option<(u16, f64, Tint)> {
    use model::*;
    let fm = s.forest as f64;
    let v = |base: u16, n: u16| base + ((pick * 997.0) as u16 % n);
    if s.h > 4600.0 {
        return (q < 0.02).then(|| (v(ROCK, 3), 0.8 + size * 2.0, Tint::Normal));
    }
    match s.region {
        // North: rock mines. Boulder fields, scattered rocks, and pines
        // only in the sheltered low valleys
        0 => {
            if s.patch > 0.25 && q < 0.07 {
                Some((v(ROCK, 3), 1.2 + size * 2.6, Tint::Normal))
            } else if q < 0.025 {
                Some((v(ROCK, 3), 0.4 + size * 1.6, Tint::Normal))
            } else if s.h < 1_900.0 && q < 0.04 {
                Some((v(PINE, 5), 1.2 + size * 0.6, Tint::Normal))
            } else {
                None
            }
        }
        // Northeast: lava flows and desert. Sandstone rocks in groups,
        // dead trees, and a few dry bushes
        1 => {
            if s.patch > 0.35 && q < 0.04 {
                Some((v(ROCK, 3), 0.6 + size * 2.2, Tint::Desert))
            } else if q < 0.012 {
                Some((v(ROCK, 3), 0.35 + size * 1.4, Tint::Desert))
            } else if q < 0.016 {
                Some((v(DEAD, 3), 0.6 + size * 0.4, Tint::Dry))
            } else if s.h < 1_000.0 && q < 0.022 {
                Some((BUSH, 0.5 + size * 0.4, Tint::Dry))
            } else {
                None
            }
        }
        // Southeast: mid forest with fields
        2 => {
            if s.hedge {
                // hedgerows along the field edges, with the odd tree
                if q < 0.05 {
                    Some((v(COMMON_TREE, 5), 1.1 + size * 0.5, Tint::Normal))
                } else if q < 0.26 {
                    let m = if pick < 0.3 { BUSH_FLOWERS } else { BUSH };
                    Some((m, 1.1 + size * 0.6, Tint::Normal))
                } else {
                    None
                }
            } else if s.field {
                (q < 0.003).then(|| (v(COMMON_TREE, 5), 1.4 + size * 0.4, Tint::Normal))
            } else if fm > 0.35 && q < 0.26 * fm {
                let m = if pick < 0.75 { v(COMMON_TREE, 5) } else { v(PINE, 5) };
                Some((m, 1.3 + size * 0.5, Tint::Normal))
            } else if q < 0.015 {
                let m = if pick < 0.5 { BUSH_FLOWERS } else { BUSH };
                Some((m, 0.8 + size * 0.5, Tint::Normal))
            } else {
                None
            }
        }
        // South: dark forest
        3 => {
            // old, tall, close-packed trees, many gnarled
            if q < 0.24 * fm.max(0.4) {
                let m = if pick < 0.55 {
                    v(PINE, 5)
                } else if pick < 0.85 {
                    v(COMMON_TREE, 5)
                } else {
                    v(TWISTED, 2)
                };
                Some((m, 1.6 + size * 0.9, Tint::Dark))
            } else if q < 0.26 {
                Some((BUSH, 0.7 + size * 0.5, Tint::Dark))
            } else {
                None
            }
        }
        // Southwest: grasslands
        4 => {
            // open grass with lone trees and the odd copse
            if q < if s.patch > 0.45 { 0.03 } else { 0.003 } {
                Some((v(COMMON_TREE, 5), 1.4 + size * 0.5, Tint::Dry))
            } else if q < 0.016 {
                let m = if pick < 0.5 { BUSH_FLOWERS } else { BUSH };
                Some((m, 0.6 + size * 0.6, Tint::Dry))
            } else if q < 0.019 {
                Some((v(ROCK, 3), 0.25 + size * 0.6, Tint::Normal))
            } else {
                None
            }
        }
        // Northwest: lakes and swampy forest
        _ => {
            if fm > 0.3 && q < 0.2 * fm {
                let m = if pick < 0.55 {
                    v(COMMON_TREE, 5)
                } else if pick < 0.8 {
                    v(DEAD, 3)
                } else if pick < 0.9 {
                    v(TWISTED, 2)
                } else {
                    v(PINE, 5)
                };
                Some((m, 1.2 + size * 0.45, Tint::Swamp))
            } else if q < 0.03 {
                Some((BUSH, 0.7 + size * 0.6, Tint::Swamp))
            } else if q < 0.036 {
                Some((v(DEAD, 3), 0.6 + size * 0.3, Tint::Swamp))
            } else {
                None
            }
        }
    }
}

/// Trees, bushes, rocks and mine entrances for one prop cell.
pub fn prop_placements(t: &Terrain, ci: i64, cj: i64) -> Vec<Placement> {
    let (x0, z0) = (ci as f64 * PROP_CELL, cj as f64 * PROP_CELL);
    let (ox, oz) = (x0 + PROP_CELL / 2.0, z0 + PROP_CELL / 2.0);
    let mut out = Vec::new();
    if ox.hypot(oz) > LAND_R + 1000.0 {
        return out;
    }
    let mut r = Rng::new(cell_seed(ci, cj, 1));
    let (spx, spz) = spawn_point();
    let mines = mine_sites();
    let (evx, evz) = to_world(EX, EY);
    const STEP: f64 = 8.0;
    let steps = (PROP_CELL / STEP) as usize;
    for j in 0..steps {
        for i in 0..steps {
            let jx = x0 + (i as f64 + r.next()) * STEP;
            let jz = z0 + (j as f64 + r.next()) * STEP;
            let (q, pick, size) = (r.next(), r.next(), r.next());
            let rot = (r.next() * std::f64::consts::TAU) as f32;
            if q > 0.3 {
                continue; // nothing anywhere is denser than this
            }
            if on_deck(t, jx, jz) {
                continue;
            }
            let s = t.sample(jx, jz);
            // boulders along the rocky North and South shores
            if (s.region == 0 || s.region == 3) && s.water.is_none() && s.h > 0.3 && s.shore < 160.0 && !s.canyon {
                if q < 0.08 {
                    let m = model::ROCK + ((pick * 997.0) as u16 % 3);
                    out.push(Placement { model: m, pos: [(jx - ox) as f32, (s.h - 0.3) as f32, (jz - oz) as f32], rot, scale: (0.6 + size * 1.6) as f32, tint: Tint::Normal });
                }
                continue;
            }
            if s.water.is_some() || (s.h < 2.0 && !s.canyon) || s.glow > 0.0 || s.shore < 30.0 {
                continue;
            }
            if s.wet > 0.3 {
                // Northwest pools: only dead trees stand in the water
                if s.region == 5 && q < 0.012 {
                    let m = model::DEAD + ((pick * 997.0) as u16 % 3);
                    out.push(Placement { model: m, pos: [(jx - ox) as f32, (s.h - 0.6) as f32, (jz - oz) as f32], rot, scale: (0.6 + size * 0.4) as f32, tint: Tint::Swamp });
                }
                continue;
            }
            if (jx - spx).hypot(jz - spz) < 10.0 || mines.iter().any(|&(a, b)| (a - jx).hypot(b - jz) < 22.0) {
                continue;
            }
            let Some((m, sc, tint)) = choose_prop(&s, q, pick, size) else { continue };
            let is_rock = (model::ROCK..model::ROCK + 3).contains(&m);
            if !is_rock {
                // trees don't grow on steep slopes; rocks do
                let slope = (t.height(jx + 3.0, jz) - t.height(jx - 3.0, jz)).abs() + (t.height(jx, jz + 3.0) - t.height(jx, jz - 3.0)).abs();
                if slope > 7.0 {
                    continue;
                }
            }
            out.push(Placement { model: m, pos: [(jx - ox) as f32, (s.h - 0.15) as f32, (jz - oz) as f32], rot, scale: sc as f32, tint });
        }
    }
    // the bridge city's buildings
    for l in crate::city::lots_in(&t.bridge, &t.city, x0, z0, x0 + PROP_CELL, z0 + PROP_CELL) {
        let tint = [Tint::Normal, Tint::Dry, Tint::Dark, Tint::Desert][l.look as usize % 4];
        out.push(Placement { model: model::BUILDING + l.kind, pos: [(l.x - ox) as f32, l.y as f32, (l.z - oz) as f32], rot: l.rot, scale: 1.0, tint });
    }
    for (mx, mz) in mines {
        if mx >= x0 && mx < x0 + PROP_CELL && mz >= z0 && mz < z0 + PROP_CELL {
            let face = (mx - evx).atan2(mz - evz); // face away from Everest
            let y = (t.height(mx, mz) - 0.3) as f32;
            out.push(Placement { model: model::MINE, pos: [(mx - ox) as f32, y, (mz - oz) as f32], rot: -face as f32, scale: 1.0, tint: Tint::Normal });
        }
    }
    out
}

fn placeholder_colour(m: u16, tint: Tint) -> [f32; 3] {
    use model::*;
    let leafy = match tint {
        Tint::Dark => [0.13, 0.27, 0.15],
        Tint::Swamp => [0.30, 0.42, 0.22],
        Tint::Dry => [0.40, 0.55, 0.22],
        _ => [0.27, 0.48, 0.2],
    };
    if (PINE..PINE + 5).contains(&m) {
        [leafy[0] * 0.8, leafy[1] * 0.85, leafy[2] * 1.05]
    } else if (DEAD..DEAD + 3).contains(&m) {
        [0.45, 0.43, 0.40]
    } else if (ROCK..ROCK + 3).contains(&m) {
        if tint == Tint::Desert {
            [0.78, 0.6, 0.42]
        } else {
            [0.6, 0.6, 0.58]
        }
    } else if m == MINE {
        [0.6, 0.58, 0.55]
    } else {
        leafy
    }
}

fn placeholder_height(k: PropKind) -> f32 {
    match k {
        PropKind::Pine => 11.5,
        PropKind::Broad => 9.1,
        PropKind::Dead => 7.5,
        PropKind::Bush => 1.7,
        PropKind::Rock => 1.4,
        PropKind::Mine | PropKind::House => 1.0,
    }
}

/// The simple stand-in shapes for a cell's placements: all of them for
/// cells in the distance (`far`), or only the ones without a real model
/// (mine entrances) for cells near the player.
pub fn placeholder_mesh(kinds: &PropKinds, placements: &[Placement], far: bool) -> Option<MeshData> {
    let mut m = MeshData::default();
    for p in placements {
        if far && p.model >= model::BUILDING && p.model != model::MINE {
            let b = &crate::city::BUILDINGS[(p.model - model::BUILDING) as usize];
            if MODELS[p.model as usize].far.is_some() {
                house_box(&mut m, p.pos, p.rot, b.w as f32, b.l as f32, b.height, p.tint);
            }
            continue;
        }
        let (kind, scale) = if p.model == model::MINE {
            (PropKind::Mine, 1.0)
        } else if far {
            let info = &MODELS[p.model as usize];
            match info.far {
                Some(k) => (k, info.height * p.scale / placeholder_height(k)),
                None => continue,
            }
        } else {
            continue;
        };
        add_geo(&mut m, kinds.get(kind), p.pos, scale, p.rot, placeholder_colour(p.model, p.tint));
    }
    (!m.is_empty()).then_some(m)
}

/// Flowers, ferns, clover, mushrooms, grass clumps and pebbles for one
/// small-plant cell, by region. Positions are relative to the cell centre.
pub fn detail_placements(t: &Terrain, ci: i64, cj: i64) -> Vec<Placement> {
    use model::*;
    let mut out = Vec::new();
    let (x0, z0) = (ci as f64 * DETAIL_CELL, cj as f64 * DETAIL_CELL);
    let (ox, oz) = (x0 + DETAIL_CELL / 2.0, z0 + DETAIL_CELL / 2.0);
    if ox.hypot(oz) > LAND_R {
        return out;
    }
    let Some(lat) = Lattice::new(t, x0, z0) else { return out };
    let mut r = Rng::new(cell_seed(ci, cj, 3));
    const STEP: f64 = 2.0;
    let steps = (DETAIL_CELL / STEP) as usize;
    for j in 0..steps {
        for i in 0..steps {
            let gx = (i as f64 + r.next()) * STEP;
            let gz = (j as f64 + r.next()) * STEP;
            let (q, pick, size) = (r.next(), r.next(), r.next());
            let rot = r.next() as f32 * std::f32::consts::TAU;
            let (s, h) = lat.at(gx, gz);
            if s.water.is_some() || s.glow > 0.0 || (s.h < 2.0 && !s.canyon) || s.shore < 20.0 || s.h > 4300.0 || on_deck(t, x0 + gx, z0 + gz) {
                continue;
            }
            let v = |base: u16, n: u16| base + ((pick * 997.0) as u16 % n);
            let wet = s.wet > 0.15;
            let choice: Option<(u16, f64, Tint)> = match s.region {
                4 => {
                    // wildflowers grow in drifts
                    let drift = s.patch > 0.15;
                    if drift && q < 0.06 {
                        Some((v(FLOWER, 4), 0.3 + size * 0.15, Tint::Normal))
                    } else if q < 0.07 {
                        Some((v(GRASS + 2, 2), 0.35 + size * 0.2, Tint::Dry))
                    } else if q < 0.074 {
                        Some((v(FLOWER, 4), 0.3 + size * 0.15, Tint::Normal))
                    } else if q < 0.095 {
                        Some((v(CLOVER, 2), 0.3 + size * 0.15, Tint::Dry))
                    } else if q < 0.098 {
                        Some((v(PEBBLE, 5), 1.0 + size * 1.5, Tint::Normal))
                    } else {
                        None
                    }
                }
                2 => {
                    if q < 0.05 {
                        Some((v(GRASS, 2), 0.35 + size * 0.2, Tint::Normal))
                    } else if q < if s.field { 0.075 } else { 0.06 } {
                        Some((v(FLOWER, 4), 0.3 + size * 0.15, Tint::Normal))
                    } else if q < 0.07 {
                        Some((v(CLOVER, 2), 0.3 + size * 0.15, Tint::Normal))
                    } else {
                        None
                    }
                }
                3 => {
                    if q < 0.05 {
                        Some((FERN, 0.18 + size * 0.12, Tint::Dark))
                    } else if q < if s.patch > 0.4 { 0.08 } else { 0.056 } {
                        // mushrooms come in clusters
                        Some((v(MUSHROOM, 2), 0.5 + size * 0.6, Tint::Normal))
                    } else if q < 0.065 {
                        Some((PLANT_BIG, 0.25 + size * 0.15, Tint::Dark))
                    } else if q < 0.09 {
                        Some((GRASS, 0.3 + size * 0.2, Tint::Dark))
                    } else {
                        None
                    }
                }
                5 => {
                    if wet && q < 0.06 {
                        Some((LILY, 0.8 + size * 0.6, Tint::Swamp))
                    } else if s.wet > 0.02 && q < 0.12 {
                        // reeds around the pools
                        Some((GRASS + 3, 0.6 + size * 0.35, Tint::Swamp))
                    } else if q < 0.04 {
                        Some((FERN, 0.16 + size * 0.1, Tint::Swamp))
                    } else if q < 0.08 {
                        Some((GRASS + 3, 0.35 + size * 0.25, Tint::Swamp))
                    } else if q < 0.087 {
                        Some((v(MUSHROOM, 2), 0.5 + size * 0.5, Tint::Normal))
                    } else {
                        None
                    }
                }
                0 => {
                    if q < 0.04 {
                        Some((v(PEBBLE, 5), 1.0 + size * 2.0, Tint::Normal))
                    } else if q < 0.055 {
                        Some((GRASS, 0.3 + size * 0.15, Tint::Normal))
                    } else {
                        None
                    }
                }
                // Northeast: pebbles and a few dry tufts
                1 => {
                    if q < 0.015 {
                        Some((v(PEBBLE, 5), 1.0 + size * 2.0, Tint::Normal))
                    } else if q < 0.024 {
                        Some((GRASS + 2, 0.3 + size * 0.2, Tint::Dry))
                    } else {
                        None
                    }
                }
                _ => (q < 0.02).then(|| (v(PEBBLE, 5), 1.0 + size * 2.0, Tint::Normal)),
            };
            if let Some((m, sc, tint)) = choice {
                out.push(Placement { model: m, pos: [(x0 + gx - ox) as f32, h as f32 - 0.03, (z0 + gz - oz) as f32], rot, scale: sc as f32, tint });
            }
        }
    }
    out
}

/// The finest terrain grid (8 m) over one 32 m cell, for placing small
/// things exactly on the drawn ground.
struct Lattice {
    ss: Vec<Sample>,
}

impl Lattice {
    const L: usize = 5;
    const CELL: f64 = 8.0;
    fn new(t: &Terrain, x0: f64, z0: f64) -> Option<Lattice> {
        let mut ss = Vec::with_capacity(Self::L * Self::L);
        for j in 0..Self::L {
            for i in 0..Self::L {
                ss.push(t.sample(x0 + i as f64 * Self::CELL, z0 + j as f64 * Self::CELL));
            }
        }
        if ss.iter().all(|s| s.water.is_some() || (s.h < 2.0 && !s.canyon)) {
            return None;
        }
        Some(Lattice { ss })
    }
    /// The nearest sample, and the drawn ground height, at (gx, gz) metres into the cell.
    fn at(&self, gx: f64, gz: f64) -> (&Sample, f64) {
        let l = Self::L;
        let (fi, fj) = (gx / Self::CELL, gz / Self::CELL);
        let (ii, jj) = ((fi.floor() as usize).min(l - 2), (fj.floor() as usize).min(l - 2));
        let (u, w) = (fi - ii as f64, fj - jj as f64);
        let ss = &self.ss;
        let s = &ss[(jj + (w > 0.5) as usize) * l + ii + (u > 0.5) as usize];
        let h = tri_interp(ss[jj * l + ii].h, ss[jj * l + ii + 1].h, ss[(jj + 1) * l + ii].h, ss[(jj + 1) * l + ii + 1].h, u, w);
        (s, h)
    }
}

/// Side of a grass cell in metres (grass is drawn close to the player only).
pub const GRASS_CELL: f64 = 32.0;

/// Tufts of grass for one cell, coloured from the ground under them.
/// Positions are relative to the cell's centre.
pub fn grass_cell(t: &Terrain, ci: i64, cj: i64) -> Option<MeshData> {
    let (x0, z0) = (ci as f64 * GRASS_CELL, cj as f64 * GRASS_CELL);
    let (ox, oz) = (x0 + GRASS_CELL / 2.0, z0 + GRASS_CELL / 2.0);
    if ox.hypot(oz) > LAND_R {
        return None;
    }
    // the finest terrain grid (8 m) inside this cell
    const L: usize = 5;
    let cell = GRASS_CELL / (L - 1) as f64;
    let mut ss = Vec::with_capacity(L * L);
    for j in 0..L {
        for i in 0..L {
            ss.push(t.sample(x0 + i as f64 * cell, z0 + j as f64 * cell));
        }
    }
    if ss.iter().all(|s| s.water.is_some() || (s.h < 2.0 && !s.canyon)) || on_deck(t, x0 + GRASS_CELL / 2.0, z0 + GRASS_CELL / 2.0) {
        return None;
    }
    let mut r = Rng::new(cell_seed(ci, cj, 2));
    let mut m = MeshData::default();
    const STEP: f64 = 0.8;
    let steps = (GRASS_CELL / STEP) as usize;
    for j in 0..steps {
        for i in 0..steps {
            let gx = (i as f64 + r.next()) * STEP;
            let gz = (j as f64 + r.next()) * STEP;
            let q = r.next();
            let tall = r.next() as f32;
            let rot = r.next() as f32 * std::f32::consts::TAU;
            let (fi, fj) = (gx / cell, gz / cell);
            let (ii, jj) = ((fi.floor() as usize).min(L - 2), (fj.floor() as usize).min(L - 2));
            let (u, w) = (fi - ii as f64, fj - jj as f64);
            let s = &ss[(jj + (w > 0.5) as usize) * L + ii + (u > 0.5) as usize];
            let base_density = match s.region {
                4 => 0.9,
                2 => {
                    if s.field {
                        0.45
                    } else {
                        0.7
                    }
                }
                5 => 0.65,
                3 => 0.35,
                0 => 0.12,
                _ => 0.0,
            };
            let dens = base_density * (1.0 - sm(3600.0, 4300.0, s.h)) * sm(30.0, 80.0, s.shore) * (1.0 - s.forest as f64 * 0.5);
            if q > dens || s.water.is_some() || s.glow > 0.0 || s.wet > 0.3 || (s.h < 2.0 && !s.canyon) {
                continue;
            }
            let h = tri_interp(ss[jj * L + ii].h, ss[jj * L + ii + 1].h, ss[(jj + 1) * L + ii].h, ss[(jj + 1) * L + ii + 1].h, u, w);
            let base = [(x0 + gx - ox) as f32, h as f32 - 0.05, (z0 + gz - oz) as f32];
            let ground = ground_grade(s.col);
            let root = lin3(scale3(ground, 0.92));
            let tip = lin3(mix3(scale3(ground, 1.18), [0.62, 0.82, 0.32], 0.2));
            let height = 0.22 + tall * 0.38;
            for b in 0..4 {
                let a = rot + b as f32 * 1.571 + tall;
                let (ca, sa) = (a.cos(), a.sin());
                let lean = 0.22 * height;
                let p0 = [base[0] - sa * 0.07, base[1], base[2] + ca * 0.07];
                let p1 = [base[0] + sa * 0.07, base[1], base[2] - ca * 0.07];
                let p2 = [base[0] + ca * lean, base[1] + height, base[2] + sa * lean];
                let k = m.positions.len() as u32;
                for (p, c) in [(p0, root), (p1, root), (p2, tip)] {
                    m.positions.push(p);
                    m.normals.push([0.0, 1.0, 0.0]);
                    m.colors.push(c);
                }
                m.indices.extend_from_slice(&[k, k + 1, k + 2]);
            }
        }
    }
    (!m.is_empty()).then_some(m)
}

// ---------------------------------------------------------------------
// The Cloudwall and the sky
// ---------------------------------------------------------------------

/// Height of the Cloudwall's main body (its towers rise higher). Tall
/// enough to stand a few degrees above the horizon from anywhere on the
/// island, about 9,400 km away.
pub const CLOUDWALL_TOP: f64 = 700_000.0;

/// The Cloudwall: a towering ring of cloud at the edge of the Rim Sea,
/// billowing in and out, its top rising into great towers here and there.
/// The cloud shader gives it light and texture.
pub fn cloudwall_mesh() -> MeshData {
    let mut r = Rng::new(5150);
    let noise = Noise::new(&mut r);
    const SEG: usize = 1440;
    let rows: [f64; 9] = [-5_000.0, 20_000.0, 70_000.0, 150_000.0, 250_000.0, 370_000.0, 500_000.0, 620_000.0, CLOUDWALL_TOP];
    let mut m = MeshData::default();
    for s in 0..=SEG {
        let a = s as f64 / SEG as f64 * std::f64::consts::TAU;
        let (ca, sa) = (a.cos(), a.sin());
        let tower = noise.fbm(ca * 18.0 + 3.0, sa * 18.0, 4);
        let top_extra = 320_000.0 * (tower + 0.15).max(0.0) + 90_000.0 * noise.fbm(ca * 70.0 + 9.0, sa * 70.0, 3);
        for (k, &y) in rows.iter().enumerate() {
            let t = k as f64 / (rows.len() - 1) as f64;
            let lump = noise.fbm(ca * 40.0 + k as f64 * 0.37, sa * 40.0, 4);
            let yy = y + top_extra * t * t;
            let rr = CLOUD_R + 160_000.0 * lump * (0.3 + t) - 120_000.0 * t;
            m.positions.push([(ca * rr) as f32, yy as f32, (sa * rr) as f32]);
            m.normals.push(norm3([-(ca as f32), 0.4 * t as f32, -(sa as f32)]));
            m.colors.push([1.0, 1.0, 1.0, 1.0]);
        }
    }
    let nr = rows.len() as u32;
    for s in 0..SEG as u32 {
        for k in 0..nr - 1 {
            let a = s * nr + k;
            let b = a + 1;
            let c = a + nr;
            let d = c + 1;
            m.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    m
}

/// A sky dome, brighter near the horizon. Colour it with the sky colour.
pub fn sky_dome(radius: f32) -> MeshData {
    let (v, f) = icosphere(3);
    let mut m = MeshData::default();
    for p in &v {
        m.positions.push([p[0] * radius, p[1] * radius, p[2] * radius]);
        m.normals.push([-p[0], -p[1], -p[2]]);
        let up = p[1].max(0.0);
        let b = 1.12 - 0.42 * up.powf(0.6);
        let warm = 1.0 - up.powf(0.3) * 0.08;
        m.colors.push([b * warm * 1.02, b * warm, b, 1.0]);
    }
    for t in f {
        m.indices.extend_from_slice(&[t[0] as u32, t[2] as u32, t[1] as u32]);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lod;

    #[test]
    fn icosphere_faces() {
        assert_eq!(icosphere(0).1.len(), 20);
        assert_eq!(icosphere(1).1.len(), 80);
    }

    #[test]
    fn chunks_props_and_grass_at_spawn() {
        let t = Terrain::new();
        let (sx, sz) = spawn_point();
        let keys = lod::select([sx, 400.0, sz], 1.7);
        let c = build_chunk(&t, keys[0]);
        let tm = c.terrain.expect("land at spawn");
        assert_eq!(tm.positions.len(), (CHUNK_N + 1) * (CHUNK_N + 1) + 4 * CHUNK_N * 2);
        let kinds = PropKinds::build();
        let mut props = 0;
        let mut far = 0;
        for j in -6..6 {
            for i in -6..6 {
                let p = prop_placements(&t, (sx / PROP_CELL).floor() as i64 + i, (sz / PROP_CELL).floor() as i64 + j);
                props += p.len();
                far += placeholder_mesh(&kinds, &p, true).map_or(0, |m| m.triangle_count());
            }
        }
        assert!(props > 0 && far > 0);
        let mut small = 0;
        for j in -2..2 {
            for i in -2..2 {
                small += detail_placements(&t, (sx / DETAIL_CELL).floor() as i64 + i, (sz / DETAIL_CELL).floor() as i64 + j).len();
            }
        }
        assert!(small > 20, "{small} small plants");
        let g = grass_cell(&t, (sx / GRASS_CELL).floor() as i64, (sz / GRASS_CELL).floor() as i64).expect("grass at spawn");
        assert!(g.triangle_count() > 300);
        let d = drawn_height(&t, sx + 3.3, sz + 5.1);
        assert!((d - t.height(sx + 3.3, sz + 5.1)).abs() < 3.0);
    }

    #[test]
    fn model_table_matches_indexes() {
        assert_eq!(MODELS[model::PINE as usize].name, "Pine_1");
        assert_eq!(MODELS[model::TWISTED as usize].name, "TwistedTree_2");
        assert_eq!(MODELS[model::ROCK as usize].name, "Rock_Medium_1");
        assert_eq!(MODELS[model::GRASS as usize].name, "Grass_Common_Short");
        assert_eq!(MODELS[model::PEBBLE as usize + 4].name, "Pebble_Square_2");
        assert_eq!(MODELS[model::PEBBLE as usize + 5].name, "House_4x6_2");
        assert_eq!(MODELS.len(), model::BUILDING as usize + crate::city::BUILDINGS.len());
        for (k, b) in crate::city::BUILDINGS.iter().enumerate() {
            assert_eq!(MODELS[model::BUILDING as usize + k].name, b.name);
        }
    }

    #[test]
    fn canyon_and_bridge_city() {
        let t = Terrain::new();
        let b = &t.bridge;
        // a chunk deep in the canyon beside the bridge is land, with detail channels
        let (x, z) = b.from_deck(0.0, 2_000.0);
        let keys = lod::select([x, -40_000.0, z], 1.7);
        let c = build_chunk(&t, keys[0]);
        let tm = c.terrain.expect("canyon floor");
        assert_eq!(tm.uvs.len(), tm.positions.len());
        assert!(tm.positions.iter().any(|p| p[1] < -30_000.0));
        assert!(c.water.is_none(), "no sea in the canyon");
        // standing on the deck, and the parapets keep you on it
        let (dx, dz) = b.from_deck(-1_000.0, 0.0);
        assert_eq!(stand_height(&t, dx, dz, b.deck_y + 1.7), b.deck_y);
        assert!(stand_height(&t, dx, dz, -30_000.0) < -30_000.0);
        let (ox, oz) = b.from_deck(-1_000.0, 80.0);
        let (kx, kz) = keep_on_deck(&t, dx, dz, ox, oz, b.deck_y);
        assert!(b.deck_at(kx, kz).is_some());
        // houses in the prop cells on the deck, and no trees
        let (hx, hz) = b.from_deck(500.0, 20.0);
        let (ci, cj) = ((hx / PROP_CELL).floor() as i64, (hz / PROP_CELL).floor() as i64);
        let p = prop_placements(&t, ci, cj);
        assert!(p.iter().any(|q| q.model >= model::BUILDING && q.model != model::MINE), "no houses");
        let (cx, cz) = ((ci as f64 + 0.5) * PROP_CELL, (cj as f64 + 0.5) * PROP_CELL);
        assert!(p.iter().all(|q| q.model >= model::BUILDING || !on_deck(&t, cx + q.pos[0] as f64, cz + q.pos[2] as f64)));
        let kinds = PropKinds::build();
        assert!(placeholder_mesh(&kinds, &p, true).map_or(0, |m| m.triangle_count()) > 50);
    }

    #[test]
    fn ocean_chunk_is_one_square() {
        let t = Terrain::new();
        let k = lod::NodeKey { level: 3, ix: 0, iz: 0 };
        assert!(k.ocean_only());
        let c = build_chunk(&t, k);
        assert!(c.terrain.is_none());
        assert_eq!(c.water.unwrap().triangle_count(), 2);
    }
}
