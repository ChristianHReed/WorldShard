//! Meshes for two landmarks (first pass): the ruined outpost on the Bridge
//! City's Southeast approach, and the waterfalls of the Terrace Lakes.

use crate::meshgen::{lin3, MeshData};
use crate::worldgen::features::{Fall, Outpost};
use crate::worldgen::noise::Rng;

/// Builds in a frame turned to face `axis`: u along it, y up, v across.
struct Frame<'a> {
    axis: (f64, f64),
    m: &'a mut MeshData,
}

impl Frame<'_> {
    fn p(&self, u: f64, y: f64, v: f64) -> [f32; 3] {
        let (ax, az) = self.axis;
        [(u * ax - v * az) as f32, y as f32, (u * az + v * ax) as f32]
    }
    fn face(&mut self, q: [[f64; 3]; 4], col: [f32; 3]) {
        let [a, b, c, d] = q.map(|k| self.p(k[0], k[1], k[2]));
        self.m.face(a, b, c, d, lin3(col));
    }
    /// A block from (u0, y0, v0) to (u1, y1, v1), turned `rot` radians
    /// about its centre; a mossy band along its foot.
    #[allow(clippy::too_many_arguments)]
    fn block(&mut self, u0: f64, u1: f64, y0: f64, y1: f64, v0: f64, v1: f64, rot: f64, stone: [f32; 3]) {
        let (cu, cv) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
        let (c, s) = (rot.cos(), rot.sin());
        let at = |u: f64, y: f64, v: f64| {
            let (du, dv) = (u - cu, v - cv);
            [cu + du * c - dv * s, y, cv + du * s + dv * c]
        };
        let k = |c: [f32; 3], f: f32| [c[0] * f, c[1] * f, c[2] * f];
        let moss = [stone[0] * 0.72, stone[1] * 0.8, stone[2] * 0.62];
        let band = (y0 + 1.4).min(y1);
        self.face([at(u0, y1, v1), at(u1, y1, v1), at(u1, y1, v0), at(u0, y1, v0)], k(stone, 1.06));
        for (ya, yb, col) in [(y0, band, moss), (band, y1, stone)] {
            if yb <= ya {
                continue;
            }
            self.face([at(u0, ya, v1), at(u1, ya, v1), at(u1, yb, v1), at(u0, yb, v1)], col);
            self.face([at(u1, ya, v0), at(u0, ya, v0), at(u0, yb, v0), at(u1, yb, v0)], k(col, 0.9));
            self.face([at(u1, ya, v1), at(u1, ya, v0), at(u1, yb, v0), at(u1, yb, v1)], k(col, 0.84));
            self.face([at(u0, ya, v0), at(u0, ya, v1), at(u0, yb, v1), at(u0, yb, v0)], k(col, 0.88));
        }
    }
}

const STONE: [f32; 3] = [0.68, 0.64, 0.57];

/// The ruined outpost: a square fort 90 m across with broken curtain walls,
/// corner towers (one fallen), a gatehouse facing the bridge, the shell of
/// a keep, and fallen stones about it. Positions relative to the fort's
/// centre (y absolute).
pub fn outpost_mesh(o: &Outpost) -> MeshData {
    let mut m = MeshData::default();
    let mut r = Rng::new(31_337);
    let y = o.y - 1.5;
    {
        let mut f = Frame { axis: o.facing, m: &mut m };
        const H: f64 = 45.0;
        const T: f64 = 2.6;
        let tone = |r: &mut Rng| {
            let k = 0.9 + 0.18 * r.next() as f32;
            [STONE[0] * k, STONE[1] * k, STONE[2] * k]
        };
        // curtain walls in short stretches of different heights; some
        // fallen to rubble, some gone
        for side in 0..4 {
            let mut a = -H + 5.0;
            while a < H - 5.0 {
                let len = (7.0 + 5.0 * r.next()).min(H - 5.0 - a);
                let roll = r.next();
                // the gate, in the front wall
                let gate = side == 0 && (a + len / 2.0).abs() < 8.0;
                if !gate && roll > 0.12 {
                    let top = if roll < 0.3 { 1.0 + 1.5 * r.next() } else { 3.5 + 6.0 * r.next() };
                    let col = tone(&mut r);
                    let (b0, b1) = (a, a + len);
                    match side {
                        0 => f.block(H - T, H, y, y + 1.5 + top, b0, b1, 0.0, col),
                        1 => f.block(-H, -H + T, y, y + 1.5 + top, b0, b1, 0.0, col),
                        2 => f.block(b0, b1, y, y + 1.5 + top, H - T, H, 0.0, col),
                        _ => f.block(b0, b1, y, y + 1.5 + top, -H, -H + T, 0.0, col),
                    }
                }
                a += len;
            }
        }
        // corner towers (the back-left one has fallen)
        for (cu, cv, top) in [(H, H, 16.0), (H, -H, 11.0), (-H, H, 13.0), (-H, -H, 3.0)] {
            let col = tone(&mut r);
            f.block(cu - 5.0, cu + 5.0, y, y + 1.5 + top, cv - 5.0, cv + 5.0, 0.0, col);
            // a broken crown: one corner standing taller
            if top > 6.0 {
                f.block(cu - 5.0, cu - 1.5, y + 1.5 + top, y + 1.5 + top + 2.5 + 2.0 * r.next(), cv - 5.0, cv - 1.5, 0.0, col);
            }
        }
        // the gatehouse
        for v in [-8.5, 8.5] {
            let col = tone(&mut r);
            let top = if v < 0.0 { 12.0 } else { 8.0 };
            f.block(H - 6.0, H + 3.0, y, y + 1.5 + top, v - 3.5, v + 3.5, 0.0, col);
        }
        // the shell of the keep: walls only, the side toward the gate fallen
        let (k0, k1, w0, w1) = (-32.0, -12.0, -11.0, 11.0);
        let kt = 1.8;
        for (u0, u1, v0, v1, top) in [
            (k0, k1, w1 - kt, w1, 12.0),
            (k0, k1, w0, w0 + kt, 9.5),
            (k0, k0 + kt, w0, w1, 14.0),
            (k1 - kt, k1, w0, -3.0, 4.0),
            (k1 - kt, k1, 5.0, w1, 2.0),
        ] {
            let col = tone(&mut r);
            f.block(u0, u1, y, y + 1.5 + top, v0, v1, 0.0, col);
        }
        // fallen stones
        for _ in 0..36 {
            let (u, v) = ((r.next() - 0.5) * 2.0 * (H + 10.0), (r.next() - 0.5) * 2.0 * (H + 10.0));
            let sz = 0.7 + 1.4 * r.next();
            let col = tone(&mut r);
            f.block(u - sz / 2.0, u + sz / 2.0, y + 1.0, y + 1.5 + sz * 0.7, v - sz / 2.0, v + sz / 2.0, r.next() * 3.0, col);
        }
    }
    // dressed stone close up (see the terrain shader)
    m.uvs = vec![[2.0, 0.0]; m.positions.len()];
    m
}

/// The Terrace Lakes' waterfalls: a curtain of water arcing out from each
/// lip down to the lake below, with foam where it lands. Positions relative
/// to `origin` (y absolute); colours carry the water's see-through alpha.
pub fn falls_mesh(falls: &[Fall], origin: (f64, f64)) -> MeshData {
    let mut m = MeshData::default();
    for f in falls {
        let (dx, dz) = f.dir;
        let (sx, sz) = (-dz, dx);
        let drop = f.top - f.bottom;
        // leaving the lip at about 3 m/s
        let reach = 3.0 * (2.0 * drop / 9.8).sqrt();
        const ROWS: usize = 10;
        const COLS: usize = 8;
        let at = |row: usize, col: usize| {
            let t = row as f64 / ROWS as f64;
            let out = reach * t.sqrt() + 0.5;
            let half = f.width / 2.0 * (1.0 + 0.25 * t);
            let w = (col as f64 / COLS as f64 - 0.5) * 2.0 * half;
            let x = f.lip.0 + dx * out + sx * w - origin.0;
            let z = f.lip.1 + dz * out + sz * w - origin.1;
            [x as f32, (f.top + 0.3 - drop * t) as f32, z as f32]
        };
        for row in 0..ROWS {
            for col in 0..COLS {
                // streaks down the curtain, whiter toward the foot
                let streak = if (col * 7 + row / 3) % 3 == 0 { 1.0 } else { 0.88 };
                let foot = row as f32 / ROWS as f32;
                let c = [0.72 + 0.2 * foot, 0.82 + 0.14 * foot, 0.88 + 0.1 * foot].map(|v: f32| (v * streak).min(1.0));
                let col4 = [c[0] * c[0], c[1] * c[1], c[2] * c[2], 0.78 + 0.12 * foot];
                m.face(at(row, col), at(row, col + 1), at(row + 1, col + 1), at(row + 1, col), col4);
            }
        }
        // foam on the lake where it lands
        let (fx, fz) = (f.lip.0 + dx * (reach + 6.0), f.lip.1 + dz * (reach + 6.0));
        let rad = f.width * 0.75;
        let centre = [(fx - origin.0) as f32, (f.bottom + 0.15) as f32, (fz - origin.1) as f32];
        const SEG: usize = 16;
        for k in 0..SEG {
            let a0 = k as f64 / SEG as f64 * std::f64::consts::TAU;
            let a1 = (k + 1) as f64 / SEG as f64 * std::f64::consts::TAU;
            let p = |a: f64| [centre[0] + (a.cos() * rad) as f32, centre[1], centre[2] + (a.sin() * rad) as f32];
            let b = m.positions.len() as u32;
            m.positions.extend_from_slice(&[centre, p(a1), p(a0)]);
            m.normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 3]);
            m.colors.extend_from_slice(&[[0.92, 0.95, 0.97, 0.9], [0.85, 0.9, 0.93, 0.0], [0.85, 0.9, 0.93, 0.0]]);
            m.indices.extend_from_slice(&[b, b + 1, b + 2]);
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::terrain::Terrain;

    #[test]
    fn landmarks_are_built() {
        let t = Terrain::new();
        let ruin = outpost_mesh(&t.outpost);
        let tris = ruin.triangle_count();
        assert!(tris > 300 && tris < 20_000, "{tris}");
        assert!(ruin.positions.iter().all(|p| (p[0] as f64).hypot(p[2] as f64) < 120.0));
        let tr = t.terraces.as_ref().expect("terrace lakes placed");
        let falls = falls_mesh(&tr.falls, tr.start);
        assert!(falls.triangle_count() > 100);
        // each fall's lip is on its step, and the water below is the next lake
        for (k, f) in tr.falls.iter().enumerate() {
            let below = (f.lip.0 + f.dir.0 * 30.0, f.lip.1 + f.dir.1 * 30.0);
            let s = t.sample(below.0, below.1);
            assert_eq!(s.water, Some(tr.lakes[k + 1].level), "fall {k}");
            let above = (f.lip.0 - f.dir.0 * 20.0, f.lip.1 - f.dir.1 * 20.0);
            assert_eq!(t.sample(above.0, above.1).water, Some(f.top), "stream {k}");
        }
    }
}
