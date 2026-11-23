//! Day and night: where the sun is and what the light, sky and fog look
//! like at a given time of day. Same numbers as the web prototype.
//!
//! World axes: +x east, +y up, +z south (north is -z).

use crate::worldgen::noise::sm;

#[derive(Clone, Copy, Debug)]
pub struct SunState {
    /// Unit vector pointing *towards* the sun.
    pub sun_dir: [f32; 3],
    /// Unit vector pointing towards the moon (opposite the sun).
    pub moon_dir: [f32; 3],
    /// 0 at night, 1 in full day.
    pub day: f32,
    /// Sunlight colour and strength (0..~1 per channel, sRGB-ish).
    pub sun_col: [f32; 3],
    /// Moonlight colour and strength.
    pub moon_col: [f32; 3],
    /// Ambient light from the sky.
    pub sky_amb: [f32; 3],
    /// Background sky colour (sRGB).
    pub sky_col: [f32; 3],
    /// Fog / horizon colour (sRGB).
    pub fog_col: [f32; 3],
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Light and sky for `hours` (0..24). Sunrise ~6:00 in the east, noon
/// overhead (tilted a little to the south), sunset ~18:00 in the west.
pub fn sun_state(hours: f64) -> SunState {
    let a = (hours / 24.0 - 0.25) * std::f64::consts::TAU;
    let sun = norm([a.cos() as f32, a.sin() as f32, 0.32]);
    let moon = [-sun[0], -sun[1], -sun[2]];
    let sy = sun[1] as f64;
    let day = sm(-0.12, 0.18, sy) as f32;
    let warm = sm(0.45, 0.05, sy) as f32;
    let up = sm(-0.05, 0.12, sy) as f32;
    let sun_col = [0.92 * up, (0.86 - 0.26 * warm) * up, (0.76 - 0.42 * warm) * up];
    let mu = sm(-0.05, 0.25, moon[1] as f64) as f32;
    let moon_col = [0.16 * mu, 0.2 * mu, 0.32 * mu];
    let sky_amb = [0.22 * day + 0.08, 0.27 * day + 0.1, 0.36 * day + 0.17];
    let fog_col = mix3([0.05, 0.07, 0.13], [0.70, 0.80, 0.90], day);
    let mut sky_col = mix3([0.012, 0.018, 0.045], [0.45, 0.64, 0.88], day);
    // sunset / sunrise glow
    let low = (1.0 - (sun[1].abs() * 4.0)).clamp(0.0, 1.0);
    sky_col = mix3(sky_col, [0.95, 0.58, 0.36], low * 0.45 * day.max(0.3));
    SunState { sun_dir: sun, moon_dir: moon, day, sun_col, moon_col, sky_amb, sky_col, fog_col }
}

/// A game day lasts one real hour: 40 minutes of daylight (6:00 to 18:00)
/// and 20 minutes of night (18:00 to 6:00).
pub const DAYLIGHT_SECS: f64 = 40.0 * 60.0;
pub const NIGHT_SECS: f64 = 20.0 * 60.0;

/// Game hours that pass in one real second, at game time `hours`.
pub fn clock_rate(hours: f64) -> f64 {
    if (6.0..18.0).contains(&hours.rem_euclid(24.0)) {
        12.0 / DAYLIGHT_SECS
    } else {
        12.0 / NIGHT_SECS
    }
}

/// "09:05" style clock text.
pub fn clock_text(hours: f64) -> String {
    let h = hours.rem_euclid(24.0);
    format!("{:02}:{:02}", h.floor() as u32, ((h.fract()) * 60.0).floor() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_day_lasts_an_hour_with_a_short_night() {
        // run the clock from midnight for one real hour, a quarter second at a time
        let (mut h, mut light) = (0.0f64, 0.0f64);
        for _ in 0..(3600 * 4) {
            if (6.0..18.0).contains(&h) {
                light += 0.25;
            }
            h += 0.25 * clock_rate(h);
        }
        assert!((h - 24.0).abs() < 0.05, "one real hour made {h} game hours");
        assert!((light - DAYLIGHT_SECS).abs() < 5.0, "daylight lasted {light} s");
    }
    #[test]
    fn noon_is_bright_midnight_is_dark() {
        let noon = sun_state(12.0);
        let night = sun_state(0.0);
        assert!(noon.sun_dir[1] > 0.9 && noon.day > 0.99);
        assert!(night.sun_dir[1] < -0.9 && night.day < 0.01);
        assert!(sun_state(6.5).sun_dir[0] > 0.9); // morning sun in the east
    }
    #[test]
    fn clock() {
        assert_eq!(clock_text(9.0), "09:00");
        assert_eq!(clock_text(25.5), "01:30");
    }
}
