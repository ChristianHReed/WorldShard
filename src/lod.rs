//! Level of detail for a continent-sized world: a quadtree of square chunks,
//! fine near the camera and coarse far away. Every chunk is drawn with the
//! same number of grid cells, so the whole world costs about the same to
//! draw wherever you stand or fly.

use crate::worldgen::island::{EX, EY, HX, HY};
use crate::worldgen::terrain::{range_peak_spots, to_world, CLOUD_R, LAND_R};

/// The root square covers the whole Rim Sea (±16,777 km).
pub const ROOT_SIZE: f64 = 33_554_432.0;
/// Finest level: 256 m chunks (8 m grid cells).
pub const MAX_LEVEL: u8 = 17;
/// Grid cells per chunk side.
pub const CHUNK_N: usize = 32;

/// Grid cells per side for a chunk: big, distant chunks use fewer.
pub fn chunk_n(k: &NodeKey) -> usize {
    if k.size() >= 65_536.0 {
        CHUNK_N / 2
    } else {
        CHUNK_N
    }
}
/// A chunk splits when the camera is closer than this many chunk sizes.
pub const SPLIT_K: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeKey {
    pub level: u8,
    pub ix: u32,
    pub iz: u32,
}

impl NodeKey {
    pub const ROOT: NodeKey = NodeKey { level: 0, ix: 0, iz: 0 };

    pub fn size(&self) -> f64 {
        ROOT_SIZE / (1u64 << self.level) as f64
    }
    /// North-west corner (min x, min z) in metres.
    pub fn corner(&self) -> (f64, f64) {
        let s = self.size();
        (-ROOT_SIZE / 2.0 + self.ix as f64 * s, -ROOT_SIZE / 2.0 + self.iz as f64 * s)
    }
    pub fn centre(&self) -> (f64, f64) {
        let (x, z) = self.corner();
        let h = self.size() / 2.0;
        (x + h, z + h)
    }
    pub fn children(&self) -> [NodeKey; 4] {
        let (l, x, z) = (self.level + 1, self.ix * 2, self.iz * 2);
        [
            NodeKey { level: l, ix: x, iz: z },
            NodeKey { level: l, ix: x + 1, iz: z },
            NodeKey { level: l, ix: x, iz: z + 1 },
            NodeKey { level: l, ix: x + 1, iz: z + 1 },
        ]
    }
    /// True if `self` is `other` or one of its ancestors.
    pub fn contains(&self, other: &NodeKey) -> bool {
        if other.level < self.level {
            return false;
        }
        let d = other.level - self.level;
        (other.ix >> d) == self.ix && (other.iz >> d) == self.iz
    }
    /// Shortest distance from (px, pz) to this square (0 inside).
    pub fn distance_to(&self, px: f64, pz: f64) -> f64 {
        let (x0, z0) = self.corner();
        let s = self.size();
        let dx = (x0 - px).max(px - (x0 + s)).max(0.0);
        let dz = (z0 - pz).max(pz - (z0 + s)).max(0.0);
        dx.hypot(dz)
    }
    /// No land can be inside: open sea only.
    pub fn ocean_only(&self) -> bool {
        self.distance_to(0.0, 0.0) > LAND_R
    }
    /// Entirely past the Cloudwall: nothing to draw.
    pub fn beyond_cloudwall(&self) -> bool {
        self.distance_to(0.0, 0.0) > CLOUD_R + 200_000.0
    }
}

/// How far the land is drawn: beyond this it is lost in the haze anyway.
/// Grows when flying high so the whole island can be seen.
pub fn view_distance(above: f64) -> f64 {
    view_distance_with(above, DEFAULT_VIEW)
}

/// Land is drawn this far by default: across the whole island, so the
/// giants and their range show from any shore.
pub const DEFAULT_VIEW: f64 = 3_600_000.0;

/// How far land is drawn, given the player's chosen distance.
pub fn view_distance_with(above: f64, chosen: f64) -> f64 {
    chosen.max(above.max(0.0) * 6.0)
}

/// Chunks over the giant peaks and their range stay detailed from any
/// distance, so their outlines stay sharp on the horizon.
fn over_a_peak(k: &NodeKey) -> bool {
    if k.size() <= 80_000.0 {
        return false;
    }
    let giants = [(to_world(EX, EY), 120_000.0), (to_world(HX, HY), 60_000.0)];
    if giants.iter().any(|&((px, pz), r)| k.distance_to(px, pz) < r) {
        return true;
    }
    // the tall range peaks around Everest, a little less finely
    k.size() > 140_000.0 && range_peak_spots().iter().any(|&(px, pz, top)| top > 100_000.0 && k.distance_to(px, pz) < 0.3 * top)
}

/// The chunks to draw for a camera at `cam` (x, y, z) standing `above` metres
/// over the ground. Sorted nearest first.
pub fn select(cam: [f64; 3], above: f64) -> Vec<NodeKey> {
    select_k(cam, above, SPLIT_K)
}

/// Like `select`, with a different split distance: smaller `k` means
/// fewer, coarser chunks (for phones).
pub fn select_k(cam: [f64; 3], above: f64, k_split: f64) -> Vec<NodeKey> {
    select_kv(cam, above, k_split, DEFAULT_VIEW)
}

/// Like `select_k`, drawing land out to `view` metres (or further when high up).
pub fn select_kv(cam: [f64; 3], above: f64, k_split: f64, view: f64) -> Vec<NodeKey> {
    let far = view_distance_with(above, view);
    let mut out = Vec::new();
    let mut stack = vec![NodeKey::ROOT];
    let up = above.max(0.0);
    while let Some(k) = stack.pop() {
        if k.beyond_cloudwall() {
            continue;
        }
        let d = k.distance_to(cam[0], cam[2]).hypot(up);
        if d > far {
            continue;
        }
        if k.level < MAX_LEVEL && !k.ocean_only() && (d < k_split * k.size() || over_a_peak(&k)) {
            stack.extend_from_slice(&k.children());
        } else {
            out.push(k);
        }
    }
    out.sort_by(|a, b| {
        let da = a.distance_to(cam[0], cam[2]);
        let db = b.distance_to(cam[0], cam[2]);
        da.partial_cmp(&db).unwrap().then(b.level.cmp(&a.level))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers_once(keys: &[NodeKey], x: f64, z: f64) -> usize {
        keys.iter()
            .filter(|k| {
                let (x0, z0) = k.corner();
                let s = k.size();
                x >= x0 && x < x0 + s && z >= z0 && z < z0 + s
            })
            .count()
    }

    #[test]
    fn selection_tiles_the_ground_once() {
        let cam = [-905_738.0, 385.0, 522_928.0];
        let keys = select(cam, 1.7);
        assert!(keys.len() < 440, "{} chunks", keys.len());
        assert_eq!(keys[0].level, MAX_LEVEL);
        for &(x, z) in &[(cam[0], cam[2]), (cam[0] + 300.0, cam[2] - 1000.0), (cam[0] - 150_000.0, cam[2] + 90_000.0)] {
            assert_eq!(covers_once(&keys, x, z), 1, "at {x},{z}");
        }
    }

    #[test]
    fn high_flight_is_coarse() {
        let keys = select([0.0, 120_000.0, 0.0], 115_000.0);
        assert!(keys.len() < 250, "{} chunks", keys.len());
        assert!(keys.iter().all(|k| k.level < MAX_LEVEL));
    }

    #[test]
    fn ancestry() {
        let k = NodeKey { level: 5, ix: 9, iz: 20 };
        for c in k.children() {
            assert!(k.contains(&c));
            assert!(!c.contains(&k));
        }
    }
}
