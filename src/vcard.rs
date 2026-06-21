//! vCard 3.0 string construction from `key=value` pairs.

use anyhow::{bail, Result};

/// Build a vCard 3.0 string from a list of `key=value` arguments.
///
/// Recognized keys (case-insensitive): `name`, `email`, `phone`, `org`,
/// `title`, `url`, `address`, `note`. `name` is required.
///
/// The output is a CRLF-delimited vCard, which is what QR scanners expect
/// (the vCard spec mandates CRLF line endings).
pub fn build_vcard(pairs: &[(String, String)]) -> Result<String> {
    let mut name: Option<String> = None;
    let mut email: Option<String> = None;
    let mut phone: Option<String> = None;
    let mut org: Option<String> = None;
    let mut title: Option<String> = None;
    let mut url: Option<String> = None;
    let mut address: Option<String> = None;
    let mut note: Option<String> = None;

    for (k, v) in pairs {
        let value = v.trim().to_string();
        match k.to_ascii_lowercase().as_str() {
            "name" | "n" | "fn" => name = Some(value),
            "email" | "e" | "mail" => email = Some(value),
            "phone" | "tel" | "p" => phone = Some(value),
            "org" | "organization" | "company" => org = Some(value),
            "title" | "role" => title = Some(value),
            "url" | "website" | "web" => url = Some(value),
            "address" | "adr" => address = Some(value),
            "note" => note = Some(value),
            other => bail!(
                "unknown vCard field '{other}'. Supported: name, email, phone, org, title, url, address, note"
            ),
        }
    }

    let name = match name {
        Some(n) if !n.is_empty() => n,
        _ => bail!("vCard requires a non-empty name= field, e.g. --vcard name=\"Jane Doe\""),
    };

    // CRLF per RFC 6350/2426. Build with explicit \r\n.
    let mut out = String::new();
    out.push_str("BEGIN:VCARD\r\n");
    out.push_str("VERSION:3.0\r\n");

    // N: Family;Given;Additional;Prefix;Suffix  — best-effort split on first space.
    let (given, family) = split_name(&name);
    out.push_str(&format!("N:{};{};;;\r\n", esc(&family), esc(&given)));
    out.push_str(&format!("FN:{}\r\n", esc(&name)));

    if let Some(o) = org.filter(|s| !s.is_empty()) {
        out.push_str(&format!("ORG:{}\r\n", esc(&o)));
    }
    if let Some(t) = title.filter(|s| !s.is_empty()) {
        out.push_str(&format!("TITLE:{}\r\n", esc(&t)));
    }
    if let Some(p) = phone.filter(|s| !s.is_empty()) {
        out.push_str(&format!("TEL;TYPE=CELL:{}\r\n", esc(&p)));
    }
    if let Some(e) = email.filter(|s| !s.is_empty()) {
        out.push_str(&format!("EMAIL;TYPE=INTERNET:{}\r\n", esc(&e)));
    }
    if let Some(u) = url.filter(|s| !s.is_empty()) {
        out.push_str(&format!("URL:{}\r\n", esc(&u)));
    }
    if let Some(a) = address.filter(|s| !s.is_empty()) {
        // ADR: PO;Ext;Street;City;Region;Postal;Country — dump into Street slot.
        out.push_str(&format!("ADR;TYPE=HOME:;;{};;;;\r\n", esc(&a)));
    }
    if let Some(n) = note.filter(|s| !s.is_empty()) {
        out.push_str(&format!("NOTE:{}\r\n", esc(&n)));
    }

    out.push_str("END:VCARD\r\n");
    Ok(out)
}

/// Split a full name into (given, family). If there's no space, the whole
/// thing is the given name and family is empty.
fn split_name(full: &str) -> (String, String) {
    let trimmed = full.trim();
    match trimmed.rsplit_once(' ') {
        Some((given, family)) => (given.trim().to_string(), family.trim().to_string()),
        None => (trimmed.to_string(), String::new()),
    }
}

/// Escape vCard special characters per RFC 6350 §3.4:
/// backslash, comma, semicolon, and newline.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '\n' => out.push_str("\\n"),
            '\r' => {} // drop bare CR; CRLF line breaks are added structurally
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(kvs: &[(&str, &str)]) -> Vec<(String, String)> {
        kvs.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn builds_minimal_vcard() {
        let vc = build_vcard(&pairs(&[("name", "Jane Doe")])).unwrap();
        assert!(vc.starts_with("BEGIN:VCARD\r\n"));
        assert!(vc.contains("VERSION:3.0\r\n"));
        assert!(vc.contains("FN:Jane Doe\r\n"));
        assert!(vc.contains("N:Doe;Jane;;;\r\n"));
        assert!(vc.ends_with("END:VCARD\r\n"));
    }

    #[test]
    fn builds_full_vcard() {
        let vc = build_vcard(&pairs(&[
            ("name", "Jane Doe"),
            ("email", "jane@x.com"),
            ("phone", "+15551234"),
            ("org", "Acme"),
        ]))
        .unwrap();
        assert!(vc.contains("EMAIL;TYPE=INTERNET:jane@x.com\r\n"));
        assert!(vc.contains("TEL;TYPE=CELL:+15551234\r\n"));
        assert!(vc.contains("ORG:Acme\r\n"));
    }

    #[test]
    fn requires_name() {
        assert!(build_vcard(&pairs(&[("email", "a@b.com")])).is_err());
        assert!(build_vcard(&pairs(&[("name", "")])).is_err());
    }

    #[test]
    fn rejects_unknown_field() {
        assert!(build_vcard(&pairs(&[("name", "X"), ("bogus", "y")])).is_err());
    }

    #[test]
    fn escapes_special_chars() {
        let vc = build_vcard(&pairs(&[("name", "Doe, John; Jr")])).unwrap();
        assert!(vc.contains("FN:Doe\\, John\\; Jr\r\n"));
    }

    #[test]
    fn single_word_name() {
        let vc = build_vcard(&pairs(&[("name", "Cher")])).unwrap();
        assert!(vc.contains("N:;Cher;;;\r\n"));
        assert!(vc.contains("FN:Cher\r\n"));
    }
}
