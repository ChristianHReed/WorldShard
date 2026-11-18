//! Seeded random numbers and Perlin noise, bit-for-bit compatible with the
//! JavaScript prototype and the design map.

/// Park–Miller "minimal standard" generator (same as the JS `rnd()`).
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    /// Next value in (0, 1).
    pub fn next(&mut self) -> f64 {
        self.0 = (self.0 * 16807) % 2_147_483_647;
        self.0 as f64 / 2_147_483_647.0
    }
}

/// Improved Perlin noise with a seeded permutation table.
pub struct Noise {
    p: [u8; 512],
}

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}
#[inline]
fn g2(h: u8, x: f64, y: f64) -> f64 {
    match h & 7 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        _ => -y,
    }
}
#[inline]
fn g3(h: u8, x: f64, y: f64, z: f64) -> f64 {
    let hh = h & 15;
    let u = if hh < 8 { x } else { y };
    let v = if hh < 4 {
        y
    } else if hh == 12 || hh == 14 {
        x
    } else {
        z
    };
    (if hh & 1 != 0 { -u } else { u }) + (if hh & 2 != 0 { -v } else { v })
}

impl Noise {
    /// Builds the permutation table, consuming the first 255 draws of `rng`
    /// exactly like the JS version does.
    pub fn new(rng: &mut Rng) -> Self {
        let mut perm: [u8; 256] = [0; 256];
        for (i, v) in perm.iter_mut().enumerate() {
            *v = i as u8;
        }
        for i in (1..256usize).rev() {
            let j = (rng.next() * (i as f64 + 1.0)).floor() as usize;
            perm.swap(i, j);
        }
        let mut p = [0u8; 512];
        for i in 0..512 {
            p[i] = perm[i & 255];
        }
        Noise { p }
    }

    /// Raw permutation table entry (used for the field patchwork).
    #[inline]
    pub fn perm(&self, i: usize) -> u8 {
        self.p[i]
    }

    #[inline]
    fn pi(&self, i: usize) -> usize {
        self.p[i] as usize
    }

    /// 2D Perlin noise, roughly in [-1, 1].
    pub fn n2(&self, x: f64, y: f64) -> f64 {
        let fx = x.floor();
        let fy = y.floor();
        let (x, y) = (x - fx, y - fy);
        let xi = (fx as i64 & 255) as usize;
        let yi = (fy as i64 & 255) as usize;
        let (u, v) = (fade(x), fade(y));
        let aa = self.p[self.pi(xi) + yi];
        let ab = self.p[self.pi(xi) + yi + 1];
        let ba = self.p[self.pi(xi + 1) + yi];
        let bb = self.p[self.pi(xi + 1) + yi + 1];
        lerp(
            lerp(g2(aa, x, y), g2(ba, x - 1.0, y), u),
            lerp(g2(ab, x, y - 1.0), g2(bb, x - 1.0, y - 1.0), u),
            v,
        )
    }

    /// 3D Perlin noise, roughly in [-1, 1].
    pub fn n3(&self, x: f64, y: f64, z: f64) -> f64 {
        let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
        let (x, y, z) = (x - fx, y - fy, z - fz);
        let xi = (fx as i64 & 255) as usize;
        let yi = (fy as i64 & 255) as usize;
        let zi = (fz as i64 & 255) as usize;
        let (u, v, w) = (fade(x), fade(y), fade(z));
        let a = self.pi(xi) + yi;
        let aa = self.pi(a) + zi;
        let ab = self.pi(a + 1) + zi;
        let b = self.pi(xi + 1) + yi;
        let ba = self.pi(b) + zi;
        let bb = self.pi(b + 1) + zi;
        let p = &self.p;
        lerp(
            lerp(
                lerp(g3(p[aa], x, y, z), g3(p[ba], x - 1.0, y, z), u),
                lerp(g3(p[ab], x, y - 1.0, z), g3(p[bb], x - 1.0, y - 1.0, z), u),
                v,
            ),
            lerp(
                lerp(
                    g3(p[aa + 1], x, y, z - 1.0),
                    g3(p[ba + 1], x - 1.0, y, z - 1.0),
                    u,
                ),
                lerp(
                    g3(p[ab + 1], x, y - 1.0, z - 1.0),
                    g3(p[bb + 1], x - 1.0, y - 1.0, z - 1.0),
                    u,
                ),
                v,
            ),
            w,
        )
    }

    /// Fractal Brownian motion: `o` octaves of `n2`, normalised.
    pub fn fbm(&self, x: f64, y: f64, o: u32) -> f64 {
        let (mut s, mut a, mut f, mut m) = (0.0, 0.5, 1.0, 0.0);
        for k in 0..o {
            s += a * self.n2(x * f + k as f64 * 17.3, y * f);
            m += a;
            a *= 0.5;
            f *= 2.03;
        }
        s / m
    }

    /// Ridged multifractal: sharp crests, used for mountains and erosion.
    pub fn ridged(&self, x: f64, y: f64, o: u32) -> f64 {
        let (mut s, mut a, mut f, mut m) = (0.0, 0.5, 1.0, 0.0);
        for k in 0..o {
            let n = 1.0 - self.n2(x * f + k as f64 * 9.7, y * f).abs();
            s += a * n * n;
            m += a;
            a *= 0.5;
            f *= 2.07;
        }
        s / m
    }
}

/// Smoothstep from `a` to `b` (works for a > b too, like the JS helper).
#[inline]
pub fn sm(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed angle difference in degrees, wrapped to [-180, 180).
#[inline]
pub fn angd(d: f64) -> f64 {
    ((d + 540.0) % 360.0) - 180.0
}
