//! The bridge city: houses, towers and squares laid out along the bridge
//! deck across the Great Canyon (first pass).
//!
//! The deck is 140 m wide. Down its middle runs a 20 m avenue; on each side
//! a row of houses faces the avenue and a second row, back to back with it
//! across a narrow alley, faces an outer street. Beyond that a promenade
//! runs along the parapet, with watchtowers every 900 m. Cross streets
//! break the rows every 140–180 m, a square opens every 2 km, and tall gate
//! towers stand at both ends.

use crate::worldgen::borders::BridgeSite;
use crate::worldgen::noise::Rng;

/// A building made by `tools/prepare_village.py` (the names must match).
pub struct BuildingType {
    pub name: &'static str,
    /// Width along its front, depth, height (metres).
    pub w: f64,
    pub l: f64,
    pub height: f32,
}

macro_rules! b {
    ($n:expr, $w:expr, $l:expr, $h:expr) => {
        BuildingType { name: $n, w: $w, l: $l, height: $h }
    };
}

pub const BUILDINGS: [BuildingType; 12] = [
    b!("House_4x6_2", 4.0, 6.0, 9.4),
    b!("House_4x8_3", 4.0, 8.0, 12.18),
    b!("House_6x6_2", 6.0, 6.0, 10.62),
    b!("House_6x8_2", 6.0, 8.0, 10.62),
    b!("House_6x8_3", 6.0, 8.0, 13.62),
    b!("House_6x10_2", 6.0, 10.0, 10.62),
    b!("House_8x8_3", 8.0, 8.0, 14.55),
    b!("House_8x12_2", 8.0, 12.0, 11.55),
    b!("Tower_4x4_6", 4.0, 4.0, 24.79),
    b!("Tower_4x4_9", 4.0, 4.0, 33.79),
    b!("Prop_Wagon", 2.0, 4.0, 1.53),
    b!("Prop_Crate", 1.0, 1.0, 1.06),
];
pub const HOUSE_KINDS: usize = 8;
pub const TOWER: usize = 8;
pub const GATE_TOWER: usize = 9;
pub const WAGON: usize = 10;
pub const CRATE: usize = 11;

/// One building on the deck.
#[derive(Clone, Copy, Debug)]
pub struct Lot {
    /// World position of its centre, and the deck height it stands on.
    pub x: f64,
    pub z: f64,
    pub y: f64,
    /// Along the deck (for finding lots quickly).
    pub u: f64,
    pub v: f64,
    /// Index into `BUILDINGS`.
    pub kind: u16,
    /// Turn about the up axis (radians); the front faces the street.
    pub rot: f32,
    /// Colour variation (0–3).
    pub look: u8,
}

const AVENUE: f64 = 10.0;
const ALLEY_AT: f64 = 22.0;
const OUTER_FRONT: f64 = 38.0;

/// Every building on the bridge, sorted along the deck.
pub fn layout(b: &BridgeSite) -> Vec<Lot> {
    let mut out = Vec::new();
    let mut r = Rng::new(7_100);
    let (ax, az) = b.axis;
    // turns that face a building's front (+z) toward -v or +v
    let face_in = az.atan2(-ax) as f32;
    let face_out = (-az).atan2(ax) as f32;
    let put = |out: &mut Vec<Lot>, u: f64, v: f64, kind: usize, rot: f32, look: u8| {
        let (x, z) = b.from_deck(u, v);
        out.push(Lot { x, z, y: b.deck_y, u, v, kind: kind as u16, rot, look });
    };
    let reach = b.half_len - 700.0;
    for side in [-1.0f64, 1.0] {
        // the two rows of houses on this side
        for row in 0..2 {
            let mut u = -reach;
            let mut since_street = 0.0;
            let mut street_every = 140.0 + 40.0 * r.next();
            while u < reach {
                // a square every 2 km
                let to_square = (u / 2_000.0).round() * 2_000.0 - u;
                if to_square.abs() < 35.0 {
                    u += 35.0 + to_square + 1.0;
                    since_street = 0.0;
                    continue;
                }
                if since_street > street_every {
                    u += 8.0;
                    since_street = 0.0;
                    street_every = 140.0 + 40.0 * r.next();
                    continue;
                }
                let kind = (r.next() * HOUSE_KINDS as f64) as usize % HOUSE_KINDS;
                let t = &BUILDINGS[kind];
                let cu = u + t.w / 2.0;
                if cu + t.w / 2.0 > reach {
                    break;
                }
                // row 0 faces the avenue; row 1 faces the outer street
                let (v, toward_avenue) = if row == 0 { (AVENUE + t.l / 2.0, true) } else { (OUTER_FRONT - t.l / 2.0, false) };
                let v = v * side;
                let inward = (side > 0.0) == toward_avenue;
                let rot = if inward { face_in } else { face_out };
                put(&mut out, cu, v, kind, rot, (r.next() * 4.0) as u8 % 4);
                let gap = 0.6 + 1.2 * r.next();
                u += t.w + gap;
                since_street += t.w + gap;
            }
        }
        // watchtowers along the parapet
        let mut u = -reach + 200.0;
        while u < reach {
            put(&mut out, u, side * (b.half_w - 6.0), TOWER, if side > 0.0 { face_in } else { face_out }, 0);
            u += 900.0;
        }
        // gate towers either side of the avenue at both ends
        for end in [-1.0, 1.0] {
            put(&mut out, end * (b.half_len - 350.0), side * (AVENUE + 6.0), GATE_TOWER, if end > 0.0 { face_in } else { face_out }, 0);
        }
    }
    // the squares: wagons and crates
    let mut k = -((reach / 2_000.0).floor() as i64);
    while (k as f64) * 2_000.0 <= reach {
        let cu = k as f64 * 2_000.0;
        for _ in 0..7 {
            let du = (r.next() - 0.5) * 50.0;
            let dv = (r.next() - 0.5) * 2.0 * (OUTER_FRONT - 4.0);
            if dv.abs() < AVENUE - 2.0 {
                continue;
            }
            let kind = if r.next() < 0.4 { WAGON } else { CRATE };
            put(&mut out, cu + du, dv, kind, (r.next() * std::f64::consts::TAU) as f32, 0);
        }
        k += 1;
    }
    out.sort_by(|a, b| a.u.total_cmp(&b.u));
    out
}

/// Lots whose centres lie in the square [x0, x1) x [z0, z1).
pub fn lots_in<'a>(b: &BridgeSite, lots: &'a [Lot], x0: f64, z0: f64, x1: f64, z1: f64) -> impl Iterator<Item = &'a Lot> + 'a {
    let us = [b.to_deck(x0, z0).0, b.to_deck(x1, z0).0, b.to_deck(x0, z1).0, b.to_deck(x1, z1).0];
    let umin = us.iter().cloned().fold(f64::MAX, f64::min) - 1.0;
    let umax = us.iter().cloned().fold(f64::MIN, f64::max) + 1.0;
    let lo = lots.partition_point(|l| l.u < umin);
    let hi = lots.partition_point(|l| l.u <= umax);
    lots[lo..hi].iter().filter(move |l| l.x >= x0 && l.x < x1 && l.z >= z0 && l.z < z1)
}

/// A block of houses seen from afar: from `u0` to `u1` along the deck and
/// `v0` to `v1` across it, its roofs' average height.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub u0: f64,
    pub u1: f64,
    pub v0: f64,
    pub v1: f64,
    pub height: f64,
}

/// Houses grouped into blocks (rows broken by streets and squares), for the
/// city's distant look.
pub fn blocks(lots: &[Lot]) -> Vec<Block> {
    let mut rows: Vec<Vec<&Lot>> = vec![Vec::new(); 4];
    for l in lots.iter().filter(|l| (l.kind as usize) < HOUSE_KINDS) {
        let row = match (l.v > 0.0, l.v.abs() < ALLEY_AT) {
            (false, true) => 0,
            (false, false) => 1,
            (true, true) => 2,
            (true, false) => 3,
        };
        rows[row].push(l);
    }
    let mut out = Vec::new();
    for row in rows {
        let mut cur: Option<(Block, f64, usize)> = None;
        for l in row {
            let t = &BUILDINGS[l.kind as usize];
            let (a, b) = (l.u - t.w / 2.0, l.u + t.w / 2.0);
            let (v0, v1) = (l.v - t.l / 2.0, l.v + t.l / 2.0);
            match &mut cur {
                Some((blk, hsum, n)) if a - blk.u1 < 4.0 => {
                    blk.u1 = b;
                    blk.v0 = blk.v0.min(v0);
                    blk.v1 = blk.v1.max(v1);
                    *hsum += t.height as f64;
                    *n += 1;
                }
                _ => {
                    if let Some((mut blk, hsum, n)) = cur.take() {
                        blk.height = hsum / n as f64;
                        out.push(blk);
                    }
                    cur = Some((Block { u0: a, u1: b, v0, v1, height: 0.0 }, t.height as f64, 1));
                }
            }
        }
        if let Some((mut blk, hsum, n)) = cur {
            blk.height = hsum / n as f64;
            out.push(blk);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::terrain::Terrain;

    #[test]
    fn city_fills_the_deck_without_overlaps() {
        let t = Terrain::new();
        let lots = &t.city;
        let houses: Vec<&Lot> = lots.iter().filter(|l| (l.kind as usize) < HOUSE_KINDS).collect();
        assert!(houses.len() > 4_000, "{} houses", houses.len());
        // all on the deck
        for l in lots {
            assert!(t.bridge.deck_at(l.x, l.z).is_some(), "{l:?}");
        }
        // neighbours in a row don't overlap
        for side in [-1.0, 1.0] {
            for near in [true, false] {
                let row: Vec<&&Lot> = houses.iter().filter(|l| l.v * side > 0.0 && (l.v.abs() < ALLEY_AT) == near).collect();
                for w in row.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let (ta, tb) = (&BUILDINGS[a.kind as usize], &BUILDINGS[b.kind as usize]);
                    assert!(b.u - a.u >= (ta.w + tb.w) / 2.0 - 0.01, "{a:?} {b:?}");
                }
            }
        }
    }
}
