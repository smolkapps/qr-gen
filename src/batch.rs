//! Batch input parsing: plain-text (one payload per line) and CSV.

use std::path::Path;

use anyhow::{bail, Context, Result};

/// One item to encode in a batch: the payload plus a base filename (no
/// extension) to write it under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchItem {
    pub payload: String,
    /// Base filename WITHOUT extension. Always non-empty and filesystem-safe.
    pub stem: String,
}

/// Decide whether a batch path is CSV by extension.
pub fn is_csv(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
        Some(ref e) if e == "csv"
    )
}

/// Parse a batch input file (auto-detecting CSV vs. plain text).
pub fn parse_batch_file(path: &Path) -> Result<Vec<BatchItem>> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read batch file {}", path.display()))?;
    if is_csv(path) {
        parse_csv(&data)
    } else {
        Ok(parse_lines(&data))
    }
}

/// Parse plain text: one payload per non-empty line. Leading/trailing
/// whitespace is trimmed; blank lines and `#` comment lines are skipped.
pub fn parse_lines(data: &str) -> Vec<BatchItem> {
    let mut items = Vec::new();
    let mut idx = 0usize;
    for line in data.lines() {
        let payload = line.trim();
        if payload.is_empty() || payload.starts_with('#') {
            continue;
        }
        idx += 1;
        items.push(BatchItem {
            payload: payload.to_string(),
            stem: indexed_slug(payload, idx),
        });
    }
    items
}

/// Parse CSV. Looks for a `text` column (case-insensitive) for the payload and
/// an optional `filename` column for the output stem. If there's no header
/// resembling those names, the first column is treated as the payload.
pub fn parse_csv(data: &str) -> Result<Vec<BatchItem>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(data.as_bytes());

    let headers = rdr
        .headers()
        .context("failed to read CSV header row")?
        .clone();

    let lower: Vec<String> = headers
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let text_col = lower
        .iter()
        .position(|h| h == "text" || h == "payload" || h == "data" || h == "url");
    let file_col = lower
        .iter()
        .position(|h| h == "filename" || h == "file" || h == "name");

    // If no recognizable header at all, the "header" was really data: treat
    // column 0 as payload and replay the header row as the first record.
    let header_is_data = text_col.is_none() && file_col.is_none();
    let payload_idx = text_col.unwrap_or(0);

    let mut items = Vec::new();
    let mut idx = 0usize;

    let push = |record: &csv::StringRecord, idx: &mut usize, items: &mut Vec<BatchItem>| {
        let payload = record.get(payload_idx).unwrap_or("").trim();
        if payload.is_empty() {
            return;
        }
        *idx += 1;
        let stem = match file_col.and_then(|c| record.get(c)).map(str::trim) {
            Some(name) if !name.is_empty() => {
                let s = sanitize_stem(name);
                if s.is_empty() {
                    indexed_slug(payload, *idx)
                } else {
                    s
                }
            }
            _ => indexed_slug(payload, *idx),
        };
        items.push(BatchItem {
            payload: payload.to_string(),
            stem,
        });
    };

    if header_is_data {
        // Replay the header line as the first data record.
        push(&headers, &mut idx, &mut items);
    }

    for rec in rdr.records() {
        let rec = rec.context("malformed CSV row")?;
        push(&rec, &mut idx, &mut items);
    }

    if items.is_empty() {
        bail!("CSV produced no usable rows (need a non-empty payload column)");
    }
    Ok(items)
}

/// Make a filesystem-safe, indexed stem from a payload: a zero-padded index
/// prefix plus a slug of the payload, so files sort in input order and never
/// collide.
pub fn indexed_slug(payload: &str, idx: usize) -> String {
    let slug = slugify(payload);
    if slug.is_empty() {
        format!("qr-{idx:04}")
    } else {
        format!("{idx:04}-{slug}")
    }
}

/// Lowercase, replace runs of non-alphanumeric with a single dash, trim
/// dashes, and cap length so we don't blow past filesystem limits.
pub fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    let mut slug: String = trimmed.chars().take(40).collect();
    slug = slug.trim_matches('-').to_string();
    slug
}

/// Sanitize a user-supplied filename stem: strip any directory components and
/// extension, then slugify-ish (keep dots/dashes/underscores).
fn sanitize_stem(name: &str) -> String {
    // Take the file name only (no path traversal).
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    // Drop a trailing extension if present.
    let base = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(base);

    let mut out = String::with_capacity(base.len());
    let mut prev_dash = false;
    for c in base.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').chars().take(60).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_skipping_blanks_and_comments() {
        let data = "https://a.com\n\n  # a comment\nhttps://b.com\n   \nplain text\n";
        let items = parse_lines(data);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].payload, "https://a.com");
        assert_eq!(items[1].payload, "https://b.com");
        assert_eq!(items[2].payload, "plain text");
        assert!(items[0].stem.starts_with("0001-"));
        assert!(items[2].stem.starts_with("0003-"));
    }

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("https://example.com"), "https-example-com");
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("a"), "a");
    }

    #[test]
    fn csv_with_text_and_filename_columns() {
        let data = "text,filename\nhttps://a.com,alpha\nhttps://b.com,beta\n";
        let items = parse_csv(data).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].payload, "https://a.com");
        assert_eq!(items[0].stem, "alpha");
        assert_eq!(items[1].stem, "beta");
    }

    #[test]
    fn csv_text_only_column() {
        let data = "text\nhttps://a.com\nhttps://b.com\n";
        let items = parse_csv(data).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].stem.starts_with("0001-"));
    }

    #[test]
    fn csv_no_header_uses_first_column() {
        // No recognizable header -> first row is data, col 0 is payload.
        let data = "https://a.com\nhttps://b.com\n";
        let items = parse_csv(data).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].payload, "https://a.com");
    }

    #[test]
    fn csv_filename_sanitized_no_traversal() {
        let data = "text,filename\nhi,../../etc/passwd\n";
        let items = parse_csv(data).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].stem, "passwd");
    }

    #[test]
    fn empty_csv_errors() {
        assert!(parse_csv("text\n\n").is_err());
    }
}
