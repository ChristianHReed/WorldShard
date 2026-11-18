//! Checks the full-size world and writes a shaded top-down preview (PPM)
//! plus a 16-bit heightmap (PGM, 1 m per step, sea level at 10,000).
use std::io::Write;
use worldshard::lod;
use worldshard::meshgen::*;
use worldshard::worldgen::island::{EX, EY, HX, HY};
use worldshard::worldgen::terrain::*;

fn main() -> std::io::Result<()> {
    let t0 = std::time::Instant::now();
    let t = Terrain::new();
    eprintln!("world ready in {:.2?}", t0.elapsed());
    let (ex, ez) = to_world(EX, EY);
    let (hx, hz) = to_world(HX, HY);
    eprintln!("Mount Everest {:.0} m, St. Helens centre {:.0} m", t.height(ex, ez), t.height(hx, hz));
    eprintln!("Cloudwall at {:.0} km from the centre, island about {:.0} km wide", CLOUD_R / 1000.0, ISLAND_WIDTH / 1000.0);
    let (sx, sz) = spawn_point();
    eprintln!("spawn ({sx:.0}, {sz:.0}) at {:.0} m: {:?}", t.height(sx, sz), t.place_name(sx, sz));

    // what the game builds around the spawn point
    let t1 = std::time::Instant::now();
    let keys = lod::select([sx, t.height(sx, sz) + 1.7, sz], 1.7);
    let mut tris = 0;
    for &k in &keys {
        let c = build_chunk(&t, k);
        tris += c.terrain.map_or(0, |m| m.triangle_count()) + c.water.map_or(0, |m| m.triangle_count()) + c.lava.map_or(0, |m| m.triangle_count());
    }
    eprintln!("{} terrain chunks, {} triangles, built in {:.2?}", keys.len(), tris, t1.elapsed());

    // preview: 1 pixel = 4 km over ±2,200 km
    const N: usize = 1100;
    let step = 4000.0;
    let o = -(N as f64) / 2.0 * step;
    let mut ppm = Vec::with_capacity(N * N * 3 + 32);
    write!(ppm, "P6\n{N} {N}\n255\n")?;
    let mut pgm = Vec::with_capacity(N * N * 2 + 32);
    write!(pgm, "P5\n{N} {N}\n65535\n")?;
    for j in 0..N {
        for i in 0..N {
            let (x, z) = (o + i as f64 * step, o + j as f64 * step);
            let s = t.sample(x, z);
            let hx = t.height(x + step, z);
            let shade = if s.h > 0.0 { (1.0 - (hx - s.h) / 1500.0).clamp(0.55, 1.35) as f32 } else { 1.0 };
            let c = if s.water.is_some() && s.h < s.water.unwrap() {
                let d = ((s.water.unwrap() - s.h) / 300.0).clamp(0.0, 1.0) as f32;
                [0.4 - 0.32 * d, 0.78 - 0.5 * d, 0.8 - 0.35 * d]
            } else {
                s.col
            };
            for ch in 0..3 {
                let v = c[ch] * shade + s.glow * [1.0, 0.45, 0.1][ch];
                ppm.push((v.clamp(0.0, 1.0) * 255.0) as u8);
            }
            let v = (s.h + 10_000.0).clamp(0.0, 65535.0) as u16;
            pgm.extend_from_slice(&v.to_be_bytes());
        }
    }
    std::fs::write("world_preview.ppm", ppm)?;
    std::fs::write("world_height16.pgm", pgm)?;
    eprintln!("wrote world_preview.ppm and world_height16.pgm in {:.2?}", t0.elapsed());
    Ok(())
}
