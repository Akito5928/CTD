//! Text decoding, newline normalization and case folding.

/// Decode file bytes as UTF-8 (BOM removed, newlines normalized to LF).
/// Invalid UTF-8 is reported, never reinterpreted as another encoding.
pub fn decode(mut bytes: Vec<u8>) -> Result<String, String> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err("UTF-16 is not supported (save the file as UTF-8)".to_string());
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    match String::from_utf8(bytes) {
        Ok(s) => Ok(normalize_newlines(s)),
        Err(e) => Err(format!(
            "not valid UTF-8 (first invalid byte at offset {})",
            e.utf8_error().valid_up_to()
        )),
    }
}

/// CRLF -> LF, CR -> LF.
pub fn normalize_newlines(s: String) -> String {
    if !s.contains('\r') {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

/// Per-character case fold (one char in, one char out), so that character
/// offsets in the folded text equal those in the original text.
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

pub fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

pub fn hash_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_and_bom() {
        assert_eq!(decode("héllo".as_bytes().to_vec()).unwrap(), "héllo");
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice("日本語".as_bytes());
        assert_eq!(decode(b).unwrap(), "日本語");
    }

    #[test]
    fn newlines() {
        assert_eq!(decode(b"a\r\nb\rc\nd".to_vec()).unwrap(), "a\nb\nc\nd");
        assert_eq!(decode(b"a\r\n\r\nb".to_vec()).unwrap(), "a\n\nb");
        assert_eq!(decode(b"x\r".to_vec()).unwrap(), "x\n");
    }

    #[test]
    fn invalid_utf8_is_reported() {
        assert!(decode(vec![0x82, 0xA0, 0x82, 0xA2]).is_err()); // Shift_JIS
        assert!(decode(vec![0xFF, 0xFE, 0x41, 0x00]).unwrap_err().contains("UTF-16"));
    }

    #[test]
    fn folding() {
        assert_eq!(fold("SQLite ÄÖ"), "sqlite äö");
        assert_eq!(fold("日本語ABC"), "日本語abc");
        assert_eq!(fold("İ").chars().count(), 1);
    }
}
