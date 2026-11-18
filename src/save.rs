//! The save file: a small JSON object the player can keep anywhere
//! (for example Google Drive). Same format as the web prototype, so saves
//! move between the two:
//!
//! `{"game":"WorldShard","v":1,"x":..,"z":..,"yaw":..,"pitch":..,"hours":..,
//!   "day":..,"visited":["Southwest",..],"playSec":..,"savedAt":..}`
//!
//! Hand-written (no serde) so the library stays dependency-free.

#[derive(Clone, Debug, PartialEq)]
pub struct SaveData {
    pub x: f64,
    pub z: f64,
    pub yaw: f64,
    pub pitch: f64,
    /// Time of day, 0..24.
    pub hours: f64,
    pub day: u32,
    /// Names of the places the player has visited.
    pub visited: Vec<String>,
    pub play_sec: f64,
    /// Milliseconds since 1970, when known.
    pub saved_at: f64,
    /// Fly-through mode (testing) and the height flown at.
    pub fly: bool,
    pub y: f64,
}

/// Browser-storage key shared with the web prototype.
pub const STORAGE_KEY: &str = "worldshard-save-v1";

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o
}

impl SaveData {
    pub fn to_json(&self) -> String {
        let visited: Vec<String> = self.visited.iter().map(|v| format!("\"{}\"", esc(v))).collect();
        format!(
            "{{\"game\":\"WorldShard\",\"v\":2,\"fly\":{},\"y\":{:.1},\"x\":{:.2},\"z\":{:.2},\"yaw\":{:.4},\"pitch\":{:.4},\"hours\":{:.3},\"day\":{},\"visited\":[{}],\"playSec\":{:.0},\"savedAt\":{:.0}}}",
            self.fly,
            if self.fly { self.y } else { 0.0 },
            self.x,
            self.z,
            self.yaw,
            self.pitch,
            self.hours,
            self.day,
            visited.join(","),
            self.play_sec,
            self.saved_at
        )
    }

    /// Reads a save. Accepts extra text around the JSON object and saves
    /// written by the prototype's quick save (which has no "game" field).
    pub fn from_json(text: &str) -> Option<SaveData> {
        let start = text.find('{')?;
        let end = text.rfind('}')?;
        if end <= start {
            return None;
        }
        let s = &text[start..=end];
        if let Some(g) = string_field(s, "game") {
            if g != "WorldShard" {
                return None;
            }
        }
        // v1 saves come from the small first version of the world: move them
        // to the same spot on the full-size island
        let scale = match number_field(s, "v")? as i64 {
            1 => crate::worldgen::terrain::METRES_PER_PX / 4.0,
            2 => 1.0,
            _ => return None,
        };
        let x = number_field(s, "x")? * scale;
        let z = number_field(s, "z")? * scale;
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        Some(SaveData {
            x,
            z,
            yaw: number_field(s, "yaw").unwrap_or(0.0),
            pitch: number_field(s, "pitch").unwrap_or(0.0).clamp(-1.3, 1.3),
            hours: number_field(s, "hours").unwrap_or(9.0).rem_euclid(24.0),
            day: number_field(s, "day").unwrap_or(1.0).max(1.0) as u32,
            visited: string_array(s, "visited").into_iter().take(20).collect(),
            play_sec: number_field(s, "playSec").unwrap_or(0.0),
            saved_at: number_field(s, "savedAt").unwrap_or(0.0),
            fly: s.contains("\"fly\":true"),
            y: number_field(s, "y").filter(|y| y.is_finite()).unwrap_or(0.0),
        })
    }
}

/// Position just after `"key":` (whitespace skipped).
fn value_start(s: &str, key: &str) -> Option<usize> {
    let pat = format!("\"{key}\"");
    let mut from = 0;
    while let Some(i) = s[from..].find(&pat) {
        let mut j = from + i + pat.len();
        let b = s.as_bytes();
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j] == b':' {
            j += 1;
            while j < b.len() && (b[j] as char).is_whitespace() {
                j += 1;
            }
            return Some(j);
        }
        from = from + i + 1;
    }
    None
}

fn number_field(s: &str, key: &str) -> Option<f64> {
    let i = value_start(s, key)?;
    let rest = &s[i..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn read_string(rest: &str) -> Option<(String, usize)> {
    let mut chars = rest.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut out = String::new();
    let mut escape = false;
    for (i, c) in chars {
        if escape {
            out.push(match c {
                'n' => '\n',
                't' => '\t',
                other => other,
            });
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if c == '"' {
            return Some((out, i + 1));
        } else {
            out.push(c);
        }
    }
    None
}

fn string_field(s: &str, key: &str) -> Option<String> {
    let i = value_start(s, key)?;
    read_string(&s[i..]).map(|(v, _)| v)
}

fn string_array(s: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(i) = value_start(s, key) else { return out };
    let mut rest = &s[i..];
    if !rest.starts_with('[') {
        return out;
    }
    rest = &rest[1..];
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') || rest.is_empty() {
            break;
        }
        match read_string(rest) {
            Some((v, used)) => {
                out.push(v);
                rest = rest[used..].trim_start();
                if let Some(r) = rest.strip_prefix(',') {
                    rest = r;
                }
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveData {
        SaveData {
            x: -803.5,
            z: 464.0,
            yaw: 1.047,
            pitch: -0.05,
            hours: 13.5,
            day: 3,
            visited: vec!["Southwest".into(), "Mount \"Everest\"".into()],
            play_sec: 321.0,
            saved_at: 1_763_400_000_000.0,
            fly: true,
            y: 3200.5,
        }
    }

    #[test]
    fn round_trip() {
        let s = sample();
        let back = SaveData::from_json(&s.to_json()).unwrap();
        assert_eq!(back.day, 3);
        assert_eq!(back.visited, s.visited);
        assert!((back.x - s.x).abs() < 0.01 && (back.hours - 13.5).abs() < 0.001);
        assert!(back.fly && (back.y - 3200.5).abs() < 0.1);
    }

    #[test]
    fn reads_prototype_quick_save() {
        let js = r#"{"v":1,"x":-803.5,"z":464,"yaw":1.0472,"pitch":-0.05,"hours":9.25,"day":1,"visited":["Southwest"],"playSec":12,"savedAt":1763420000000}"#;
        let s = SaveData::from_json(js).unwrap();
        assert_eq!(s.visited, vec!["Southwest".to_string()]);
        assert_eq!(s.day, 1);
        // moved to the full-size island
        assert!((s.x - -803.5 * crate::worldgen::terrain::METRES_PER_PX / 4.0).abs() < 1.0);
        assert!(!s.fly);
    }

    #[test]
    fn rejects_other_files() {
        assert!(SaveData::from_json("hello").is_none());
        assert!(SaveData::from_json(r#"{"game":"Other","v":1,"x":1,"z":2}"#).is_none());
        assert!(SaveData::from_json(r#"{"game":"WorldShard","v":3,"x":1,"z":2}"#).is_none());
    }
}
