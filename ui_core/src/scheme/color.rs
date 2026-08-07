use serde::{Deserialize, Serialize};
use skia_safe::Color4f;

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "String")]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl TryFrom<String> for Color {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        let hex = s.trim_start_matches('#');
        let parse = |slice: &str| {
            u8::from_str_radix(slice, 16)
                .map(|v| v as f32 / 255.0)
                .map_err(|e| e.to_string())
        };
        match hex.len() {
            6 => Ok(Color {
                r: parse(&hex[0..2])?,
                g: parse(&hex[2..4])?,
                b: parse(&hex[4..6])?,
                a: 1.0,
            }),
            8 => Ok(Color {
                r: parse(&hex[0..2])?,
                g: parse(&hex[2..4])?,
                b: parse(&hex[4..6])?,
                a: parse(&hex[6..8])?,
            }),
            _ => Err(format!("invalid color: {s}")),
        }
    }
}

impl From<Color> for Color4f {
    fn from(val: Color) -> Self {
        Color4f {
            r: val.r,
            g: val.g,
            b: val.b,
            a: val.a,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_parse_6digit() {
        let c = Color::try_from("#6750A4".to_string()).unwrap();
        assert!((c.r - 103.0 / 255.0).abs() < 1e-4);
        assert!((c.g - 80.0 / 255.0).abs() < 1e-4);
        assert!((c.b - 164.0 / 255.0).abs() < 1e-4);
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn test_color_parse_8digit() {
        let c = Color::try_from("#6750A480".to_string()).unwrap();
        assert!((c.a - 128.0 / 255.0).abs() < 1e-4);
    }

    #[test]
    fn test_color_invalid() {
        assert!(Color::try_from("#ZZZ".to_string()).is_err());
    }

    #[test]
    fn test_color_default_is_transparent() {
        let c = Color::default();
        assert_eq!(c.a, 0.0);
    }
}
