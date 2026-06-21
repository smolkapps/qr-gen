//! qr-gen — generate QR codes (PNG + SVG), single or batch. Fully offline.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::Parser;

use qr_gen::batch::{self, BatchItem};
use qr_gen::color::parse_hex;
use qr_gen::render::{render_png, render_svg, Ecc, RenderOpts};
use qr_gen::vcard::build_vcard;

/// Output format for a QR code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Png,
    Svg,
}

#[derive(Parser, Debug)]
#[command(
    name = "qr-gen",
    version,
    about = "Generate QR codes (PNG + SVG), single or batch — fully offline.",
    long_about = "Generate QR codes from text/URLs/vCards to PNG or SVG.\n\
                  No network access is ever used.\n\n\
                  Examples:\n  \
                  qr-gen \"https://example.com\" -o out.png\n  \
                  qr-gen \"https://example.com\" -o out.svg --ecc h --fg '#222' --bg '#eee'\n  \
                  qr-gen --batch lines.txt --outdir ./qrs\n  \
                  qr-gen --batch data.csv --outdir ./qrs --svg\n  \
                  qr-gen --vcard name=\"Jane Doe\" email=jane@x.com phone=+15551234 -o jane.png"
)]
struct Cli {
    /// Text or URL to encode (omit when using --batch or --vcard).
    payload: Option<String>,

    /// Output file path. Format is inferred from the extension (.png/.svg)
    /// unless overridden by --svg. Defaults to qr.png for single mode.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Emit SVG instead of PNG.
    #[arg(long)]
    svg: bool,

    /// Minimum image edge length in pixels (module/quiet-zone scaling).
    #[arg(long, default_value_t = 512, value_name = "PX")]
    size: u32,

    /// Error-correction level: l, m, q, or h.
    #[arg(long, default_value = "m", value_name = "L|M|Q|H")]
    ecc: Ecc,

    /// Foreground (module) color as a hex string, e.g. #000000.
    #[arg(long, default_value = "#000000", value_name = "HEX")]
    fg: String,

    /// Background color as a hex string, e.g. #ffffff.
    #[arg(long, default_value = "#ffffff", value_name = "HEX")]
    bg: String,

    /// Quiet-zone width in modules (spec recommends 4; 0 disables).
    #[arg(long, default_value_t = 4, value_name = "MODULES")]
    quiet_zone: u32,

    /// Batch mode: a .txt file (one payload per line) or a .csv file
    /// (with a `text`/first column and optional `filename` column).
    #[arg(long, value_name = "FILE", conflicts_with = "payload")]
    batch: Option<PathBuf>,

    /// Output directory for batch mode (created if missing).
    #[arg(long, value_name = "DIR")]
    outdir: Option<PathBuf>,

    /// vCard mode: key=value pairs (name=, email=, phone=, org=, title=,
    /// url=, address=, note=). `name` is required.
    #[arg(long = "vcard", value_name = "KEY=VALUE", num_args = 1.., conflicts_with_all = ["payload", "batch"])]
    vcard: Option<Vec<String>>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Print the full anyhow chain to stderr.
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    // Parse colors up front so a bad value fails fast for every mode.
    let fg = parse_hex(&cli.fg).context("invalid --fg color")?;
    let bg = parse_hex(&cli.bg).context("invalid --bg color")?;

    if cli.size == 0 {
        bail!("--size must be greater than 0");
    }

    let opts = RenderOpts {
        size: cli.size,
        ecc: cli.ecc,
        fg,
        bg,
        quiet_zone: cli.quiet_zone,
    };

    // Dispatch by mode. clap's conflicts guarantee at most one of
    // payload / batch / vcard is set, but we still check explicitly.
    if let Some(items) = &cli.vcard {
        run_vcard(items, &cli, &opts)
    } else if let Some(batch_path) = &cli.batch {
        run_batch(batch_path, &cli, &opts)
    } else if let Some(payload) = &cli.payload {
        run_single(payload, &cli, &opts)
    } else {
        bail!(
            "no input given. Provide a payload string, --batch <file>, or --vcard key=value...\n\
             Run `qr-gen --help` for usage."
        );
    }
}

/// Decide the output format from the explicit flag and/or the output path.
fn resolve_format(svg_flag: bool, output: Option<&Path>) -> Format {
    if svg_flag {
        return Format::Svg;
    }
    if let Some(p) = output {
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("svg") {
                return Format::Svg;
            }
        }
    }
    Format::Png
}

fn default_extension(fmt: Format) -> &'static str {
    match fmt {
        Format::Png => "png",
        Format::Svg => "svg",
    }
}

fn run_single(payload: &str, cli: &Cli, opts: &RenderOpts) -> Result<()> {
    let fmt = resolve_format(cli.svg, cli.output.as_deref());
    let out = cli
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("qr.{}", default_extension(fmt))));
    write_one(payload, fmt, &out, opts)?;
    println!("Wrote {}", out.display());
    Ok(())
}

fn run_vcard(pairs_raw: &[String], cli: &Cli, opts: &RenderOpts) -> Result<()> {
    let pairs = parse_kv_pairs(pairs_raw)?;
    let vcard = build_vcard(&pairs)?;

    let fmt = resolve_format(cli.svg, cli.output.as_deref());
    let out = cli
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("vcard.{}", default_extension(fmt))));
    write_one(&vcard, fmt, &out, opts)?;
    println!("Wrote {} (vCard)", out.display());
    Ok(())
}

fn run_batch(batch_path: &Path, cli: &Cli, opts: &RenderOpts) -> Result<()> {
    let outdir = cli.outdir.clone().unwrap_or_else(|| PathBuf::from("qrs"));
    std::fs::create_dir_all(&outdir)
        .with_context(|| format!("cannot create output directory {}", outdir.display()))?;

    let items: Vec<BatchItem> = batch::parse_batch_file(batch_path)?;
    if items.is_empty() {
        bail!(
            "batch file {} produced no items (all lines blank/comments?)",
            batch_path.display()
        );
    }

    let fmt = resolve_format(cli.svg, None);
    let ext = default_extension(fmt);

    let mut written = 0usize;
    for item in &items {
        let path = unique_path(&outdir, &item.stem, ext);
        write_one(&item.payload, fmt, &path, opts)
            .with_context(|| format!("failed on batch item '{}'", item.payload))?;
        written += 1;
    }

    println!(
        "Wrote {written} QR code{} to {}",
        if written == 1 { "" } else { "s" },
        outdir.display()
    );
    Ok(())
}

/// Render `payload` in `fmt` and write it to `path`.
fn write_one(payload: &str, fmt: Format, path: &Path, opts: &RenderOpts) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create directory {}", parent.display()))?;
        }
    }
    match fmt {
        Format::Png => {
            let bytes = render_png(payload, opts)?;
            std::fs::write(path, &bytes)
                .with_context(|| format!("cannot write {}", path.display()))?;
        }
        Format::Svg => {
            let svg = render_svg(payload, opts)?;
            std::fs::write(path, svg.as_bytes())
                .with_context(|| format!("cannot write {}", path.display()))?;
        }
    }
    Ok(())
}

/// Build a non-colliding path `<dir>/<stem>.<ext>`, appending `-2`, `-3`, ...
/// if needed so two identical stems in a batch don't overwrite each other.
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let candidate = dir.join(format!("{stem}-{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Pathological fallback; effectively unreachable.
    first
}

/// Parse `key=value` strings into pairs, erroring on a missing `=`.
fn parse_kv_pairs(raw: &[String]) -> Result<Vec<(String, String)>> {
    let mut pairs = Vec::with_capacity(raw.len());
    for item in raw {
        match item.split_once('=') {
            Some((k, v)) => pairs.push((k.trim().to_string(), v.to_string())),
            None => {
                bail!("vCard argument '{item}' is not in key=value form (e.g. name=\"Jane Doe\")")
            }
        }
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_inference_from_extension() {
        assert_eq!(resolve_format(false, Some(Path::new("a.svg"))), Format::Svg);
        assert_eq!(resolve_format(false, Some(Path::new("a.SVG"))), Format::Svg);
        assert_eq!(resolve_format(false, Some(Path::new("a.png"))), Format::Png);
        assert_eq!(resolve_format(false, None), Format::Png);
        // --svg flag overrides a .png extension.
        assert_eq!(resolve_format(true, Some(Path::new("a.png"))), Format::Svg);
    }

    #[test]
    fn kv_parsing() {
        let p = parse_kv_pairs(&["name=Jane Doe".into(), "email=a@b.com".into()]).unwrap();
        assert_eq!(p[0], ("name".into(), "Jane Doe".into()));
        assert_eq!(p[1], ("email".into(), "a@b.com".into()));
        assert!(parse_kv_pairs(&["noequals".into()]).is_err());
    }

    #[test]
    fn value_with_equals_sign_preserved() {
        // split_once keeps everything after the first '='.
        let p = parse_kv_pairs(&["url=https://x.com/?a=b".into()]).unwrap();
        assert_eq!(p[0].1, "https://x.com/?a=b");
    }
}
