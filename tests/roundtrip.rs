//! The strong correctness test: generate a PNG QR, decode it back with rqrr,
//! and assert the decoded payload equals the input. Covers a URL, a long
//! string, and a vCard. Also exercises SVG markup directly via the library.

use qr_gen::render::{render_png, render_svg, Ecc, RenderOpts};
use qr_gen::vcard::build_vcard;

/// Decode the single QR code present in PNG bytes, returning its text payload.
fn decode_png(png: &[u8]) -> String {
    let dynimg = image::load_from_memory(png).expect("decode PNG bytes into image");
    let gray = dynimg.to_luma8();
    let mut img = rqrr::PreparedImage::prepare(gray);
    let grids = img.detect_grids();
    assert!(
        !grids.is_empty(),
        "rqrr found no QR grid in the generated image"
    );
    let (_meta, content) = grids[0].decode().expect("rqrr decode the QR grid");
    content
}

#[test]
fn roundtrip_url() {
    let payload = "https://example.com/path?query=value&x=1";
    let png = render_png(payload, &RenderOpts::default()).unwrap();
    let decoded = decode_png(&png);
    assert_eq!(decoded, payload, "decoded URL must equal input");
}

#[test]
fn roundtrip_long_string() {
    // ~400 chars — pushes into a higher QR version, good stress test.
    let payload: String = "The quick brown fox jumps over the lazy dog. "
        .repeat(9)
        .chars()
        .take(400)
        .collect();
    assert!(payload.len() >= 350);
    let png = render_png(&payload, &RenderOpts::default()).unwrap();
    let decoded = decode_png(&png);
    assert_eq!(decoded, payload, "decoded long string must equal input");
}

#[test]
fn roundtrip_vcard() {
    let vcard = build_vcard(&[
        ("name".into(), "Jane Doe".into()),
        ("email".into(), "jane@x.com".into()),
        ("phone".into(), "+15551234".into()),
        ("org".into(), "Acme Corp".into()),
    ])
    .unwrap();

    let png = render_png(&vcard, &RenderOpts::default()).unwrap();
    let decoded = decode_png(&png);
    assert_eq!(decoded, vcard, "decoded vCard must byte-equal the source");
    // Sanity: the decoded content really is a vCard.
    assert!(decoded.starts_with("BEGIN:VCARD"));
    assert!(decoded.contains("FN:Jane Doe"));
    assert!(decoded.contains("EMAIL;TYPE=INTERNET:jane@x.com"));
}

#[test]
fn roundtrip_at_each_ecc_level() {
    let payload = "ecc-level-roundtrip-check";
    for ecc in [Ecc::Low, Ecc::Medium, Ecc::Quartile, Ecc::High] {
        let opts = RenderOpts {
            ecc,
            ..Default::default()
        };
        let png = render_png(payload, &opts).unwrap();
        let decoded = decode_png(&png);
        assert_eq!(decoded, payload, "roundtrip failed at ECC {ecc:?}");
    }
}

#[test]
fn roundtrip_custom_quiet_zone() {
    // Exercise the hand-laid quiet-zone path (qz != 4, != 0).
    let payload = "https://quiet.example/zone";
    let opts = RenderOpts {
        quiet_zone: 8,
        ..Default::default()
    };
    let png = render_png(payload, &opts).unwrap();
    let decoded = decode_png(&png);
    assert_eq!(decoded, payload);
}

#[test]
fn svg_is_nonempty_with_expected_markup() {
    let svg = render_svg("https://example.com", &RenderOpts::default()).unwrap();
    assert!(!svg.is_empty());
    assert!(svg.contains("<svg"));
    assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("<rect"));
    assert!(svg.contains("</svg>"));
    // Default colors present.
    assert!(svg.contains("#000000"));
    assert!(svg.contains("#ffffff"));
}

#[test]
fn svg_honors_custom_colors() {
    let opts = RenderOpts {
        fg: qr_gen::color::parse_hex("#112233").unwrap(),
        bg: qr_gen::color::parse_hex("#ffeedd").unwrap(),
        ..Default::default()
    };
    let svg = render_svg("color-check", &opts).unwrap();
    assert!(svg.contains("#112233"));
    assert!(svg.contains("#ffeedd"));
}
