//! The great bridge across the canyon (first pass): a stone deck 140 m
//! wide carrying the city, held up by one huge arch that springs from the
//! canyon walls far below, with tall piers and smaller arches between the
//! arch and the deck, and piers stepping down the walls toward each rim.
//! From afar the city on it is drawn as blocks of walls and roofs.

use crate::city::{self, BUILDINGS, HOUSE_KINDS};
use crate::meshgen::{house_box, lin3, MeshData, Tint};
use crate::worldgen::borders::BridgeSite;
use crate::worldgen::terrain::Terrain;

/// Thickness of the deck slab under the streets.
pub const SLAB: f64 = 45.0;
const PARAPET_H: f64 = 7.0;
pub const PARAPET_T: f64 = 4.0;

/// The bridge's meshes, positioned relative to `centre` (y is absolute).
pub struct BridgeMeshes {
    pub centre: (f64, f64),
    pub structure: MeshData,
    /// The city as seen from afar, in short pieces along the deck so the
    /// pieces near the player can give way to the real houses:
    /// (from u, to u, mesh).
    pub city: Vec<(f64, f64, MeshData)>,
}

/// Builds in the deck's own frame: u along the deck, y up, v across.
struct Frame<'a> {
    b: &'a BridgeSite,
    m: &'a mut MeshData,
}

impl Frame<'_> {
    fn p(&self, u: f64, y: f64, v: f64) -> [f32; 3] {
        let (ax, az) = self.b.axis;
        [(u * ax - v * az) as f32, y as f32, (u * az + v * ax) as f32]
    }
    /// A face with corners given as (u, y, v), counter-clockwise from the front.
    fn face(&mut self, q: [[f64; 3]; 4], col: [f32; 3]) {
        let [a, b, c, d] = q.map(|k| self.p(k[0], k[1], k[2]));
        self.m.face(a, b, c, d, lin3(col));
    }
    /// A box from (u0, y0, v0) to (u1, y1, v1); `top` and `side` colours.
    #[allow(clippy::too_many_arguments)]
    fn cube(&mut self, u0: f64, u1: f64, y0: f64, y1: f64, v0: f64, v1: f64, top: [f32; 3], side: [f32; 3]) {
        let dark = |c: [f32; 3], k: f32| [c[0] * k, c[1] * k, c[2] * k];
        self.face([[u0, y1, v1], [u1, y1, v1], [u1, y1, v0], [u0, y1, v0]], top);
        self.face([[u0, y0, v0], [u1, y0, v0], [u1, y0, v1], [u0, y0, v1]], dark(side, 0.7));
        self.face([[u0, y0, v1], [u1, y0, v1], [u1, y1, v1], [u0, y1, v1]], side);
        self.face([[u1, y0, v0], [u0, y0, v0], [u0, y1, v0], [u1, y1, v0]], dark(side, 0.92));
        self.face([[u1, y0, v1], [u1, y0, v0], [u1, y1, v0], [u1, y1, v1]], dark(side, 0.85));
        self.face([[u0, y0, v0], [u0, y0, v1], [u0, y1, v1], [u0, y1, v0]], dark(side, 0.88));
    }
    /// A beam running along u through the points `us`, between `bot(u)`
    /// and `top(u)`, `hw(u)` either side of the deck's centre line.
    /// `band` splits the sides: below `bot + band` they take `ring`.
    #[allow(clippy::too_many_arguments)]
    fn beam(&mut self, us: &[f64], bot: &dyn Fn(f64) -> f64, top: &dyn Fn(f64) -> f64, hw: &dyn Fn(f64) -> f64, stone: [f32; 3], ring: [f32; 3], band: f64) {
        let shade = |c: [f32; 3], k: f32| [c[0] * k, c[1] * k, c[2] * k];
        for (i, w) in us.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let (ba, bb, ta, tb, wa, wb) = (bot(a), bot(b), top(a), top(b), hw(a), hw(b));
            // alternate the stones a little, like courses of voussoirs
            let k = if i % 2 == 0 { 1.0 } else { 0.95 };
            self.face([[a, ta, wa], [b, tb, wb], [b, tb, -wb], [a, ta, -wa]], shade(stone, k * 1.04));
            self.face([[a, ba, -wa], [b, bb, -wb], [b, bb, wb], [a, ba, wa]], shade(ring, k * 0.62));
            let (ma, mb) = ((ba + band).min(ta), (bb + band).min(tb));
            for (sgn, sk) in [(1.0, 1.0), (-1.0, 0.9)] {
                let (va, vb) = (wa * sgn, wb * sgn);
                let mut f = |y0a: f64, y0b: f64, y1a: f64, y1b: f64, c: [f32; 3]| {
                    if sgn > 0.0 {
                        self.face([[a, y0a, va], [b, y0b, vb], [b, y1b, vb], [a, y1a, va]], c);
                    } else {
                        self.face([[b, y0b, vb], [a, y0a, va], [a, y1a, va], [b, y1b, vb]], c);
                    }
                };
                f(ba, bb, ma, mb, shade(ring, k * sk));
                f(ma, mb, ta, tb, shade(stone, k * sk * 0.97));
            }
        }
        // end caps
        if let (Some(&a), Some(&b)) = (us.first(), us.last()) {
            let (ba, ta, wa) = (bot(a), top(a), hw(a));
            self.face([[a, ba, wa], [a, ta, wa], [a, ta, -wa], [a, ba, -wa]], shade(stone, 0.8));
            let (bb, tb, wb) = (bot(b), top(b), hw(b));
            self.face([[b, bb, -wb], [b, tb, -wb], [b, tb, wb], [b, bb, wb]], shade(stone, 0.85));
        }
    }
}

const PAVING: [f32; 3] = [0.66, 0.62, 0.55];
const FLAGS: [f32; 3] = [0.74, 0.70, 0.62];
const YARD: [f32; 3] = [0.56, 0.53, 0.48];
const STONE: [f32; 3] = [0.80, 0.76, 0.68];
const RING: [f32; 3] = [0.70, 0.64, 0.56];
const PIER: [f32; 3] = [0.76, 0.72, 0.65];

pub fn bridge_meshes(t: &Terrain) -> BridgeMeshes {
    let b = &t.bridge;
    let mut structure = MeshData::default();
    {
        let mut f = Frame { b, m: &mut structure };
        let (top, bot) = (b.deck_y, b.deck_y - SLAB);
        let hw = b.half_w;
        // the deck, in pieces along its length
        let n = (2.0 * b.half_len / 300.0).ceil() as usize;
        for i in 0..n {
            let u0 = -b.half_len + 2.0 * b.half_len * i as f64 / n as f64;
            let u1 = -b.half_len + 2.0 * b.half_len * (i + 1) as f64 / n as f64;
            // streets and yards across the deck
            let bands: [(f64, f64, [f32; 3]); 9] = [
                (-hw + PARAPET_T, -50.0, FLAGS),
                (-50.0, -38.0, PAVING),
                (-38.0, -10.0, YARD),
                (-10.0, 10.0, PAVING),
                (10.0, 38.0, YARD),
                (38.0, 50.0, PAVING),
                (50.0, hw - PARAPET_T, FLAGS),
                (-hw, -hw + PARAPET_T, STONE),
                (hw - PARAPET_T, hw, STONE),
            ];
            for (v0, v1, c) in bands.iter().take(7) {
                f.face([[u0, top, *v1], [u1, top, *v1], [u1, top, *v0], [u0, top, *v0]], *c);
            }
            // the parapets, their outer faces running down the slab's side
            for (v0, v1) in [(-hw, -hw + PARAPET_T), (hw - PARAPET_T, hw)] {
                let ph = top + PARAPET_H;
                f.face([[u0, ph, v1], [u1, ph, v1], [u1, ph, v0], [u0, ph, v0]], [0.84, 0.80, 0.72]);
                let (outer, inner) = if v0 < 0.0 { (v0, v1) } else { (v1, v0) };
                let s = outer.signum();
                if s > 0.0 {
                    f.face([[u0, bot, outer], [u1, bot, outer], [u1, ph, outer], [u0, ph, outer]], STONE);
                    f.face([[u1, top, inner], [u0, top, inner], [u0, ph, inner], [u1, ph, inner]], [0.72, 0.68, 0.61]);
                } else {
                    f.face([[u1, bot, outer], [u0, bot, outer], [u0, ph, outer], [u1, ph, outer]], [0.74, 0.70, 0.63]);
                    f.face([[u0, top, inner], [u1, top, inner], [u1, ph, inner], [u0, ph, inner]], [0.72, 0.68, 0.61]);
                }
                // a cornice that throws a line of shadow below the streets
                let (c0, c1) = if s > 0.0 { (hw, hw + 3.0) } else { (-hw - 3.0, -hw) };
                f.cube(u0, u1, top - 7.0, top - 3.5, c0, c1, [0.82, 0.78, 0.70], [0.66, 0.62, 0.56]);
            }
            // underneath
            f.face([[u0, bot, -hw], [u1, bot, -hw], [u1, bot, hw], [u0, bot, hw]], [0.5, 0.47, 0.43]);
        }

        // the great arch, its feet sunk into the canyon walls
        let s = b.spring_half;
        let crown_top = bot;
        let crown_bot = crown_top - 320.0;
        let foot_top = b.spring_y + 1_400.0;
        let arch_bot = move |u: f64| crown_bot - (crown_bot - b.spring_y) * (u / s).powi(2);
        let arch_top = move |u: f64| crown_top - (crown_top - foot_top) * (u / s).powi(2);
        let arch_w = move |u: f64| 62.0 + 58.0 * (u / s).powi(2);
        let reach = s * 1.1;
        let us: Vec<f64> = (0..=140).map(|i| -reach + 2.0 * reach * i as f64 / 140.0).collect();
        f.beam(&us, &arch_bot, &arch_top, &arch_w, STONE, RING, 220.0);

        // piers from the arch up to the deck, and down the walls near the rims
        let mut piers: Vec<(f64, f64, f64)> = Vec::new(); // (u, foot, half-thickness)
        const STEP: f64 = 640.0;
        let mut k = -((s / STEP).floor() as i64);
        while (k as f64) * STEP <= s - 250.0 {
            let u = k as f64 * STEP;
            let foot = arch_top(u) - 30.0;
            if bot - foot > 80.0 {
                piers.push((u, foot, 30.0 + 0.02 * (bot - foot)));
            }
            k += 1;
        }
        for side in [-1.0f64, 1.0] {
            let mut d = s + 260.0;
            while d < b.half_len - 250.0 {
                let u = side * d;
                // the lowest ground under the pier
                let mut g = f64::MAX;
                for du in [-50.0, 0.0, 50.0] {
                    for dv in [-50.0, 0.0, 50.0] {
                        let (x, z) = b.from_deck(u + du, dv);
                        g = g.min(t.height(x, z));
                    }
                }
                if bot - g > 60.0 {
                    piers.push((u, g - 60.0, 30.0 + 0.02 * (bot - g)));
                }
                d += 600.0;
            }
        }
        piers.sort_by(|a, c| a.0.total_cmp(&c.0));
        for &(u, foot, th) in &piers {
            let pw = 54.0 + 0.004 * (bot - foot);
            f.cube(u - th, u + th, foot, bot, -pw, pw, PIER, PIER);
            // a plinth band where the pier meets the deck
            f.cube(u - th - 6.0, u + th + 6.0, bot - 30.0, bot, -pw - 4.0, pw + 4.0, RING, RING);
        }
        // smaller arches between the piers, under the deck
        for w in piers.windows(2) {
            let ((ua, fa, ta), (ub, fb, tb)) = (w[0], w[1]);
            let (a, c) = (ua + ta, ub - tb);
            let gap = c - a;
            if !(40.0..=1_000.0).contains(&gap) {
                continue;
            }
            let room = (bot - fa).min(bot - fb);
            let rise = (gap * 0.42).min(room * 0.6);
            if rise < 15.0 {
                continue;
            }
            let crown = bot - 28.0;
            let mid = (a + c) / 2.0;
            let half = gap / 2.0;
            let soffit = move |u: f64| {
                let q = ((u - mid) / half).clamp(-1.0, 1.0);
                crown - rise * (1.0 - (1.0 - q * q).max(0.0).sqrt())
            };
            let deck_bot = move |_u: f64| bot;
            let spw = move |_u: f64| 52.0;
            let us: Vec<f64> = (0..=16).map(|i| a + gap * i as f64 / 16.0).collect();
            f.beam(&us, &soffit, &deck_bot, &spw, PIER, RING, 26.0);
        }
    }

    // the city from afar
    let lots = &t.city;
    let blocks = city::blocks(lots);
    const PIECE: f64 = 120.0;
    let n = (2.0 * b.half_len / PIECE).ceil() as usize;
    let mut pieces: Vec<MeshData> = (0..n).map(|_| MeshData::default()).collect();
    let piece_of = |u: f64| (((u + b.half_len) / PIECE).floor().max(0.0) as usize).min(n - 1);
    for (i, blk) in blocks.iter().enumerate() {
        let m = &mut pieces[piece_of((blk.u0 + blk.u1) / 2.0)];
        let mut f = Frame { b, m };
        let h = blk.height;
        let walls = h - 4.0;
        let tone = (i * 7919 % 13) as f32 / 13.0;
        let wall_c = [0.86 - 0.08 * tone, 0.82 - 0.1 * tone, 0.74 - 0.12 * tone];
        let roof_c = if i % 5 == 3 { [0.38, 0.40, 0.46] } else { [0.68 - 0.1 * tone, 0.32, 0.22] };
        let (u0, u1, v0, v1, y) = (blk.u0, blk.u1, blk.v0, blk.v1, b.deck_y);
        f.cube(u0, u1, y, y + walls, v0, v1, roof_c, wall_c);
        // a roof ridge along the block
        let (vm, ry) = ((v0 + v1) / 2.0, y + h);
        let roof_d = [roof_c[0] * 0.8, roof_c[1] * 0.8, roof_c[2] * 0.8];
        f.face([[u0, y + walls, v1], [u1, y + walls, v1], [u1, ry, vm], [u0, ry, vm]], roof_c);
        f.face([[u1, y + walls, v0], [u0, y + walls, v0], [u0, ry, vm], [u1, ry, vm]], roof_d);
        f.face([[u0, y + walls, v0], [u0, y + walls, v1], [u0, ry, vm], [u0, ry, vm]], wall_c);
        f.face([[u1, y + walls, v1], [u1, y + walls, v0], [u1, ry, vm], [u1, ry, vm]], wall_c);
    }
    for l in lots.iter().filter(|l| (l.kind as usize) >= HOUSE_KINDS && (l.kind as usize) <= city::GATE_TOWER) {
        let bt = &BUILDINGS[l.kind as usize];
        let pos = [(l.x - b.centre.0) as f32, l.y as f32, (l.z - b.centre.1) as f32];
        house_box(&mut pieces[piece_of(l.u)], pos, l.rot, bt.w as f32, bt.l as f32, bt.height, Tint::Normal);
    }
    let city = pieces
        .into_iter()
        .enumerate()
        .filter(|(_, m)| !m.is_empty())
        .map(|(i, m)| (-b.half_len + i as f64 * PIECE, -b.half_len + (i + 1) as f64 * PIECE, m))
        .collect();
    BridgeMeshes { centre: b.centre, structure, city }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_is_built() {
        let t = Terrain::new();
        let bm = bridge_meshes(&t);
        let tris = bm.structure.triangle_count();
        assert!(tris > 1_000 && tris < 60_000, "{tris} triangles");
        let city: usize = bm.city.iter().map(|c| c.2.triangle_count()).sum();
        assert!(city > 2_000 && city < 80_000, "{city} triangles in the city");
        // the arch reaches down to where it meets the canyon walls
        let lo = bm.structure.positions.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!((lo as f64) < t.bridge.spring_y, "lowest {lo}");
    }
}
