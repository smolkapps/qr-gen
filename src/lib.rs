//! qr-gen library core. The CLI in `main.rs` is a thin wrapper over these.
//!
//! Everything here is fully offline: no network, no API calls.

pub mod batch;
pub mod color;
pub mod render;
pub mod vcard;

pub use color::{parse_hex, Rgba};
pub use render::{render_png, render_svg, Ecc, RenderOpts};
pub use vcard::build_vcard;
