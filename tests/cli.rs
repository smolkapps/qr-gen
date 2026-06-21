//! End-to-end CLI tests: real process invocation via assert_cmd, output written
//! into a tempdir, plus the decode round-trip through the actual binary path
//! and the documented error/exit-code behavior.

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("qr-gen").expect("binary builds")
}

fn decode_png_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read png");
    let img = image::load_from_memory(&bytes)
        .expect("load png")
        .to_luma8();
    let mut prep = rqrr::PreparedImage::prepare(img);
    let grids = prep.detect_grids();
    assert!(!grids.is_empty(), "no QR grid in {}", path.display());
    grids[0].decode().expect("decode").1
}

#[test]
fn single_png_via_cli_decodes_back() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("out.png");
    let payload = "https://cli.example.com/abc";

    bin()
        .arg(payload)
        .arg("-o")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote"));

    assert!(out.exists(), "PNG should exist");
    let meta = fs::metadata(&out).unwrap();
    assert!(meta.len() > 0, "PNG should be non-empty");
    assert_eq!(decode_png_file(&out), payload);
}

#[test]
fn svg_inferred_from_extension() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("out.svg");

    bin()
        .arg("hello-svg")
        .arg("-o")
        .arg(&out)
        .assert()
        .success();

    let content = fs::read_to_string(&out).unwrap();
    assert!(content.contains("<svg"));
    assert!(content.contains("</svg>"));
    assert!(content.len() > 50);
}

#[test]
fn svg_flag_forces_svg_even_with_png_name() {
    let dir = tempdir().unwrap();
    // .png extension but --svg flag -> content must be SVG.
    let out = dir.path().join("forced.png");

    bin()
        .arg("force-svg")
        .arg("--svg")
        .arg("-o")
        .arg(&out)
        .assert()
        .success();

    let content = fs::read_to_string(&out).unwrap();
    assert!(content.starts_with("<?xml") || content.contains("<svg"));
}

#[test]
fn vcard_mode_writes_and_decodes() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("jane.png");

    bin()
        .args([
            "--vcard",
            "name=Jane Doe",
            "email=jane@x.com",
            "phone=+15551234",
        ])
        .arg("-o")
        .arg(&out)
        .assert()
        .success();

    assert!(out.exists());
    let decoded = decode_png_file(&out);
    assert!(decoded.starts_with("BEGIN:VCARD"));
    assert!(decoded.contains("FN:Jane Doe"));
    assert!(decoded.contains("jane@x.com"));
}

#[test]
fn batch_txt_produces_one_file_per_line() {
    let dir = tempdir().unwrap();
    let lines = dir.path().join("lines.txt");
    fs::write(
        &lines,
        "https://a.example\n\n# comment, skipped\nhttps://b.example\nhttps://c.example\n",
    )
    .unwrap();
    let outdir = dir.path().join("qrs");

    bin()
        .arg("--batch")
        .arg(&lines)
        .arg("--outdir")
        .arg(&outdir)
        .assert()
        .success()
        .stdout(predicate::str::contains("3 QR codes"));

    let count = fs::read_dir(&outdir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .map(|x| x == "png")
                .unwrap_or(false)
        })
        .count();
    assert_eq!(count, 3, "expected 3 PNG files");
}

#[test]
fn batch_csv_with_filename_column() {
    let dir = tempdir().unwrap();
    let csv = dir.path().join("data.csv");
    fs::write(
        &csv,
        "text,filename\nhttps://one.example,alpha\nhttps://two.example,beta\n",
    )
    .unwrap();
    let outdir = dir.path().join("out");

    bin()
        .arg("--batch")
        .arg(&csv)
        .arg("--outdir")
        .arg(&outdir)
        .assert()
        .success()
        .stdout(predicate::str::contains("2 QR codes"));

    assert!(outdir.join("alpha.png").exists());
    assert!(outdir.join("beta.png").exists());
    // And the named file decodes to the right payload.
    assert_eq!(
        decode_png_file(&outdir.join("alpha.png")),
        "https://one.example"
    );
}

#[test]
fn batch_svg_flag_emits_svg_files() {
    let dir = tempdir().unwrap();
    let lines = dir.path().join("lines.txt");
    fs::write(&lines, "https://a.example\nhttps://b.example\n").unwrap();
    let outdir = dir.path().join("svgs");

    bin()
        .arg("--batch")
        .arg(&lines)
        .arg("--outdir")
        .arg(&outdir)
        .arg("--svg")
        .assert()
        .success();

    let svgs: Vec<_> = fs::read_dir(&outdir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "svg").unwrap_or(false))
        .collect();
    assert_eq!(svgs.len(), 2);
    let body = fs::read_to_string(&svgs[0]).unwrap();
    assert!(body.contains("<svg"));
}

// ---- Error / exit-code paths ----

#[test]
fn invalid_hex_color_exits_nonzero_with_clear_stderr() {
    let dir = tempdir().unwrap();
    let out = dir.path().join("x.png");

    bin()
        .arg("hello")
        .arg("--fg")
        .arg("#zzzzzz")
        .arg("-o")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid").and(predicate::str::contains("color")));

    assert!(!out.exists(), "no file should be written on color error");
}

#[test]
fn unreadable_batch_file_exits_nonzero() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("does-not-exist.txt");

    bin()
        .arg("--batch")
        .arg(&missing)
        .arg("--outdir")
        .arg(dir.path().join("out"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot read batch file"));
}

#[test]
fn no_input_exits_nonzero() {
    bin()
        .assert()
        .failure()
        .stderr(predicate::str::contains("no input given"));
}

#[test]
fn bad_ecc_value_rejected_by_clap() {
    bin().arg("hello").arg("--ecc").arg("z").assert().failure();
}

#[test]
fn zero_size_rejected() {
    let dir = tempdir().unwrap();
    bin()
        .arg("hello")
        .arg("--size")
        .arg("0")
        .arg("-o")
        .arg(dir.path().join("z.png"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("--size"));
}
