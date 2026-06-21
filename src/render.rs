//! Core QR rendering: payload string -> PNG bytes or SVG string.

use anyhow::{bail, Context, Result};
use qrcode::types::EcLevel;
use qrcode::QrCode;

use crate::color::Rgba;

/// Error-correction level, parsed from the CLI `--ecc` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecc {
    Low,
    Medium,
    Quartile,
    High,
}

impl Ecc {
    pub fn to_qr(self) -> EcLevel {
        match self {
            Ecc::Low => EcLevel::L,
            Ecc::Medium => EcLevel::M,
            Ecc::Quartile => EcLevel::Q,
            Ecc::High => EcLevel::H,
        }
    }
}

impl std::str::FromStr for Ecc {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "l" | "low" => Ok(Ecc::Low),
            "m" | "medium" => Ok(Ecc::Medium),
            "q" | "quartile" => Ok(Ecc::Quartile),
            "h" | "high" => Ok(Ecc::High),
            other => bail!("invalid --ecc '{other}': expected one of l, m, q, h"),
        }
    }
}

/// Everything needed to render one QR code.
#[derive(Debug, Clone)]
pub struct RenderOpts {
    /// Target minimum image edge length in pixels (the renderer scales modules
    /// up to at least this; actual output may be a bit larger to keep modules
    /// an integer number of pixels).
    pub size: u32,
    pub ecc: Ecc,
    pub fg: Rgba,
    pub bg: Rgba,
    /// Quiet-zone width in modules. The spec recommends 4; 0 disables it.
    pub quiet_zone: u32,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts {
            size: 512,
            ecc: Ecc::Medium,
            fg: Rgba::BLACK,
            bg: Rgba::WHITE,
            quiet_zone: 4,
        }
    }
}

/// Build a [`QrCode`] from the payload, surfacing the common "too much data"
/// failure with a friendly message.
fn encode(payload: &str, ecc: Ecc) -> Result<QrCode> {
    if payload.is_empty() {
        bail!("cannot encode an empty payload");
    }
    QrCode::with_error_correction_level(payload.as_bytes(), ecc.to_qr()).with_context(|| {
        format!(
            "failed to encode {} bytes at ECC level {:?} — payload too large for a single QR code; \
             try a lower --ecc (l) or shorter input",
            payload.len(),
            ecc
        )
    })
}

/// Render the payload to PNG bytes.
pub fn render_png(payload: &str, opts: &RenderOpts) -> Result<Vec<u8>> {
    use image::{ImageEncoder, Rgba as ImgRgba};

    let code = encode(payload, opts.ecc)?;

    let img = code
        .render::<ImgRgba<u8>>()
        .min_dimensions(opts.size, opts.size)
        .quiet_zone(opts.quiet_zone > 0)
        .dark_color(ImgRgba(opts.fg.to_array()))
        .light_color(ImgRgba(opts.bg.to_array()))
        .build();

    // The qrcode builder bakes a fixed quiet-zone width when enabled. If the
    // user asked for a non-default quiet zone (and a non-zero one), re-render
    // with a hand-laid border so the requested width is honored exactly.
    let img = if opts.quiet_zone != 4 && opts.quiet_zone != 0 {
        render_png_custom_quiet(&code, opts)?
    } else {
        img
    };

    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .context("failed to encode PNG")?;
    Ok(bytes)
}

/// Hand-render a PNG with an exact quiet-zone width (in modules), used when the
/// requested quiet zone differs from the builder's fixed default.
fn render_png_custom_quiet(
    code: &QrCode,
    opts: &RenderOpts,
) -> Result<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
    use image::{ImageBuffer, Rgba as ImgRgba};

    let modules = module_matrix(code);
    let n = modules.len() as u32; // QR is square: n x n modules
    let qz = opts.quiet_zone;
    let total_modules = n + 2 * qz;

    // Choose an integer module size so the image is at least `opts.size` px.
    let scale = opts.size.div_ceil(total_modules).max(1);
    let dim = total_modules * scale;

    let fg = ImgRgba(opts.fg.to_array());
    let bg = ImgRgba(opts.bg.to_array());

    let mut img = ImageBuffer::from_pixel(dim, dim, bg);
    for (y, row) in modules.iter().enumerate() {
        for (x, &dark) in row.iter().enumerate() {
            if !dark {
                continue;
            }
            let px0 = (qz + x as u32) * scale;
            let py0 = (qz + y as u32) * scale;
            for dy in 0..scale {
                for dx in 0..scale {
                    img.put_pixel(px0 + dx, py0 + dy, fg);
                }
            }
        }
    }
    Ok(img)
}

/// Render the payload to an SVG document (as a String).
///
/// Hand-rendered as one `<rect>` per dark module on a background rect, which
/// keeps the file small-ish and dependency-free, and lets us honor an exact
/// quiet-zone width and arbitrary colors.
pub fn render_svg(payload: &str, opts: &RenderOpts) -> Result<String> {
    let code = encode(payload, opts.ecc)?;
    let modules = module_matrix(&code);
    let n = modules.len() as u32;
    let qz = opts.quiet_zone;
    let total = n + 2 * qz;

    // Pick a module pixel size so the viewport is at least `opts.size`.
    let scale = opts.size.div_ceil(total).max(1);
    let dim = total * scale;

    let fg = opts.fg.to_hex();
    let bg = opts.bg.to_hex();

    let mut svg = String::with_capacity(1024 + (n * n) as usize * 48);
    svg.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{dim}\" height=\"{dim}\" \
         viewBox=\"0 0 {dim} {dim}\" shape-rendering=\"crispEdges\">\n"
    ));
    // Background.
    svg.push_str(&format!(
        "<rect x=\"0\" y=\"0\" width=\"{dim}\" height=\"{dim}\" fill=\"{bg}\"/>\n"
    ));
    // Dark modules grouped under a single fill for compactness.
    svg.push_str(&format!("<g fill=\"{fg}\">\n"));
    for (y, row) in modules.iter().enumerate() {
        for (x, &dark) in row.iter().enumerate() {
            if !dark {
                continue;
            }
            let px = (qz + x as u32) * scale;
            let py = (qz + y as u32) * scale;
            svg.push_str(&format!(
                "<rect x=\"{px}\" y=\"{py}\" width=\"{scale}\" height=\"{scale}\"/>\n"
            ));
        }
    }
    svg.push_str("</g>\n</svg>\n");
    Ok(svg)
}

/// Extract the module matrix (true = dark) from a [`QrCode`] as a square
/// `Vec<Vec<bool>>`, with no quiet zone.
fn module_matrix(code: &QrCode) -> Vec<Vec<bool>> {
    let width = code.width();
    let colors = code.to_colors();
    let mut rows = Vec::with_capacity(width);
    for y in 0..width {
        let mut row = Vec::with_capacity(width);
        for x in 0..width {
            row.push(colors[y * width + x] == qrcode::Color::Dark);
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecc_parses() {
        assert_eq!("l".parse::<Ecc>().unwrap(), Ecc::Low);
        assert_eq!("M".parse::<Ecc>().unwrap(), Ecc::Medium);
        assert_eq!("quartile".parse::<Ecc>().unwrap(), Ecc::Quartile);
        assert_eq!("H".parse::<Ecc>().unwrap(), Ecc::High);
        assert!("z".parse::<Ecc>().is_err());
    }

    #[test]
    fn png_is_valid_and_nonempty() {
        let opts = RenderOpts::default();
        let png = render_png("https://example.com", &opts).unwrap();
        assert!(png.len() > 100);
        // PNG magic bytes.
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn svg_contains_markup() {
        let opts = RenderOpts::default();
        let svg = render_svg("https://example.com", &opts).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn empty_payload_errors() {
        assert!(render_png("", &RenderOpts::default()).is_err());
        assert!(render_svg("", &RenderOpts::default()).is_err());
    }

    #[test]
    fn custom_quiet_zone_changes_size() {
        let small = RenderOpts {
            quiet_zone: 0,
            ..Default::default()
        };
        let big = RenderOpts {
            quiet_zone: 10,
            ..Default::default()
        };
        // Both decode-able; just assert PNGs differ in length (different geometry).
        let a = render_png("hello", &small).unwrap();
        let b = render_png("hello", &big).unwrap();
        assert_ne!(a.len(), b.len());
    }
}
