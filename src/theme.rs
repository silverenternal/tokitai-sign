use crossterm::style::Color;
use std::{fs, io};

pub const SPINNER_COLOR: Color = Color::Rgb {
    r: 116,
    g: 205,
    b: 255,
};
pub const ACCENT_COLOR: Color = Color::Rgb {
    r: 152,
    g: 220,
    b: 255,
};
pub const DIM_COLOR: Color = Color::Rgb {
    r: 78,
    g: 132,
    b: 184,
};

pub const LOGO_BASE: Color = Color::Rgb {
    r: 52,
    g: 151,
    b: 226,
};
pub const LOGO_SHADOW: Color = Color::Rgb {
    r: 44,
    g: 128,
    b: 198,
};
pub const LOGO_HIGHLIGHT: Color = Color::Rgb {
    r: 204,
    g: 244,
    b: 255,
};
pub const TRAIL_HEAD: Color = Color::Rgb {
    r: 218,
    g: 248,
    b: 255,
};
pub const TRAIL_BODY: Color = Color::Rgb {
    r: 92,
    g: 194,
    b: 246,
};
pub const TRAIL_TAIL: Color = Color::Rgb {
    r: 36,
    g: 110,
    b: 178,
};

pub fn loader_gradient() -> Vec<(u8, u8, u8)> {
    (0u8..=50)
        .step_by(2)
        .map(|value| {
            let blue = 45u8.saturating_add(value.saturating_mul(4));
            let green = value.saturating_mul(2);
            (0, green, blue)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub logo_base: Color,
    pub logo_shadow: Color,
    pub logo_highlight: Color,
    pub trail_head: Color,
    pub trail_body: Color,
    pub trail_tail: Color,
    pub accent: Color,
    pub dim: Color,
    pub contrast_floor: f32,
    pub trail_span: f32,
    pub rhythm_intensity: f32,
}

impl Theme {
    pub fn load(path: Option<&str>) -> io::Result<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };

        let mut theme = Self::default();
        for (line_index, line) in fs::read_to_string(path)?.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(invalid_theme(line_index, "expected key=value"));
            };

            theme.set(key.trim(), value.trim(), line_index)?;
        }

        Ok(theme)
    }

    fn set(&mut self, key: &str, value: &str, line_index: usize) -> io::Result<()> {
        match key {
            "logo_base" => self.logo_base = parse_hex_color(value, line_index)?,
            "logo_shadow" => self.logo_shadow = parse_hex_color(value, line_index)?,
            "logo_highlight" => self.logo_highlight = parse_hex_color(value, line_index)?,
            "trail_head" => self.trail_head = parse_hex_color(value, line_index)?,
            "trail_body" => self.trail_body = parse_hex_color(value, line_index)?,
            "trail_tail" => self.trail_tail = parse_hex_color(value, line_index)?,
            "accent" => self.accent = parse_hex_color(value, line_index)?,
            "dim" => self.dim = parse_hex_color(value, line_index)?,
            "contrast_floor" => self.contrast_floor = parse_f32(value, line_index, 0.16, 0.7)?,
            "trail_span" => self.trail_span = parse_f32(value, line_index, 8.0, 96.0)?,
            "rhythm_intensity" => {
                self.rhythm_intensity = parse_f32(value, line_index, 0.0, 1.4)?;
            }
            _ => return Err(invalid_theme(line_index, "unknown key")),
        }

        Ok(())
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            logo_base: LOGO_BASE,
            logo_shadow: LOGO_SHADOW,
            logo_highlight: LOGO_HIGHLIGHT,
            trail_head: TRAIL_HEAD,
            trail_body: TRAIL_BODY,
            trail_tail: TRAIL_TAIL,
            accent: ACCENT_COLOR,
            dim: DIM_COLOR,
            contrast_floor: 0.33,
            trail_span: 34.0,
            rhythm_intensity: 1.0,
        }
    }
}

fn parse_hex_color(value: &str, line_index: usize) -> io::Result<Color> {
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 {
        return Err(invalid_theme(line_index, "expected #RRGGBB color"));
    }

    let r = u8::from_str_radix(&value[0..2], 16)
        .map_err(|_| invalid_theme(line_index, "invalid red channel"))?;
    let g = u8::from_str_radix(&value[2..4], 16)
        .map_err(|_| invalid_theme(line_index, "invalid green channel"))?;
    let b = u8::from_str_radix(&value[4..6], 16)
        .map_err(|_| invalid_theme(line_index, "invalid blue channel"))?;

    Ok(Color::Rgb { r, g, b })
}

fn parse_f32(value: &str, line_index: usize, min: f32, max: f32) -> io::Result<f32> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| invalid_theme(line_index, "invalid number"))?;
    if !(min..=max).contains(&parsed) {
        return Err(invalid_theme(line_index, "number outside allowed range"));
    }

    Ok(parsed)
}

fn invalid_theme(line_index: usize, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid theme line {}: {}", line_index + 1, reason),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_loader_gradient_endpoints() {
        let gradient = loader_gradient();

        assert_eq!(gradient.first(), Some(&(0, 0, 45)));
        assert_eq!(gradient.last(), Some(&(0, 100, 245)));
    }

    #[test]
    fn default_theme_uses_embedded_colors() {
        let theme = Theme::default();

        assert_eq!(theme.logo_base, LOGO_BASE);
        assert_eq!(theme.trail_span, 34.0);
        assert!(theme.contrast_floor >= 0.3);
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            parse_hex_color("#1234ab", 0).unwrap(),
            Color::Rgb {
                r: 0x12,
                g: 0x34,
                b: 0xab
            }
        );
        assert!(parse_hex_color("bad", 0).is_err());
    }
}
