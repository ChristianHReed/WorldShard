//! Builds the world and writes a shaded top-down preview (PPM) plus a
//! 16-bit heightmap (PGM) — handy for checking the generator and for
//! importing the terrain into other tools.
use std::io::Write;
use worldshard::worldgen::grid::{GN, GV};
use worldshard::worldgen::World;

fn main() -> std::io::Result<()> {
    let t0 = std::time::Instant::now();
    let w = World::generate(|_| {});
    eprintln!("generated {}x{} grid in {:.2?}", GV, GV, t0.elapsed());
    let (mut hmin, mut hmax) = (f32::MAX, f32::MIN);
    for &h in &w.h {
        hmin = hmin.min(h);
        hmax = hmax.max(h);
    }
    eprintln!("height range {hmin:.1} m .. {hmax:.1} m");
    for &(x, z) in &[
        (-803.5, 464.0),
        (0.0, 0.0),
        (-112.0, -152.0),
        (858.0, -720.0),
        (-700.0, -600.0),
        (0.0, 820.0),
    ] {
        eprintln!(
            "h({x},{z}) = {:.3}  place = {:?}",
            w.height_at(x, z),
            w.place_name(x, z)
        );
    }
    // shaded colour preview
    let mut ppm = Vec::with_capacity(GV * GV * 3 + 32);
    write!(ppm, "P6\n{GV} {GV}\n255\n")?;
    for j in 0..GV {
        for i in 0..GV {
            let k = j * GV + i;
            let h = w.h[k];
            let hx = w.h[j * GV + (i + 1).min(GN)];
            let hz = w.h[(j + 1).min(GN) * GV + i];
            let shade = if h > 0.0 {
                (1.0 - ((hx - h) + (hz - h)) * 0.03).clamp(0.55, 1.3)
            } else {
                1.0
            };
            let c = if h > 0.0 {
                w.col[k]
            } else {
                let t = (-h / 30.0).clamp(0.0, 1.0);
                [0.55 - 0.4 * t, 0.8 - 0.5 * t, 0.8 - 0.35 * t]
            };
            let g = w.glow[k];
            for ch in 0..3 {
                let mut v = c[ch] * shade;
                if g > 0.0 {
                    v = v + g * [1.0, 0.45, 0.1][ch];
                }
                ppm.push((v.clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
    }
    std::fs::write("world_preview.ppm", ppm)?;
    // 16-bit heightmap, sea level at 0 -> 2048, 0.25 m per step
    let mut pgm = Vec::with_capacity(GV * GV * 2 + 32);
    write!(pgm, "P5\n{GV} {GV}\n65535\n")?;
    for &h in &w.h {
        let v = ((h + 512.0) * 4.0).clamp(0.0, 65535.0) as u16;
        pgm.extend_from_slice(&v.to_be_bytes());
    }
    std::fs::write("world_height16.pgm", pgm)?;
    eprintln!("wrote world_preview.ppm and world_height16.pgm");
    Ok(())
}
