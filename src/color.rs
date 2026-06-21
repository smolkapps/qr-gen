//! Hex color parsing for QR foreground/background.

use anyhow::{bail, Context, Result};

/// An RGBA color. Alpha is always 255 (opaque) for parsed hex colors,
/// but the struct carries it so we can hand `[u8; 4]` straight to `image`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Rgba = Rgba {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Pixel form for the `image` crate (`Rgba<u8>`).
    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// `#rrggbb` form for SVG output.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Parse a hex color string into [`Rgba`].
///
/// Accepts (with or without a leading `#`):
/// - 3 digits  `rgb`   -> expanded to `rrggbb`
/// - 4 digits  `rgba`
/// - 6 digits  `rrggbb`
/// - 8 digits  `rrggbbaa`
///
/// Named convenience values `black` and `white` are also accepted.
pub fn parse_hex(input: &str) -> Result<Rgba> {
    let raw = input.trim();
    match raw.to_ascii_lowercase().as_str() {
        "black" => return Ok(Rgba::BLACK),
        "white" => return Ok(Rgba::WHITE),
        _ => {}
    }

    let hex = raw.strip_prefix('#').unwrap_or(raw);

    if hex.is_empty() {
        bail!("empty color string");
    }
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("invalid hex color '{input}': only 0-9 / a-f digits allowed (e.g. #ff0000)");
    }

    let nib = |c: char| -> u8 {
        // Safe: we validated all chars are ascii hexdigits above.
        c.to_digit(16).unwrap() as u8
    };
    let dup = |c: char| -> u8 {
        let v = nib(c);
        (v << 4) | v
    };

    let chars: Vec<char> = hex.chars().collect();
    let (r, g, b, a) = match chars.len() {
        3 => (dup(chars[0]), dup(chars[1]), dup(chars[2]), 255u8),
        4 => (dup(chars[0]), dup(chars[1]), dup(chars[2]), dup(chars[3])),
        6 => (
            byte(&hex[0..2])?,
            byte(&hex[2..4])?,
            byte(&hex[4..6])?,
            255u8,
        ),
        8 => (
            byte(&hex[0..2])?,
            byte(&hex[2..4])?,
            byte(&hex[4..6])?,
            byte(&hex[6..8])?,
        ),
        n => bail!("invalid hex color '{input}': expected 3, 4, 6 or 8 hex digits, got {n}"),
    };

    Ok(Rgba { r, g, b, a })
}

fn byte(s: &str) -> Result<u8> {
    u8::from_str_radix(s, 16).with_context(|| format!("invalid hex byte '{s}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_six_digit() {
        assert_eq!(
            parse_hex("#ff0000").unwrap(),
            Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            parse_hex("00ff00").unwrap(),
            Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
    }

    #[test]
    fn parses_three_digit_shorthand() {
        assert_eq!(
            parse_hex("#f00").unwrap(),
            Rgba {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            parse_hex("0f0").unwrap(),
            Rgba {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
    }

    #[test]
    fn parses_eight_digit_with_alpha() {
        assert_eq!(
            parse_hex("#11223344").unwrap(),
            Rgba {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                a: 0x44
            }
        );
    }

    #[test]
    fn parses_named() {
        assert_eq!(parse_hex("black").unwrap(), Rgba::BLACK);
        assert_eq!(parse_hex("WHITE").unwrap(), Rgba::WHITE);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_hex("#zzzzzz").is_err());
        assert!(parse_hex("nope").is_err());
        assert!(parse_hex("#12345").is_err()); // 5 digits
        assert!(parse_hex("").is_err());
    }

    #[test]
    fn roundtrips_to_hex() {
        assert_eq!(parse_hex("#abcdef").unwrap().to_hex(), "#abcdef");
    }
}
