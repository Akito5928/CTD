//! Streaming text normalization and case folding.

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

/// Incrementally converts raw file bytes into the text stored in the index:
/// UTF-8 BOM removed, CRLF / CR -> LF, case folded. Memory use does not
/// depend on the file size.
#[derive(Default)]
pub struct Normalizer {
    pending: Vec<u8>,
    prev_cr: bool,
    started: bool,
    consumed: u64,
}

impl Normalizer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes; the normalized text is appended to `out`.
    /// Errors when the data is not valid UTF-8.
    pub fn feed(&mut self, data: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(data);
        let mut start = 0;
        if !self.started {
            self.started = true;
            if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
                start = 3;
                self.consumed += 3;
            }
        }
        let body = &buf[start..];
        let valid_len = match std::str::from_utf8(body) {
            Ok(_) => body.len(),
            Err(e) => {
                if e.error_len().is_some() {
                    return Err(format!(
                        "not valid UTF-8 (invalid byte at offset {})",
                        self.consumed + e.valid_up_to() as u64
                    ));
                }
                e.valid_up_to()
            }
        };
        let text = std::str::from_utf8(&body[..valid_len]).expect("validated");
        for c in text.chars() {
            if self.prev_cr {
                self.prev_cr = false;
                if c == '\n' {
                    continue;
                }
            }
            if c == '\r' {
                out.push(b'\n');
                self.prev_cr = true;
                continue;
            }
            let f = fold_char(c);
            let mut tmp = [0u8; 4];
            out.extend_from_slice(f.encode_utf8(&mut tmp).as_bytes());
        }
        self.consumed += valid_len as u64;
        self.pending = body[valid_len..].to_vec();
        Ok(())
    }

    /// Call after the last chunk: an unfinished UTF-8 sequence is an error.
    pub fn finish(&self) -> Result<(), String> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err("not valid UTF-8 (truncated sequence at end of file)".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(chunks: &[&[u8]]) -> Result<String, String> {
        let mut n = Normalizer::new();
        let mut out = Vec::new();
        for c in chunks {
            n.feed(c, &mut out)?;
        }
        n.finish()?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn utf8_bom_and_folding() {
        assert_eq!(norm(&["Héllo ÄÖ".as_bytes()]).unwrap(), "héllo äö");
        assert_eq!(
            norm(&[b"\xEF\xBB\xBF", "日本語ABC".as_bytes()]).unwrap(),
            "日本語abc"
        );
        assert_eq!(norm(&["ＡＢＣ".as_bytes()]).unwrap(), "ａｂｃ");
    }

    #[test]
    fn newlines_across_chunks() {
        assert_eq!(norm(&[b"a\r\nb\rc\nd"]).unwrap(), "a\nb\nc\nd");
        assert_eq!(norm(&[b"a\r", b"\nb"]).unwrap(), "a\nb");
        assert_eq!(norm(&[b"a\r", b"b"]).unwrap(), "a\nb");
        assert_eq!(norm(&[b"x\r"]).unwrap(), "x\n");
    }

    #[test]
    fn multibyte_split_across_chunks() {
        let s = "日本語".as_bytes();
        assert_eq!(norm(&[&s[..2], &s[2..5], &s[5..]]).unwrap(), "日本語");
    }

    #[test]
    fn invalid_and_truncated() {
        assert!(norm(&[&[0x82, 0xA0, 0x82, 0xA2]]).is_err()); // Shift_JIS
        assert!(norm(&[&"日".as_bytes()[..2]]).is_err()); // truncated
    }
}
