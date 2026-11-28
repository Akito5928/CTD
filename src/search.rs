//! Multi-term AND search over the folded text segments.

use crate::error::CtdError;
use crate::interrupt;
use crate::store::{Index, State};
use crate::textio::fold;
use anyhow::{Context, Result};
use memchr::memmem;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Characters shown on each side of a match.
pub const SNIPPET_RADIUS: usize = 40;
/// Maximum number of files shown.
pub const MAX_FILES: usize = 20;
const READ_CHUNK: usize = 1 << 20;
/// Very long lines are processed in pieces of this size.
const LINE_CAP: usize = 256 * 1024;

/// Split the command line into search terms (Unicode whitespace, including
/// the full-width space). Empty input is a usage error.
pub fn parse_terms(args: &[String]) -> std::result::Result<Vec<String>, CtdError> {
    let mut terms: Vec<String> = Vec::new();
    for a in args {
        for t in a.split_whitespace() {
            if !terms.iter().any(|x| x == t) {
                terms.push(t.to_string());
            }
        }
    }
    if terms.is_empty() {
        return Err(CtdError::Usage(
            "the search text is empty. Usage: ctd <search terms>".to_string(),
        ));
    }
    Ok(terms)
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub rel: String,
    pub abs: PathBuf,
}

#[derive(Debug, Default)]
pub struct Found {
    /// All matching files.
    pub total: usize,
    /// The first `limit` matches in relative-path order.
    pub hits: Vec<Hit>,
}

/// Find every file containing all terms (each term in the path or the text).
pub fn find(
    root: &Path,
    cache_dir: &Path,
    idx: &Index,
    terms: &[String],
    max_depth: Option<u16>,
    limit: usize,
) -> Result<Found> {
    let folded: Vec<String> = terms.iter().map(|t| fold(t)).collect();
    let finders: Vec<memmem::Finder<'static>> = folded
        .iter()
        .map(|t| memmem::Finder::new(t.as_bytes()).into_owned())
        .collect();
    let carry = folded
        .iter()
        .map(|t| t.len())
        .max()
        .unwrap_or(1)
        .saturating_sub(1);
    let mut files: HashMap<u32, File> = HashMap::new();
    let mut buf: Vec<u8> = Vec::new();
    let mut found = Found::default();

    for (n, e) in idx.entries.iter().enumerate() {
        if e.state != State::Text || max_depth.map_or(false, |m| e.depth > m) {
            continue;
        }
        if n % 1024 == 0 {
            interrupt::check()?;
        }
        let path_folded = fold(&e.rel);
        let mut hit: Vec<bool> = finders
            .iter()
            .map(|f| f.find(path_folded.as_bytes()).is_some())
            .collect();
        if !hit.iter().all(|&b| b) && e.len > 0 {
            let f = match files.entry(e.seg) {
                std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                std::collections::hash_map::Entry::Vacant(v) => {
                    let name = &idx.segments[e.seg as usize].name;
                    let file = File::open(cache_dir.join(name)).with_context(|| {
                        format!("cannot open the index data {name} (it may have been changed by another ctd process; run the command again)")
                    })?;
                    v.insert(file)
                }
            };
            scan_range(f, e.off, e.len, &finders, carry, &mut hit, &mut buf)?;
        }
        if hit.iter().all(|&b| b) {
            found.total += 1;
            if found.hits.len() < limit {
                found.hits.push(Hit {
                    rel: e.rel.clone(),
                    abs: root.join(&e.rel),
                });
            }
        }
    }
    Ok(found)
}

/// Search a byte range in chunks; memory use is bounded by the chunk size.
fn scan_range(
    f: &mut File,
    off: u64,
    len: u64,
    finders: &[memmem::Finder<'static>],
    carry: usize,
    hit: &mut [bool],
    buf: &mut Vec<u8>,
) -> Result<()> {
    f.seek(SeekFrom::Start(off))?;
    buf.clear();
    let mut left = len;
    while left > 0 {
        let want = left.min(READ_CHUNK as u64) as usize;
        let old = buf.len();
        buf.resize(old + want, 0);
        f.read_exact(&mut buf[old..])
            .context("the index data is shorter than expected")?;
        left -= want as u64;
        for (i, fi) in finders.iter().enumerate() {
            if !hit[i] && fi.find(buf).is_some() {
                hit[i] = true;
            }
        }
        if hit.iter().all(|&b| b) {
            break;
        }
        if buf.len() > carry {
            let cut = buf.len() - carry;
            buf.drain(..cut);
        }
    }
    Ok(())
}

/// What to show for one result file.
#[derive(Debug, Default, Clone)]
pub struct Display {
    /// Representative line: (line number, snippet).
    pub line: Option<(usize, String)>,
    /// Further matching lines.
    pub more_lines: usize,
    /// The original file could not be read.
    pub unavailable: bool,
}

/// Re-read the original file (streaming, line by line) and pick the line that
/// contains the most distinct terms (earliest on ties).
pub fn describe(abs: &Path, terms: &[String]) -> Display {
    let folded: Vec<String> = terms.iter().map(|t| fold(t)).collect();
    let finders: Vec<memmem::Finder<'static>> = folded
        .iter()
        .map(|t| memmem::Finder::new(t.as_bytes()).into_owned())
        .collect();
    let qlens: Vec<usize> = folded.iter().map(|t| t.chars().count()).collect();
    let Ok(mut file) = File::open(abs) else {
        return Display {
            unavailable: true,
            ..Default::default()
        };
    };
    // (distinct terms, line number, original text, column, matched term length)
    let mut best: Option<(usize, usize, String, usize, usize)> = None;
    let mut matching = 0usize;
    let mut buf: Vec<u8> = Vec::new();
    let res = for_each_line(&mut file, |line_no, bytes| {
        fold_into(bytes, &mut buf);
        let mut count = 0usize;
        let mut first_pos = usize::MAX;
        let mut first_len = 0usize;
        for (i, f) in finders.iter().enumerate() {
            if let Some(p) = f.find(&buf) {
                count += 1;
                if p < first_pos {
                    first_pos = p;
                    first_len = qlens[i];
                }
            }
        }
        if count == 0 {
            return;
        }
        matching += 1;
        if best.as_ref().map_or(true, |b| count > b.0) {
            let col = String::from_utf8_lossy(&buf[..first_pos]).chars().count();
            best = Some((
                count,
                line_no,
                String::from_utf8_lossy(bytes).into_owned(),
                col,
                first_len,
            ));
        }
    });
    if res.is_err() {
        return Display {
            unavailable: true,
            ..Default::default()
        };
    }
    match best {
        Some((_, line_no, text, col, qlen)) => Display {
            line: Some((line_no, make_snippet(&text, col, qlen))),
            more_lines: matching - 1,
            unavailable: false,
        },
        None => Display::default(),
    }
}

/// Case-fold `bytes` into `out` (reused buffer; ASCII lines take a fast path).
fn fold_into(bytes: &[u8], out: &mut Vec<u8>) {
    out.clear();
    if bytes.is_ascii() {
        out.extend_from_slice(bytes);
        out.make_ascii_lowercase();
    } else {
        let mut tmp = [0u8; 4];
        for c in String::from_utf8_lossy(bytes).chars() {
            out.extend_from_slice(crate::textio::fold_char(c).encode_utf8(&mut tmp).as_bytes());
        }
    }
}

/// Call `f(line_number, bytes)` for every line. Handles LF, CRLF and CR,
/// strips a UTF-8 BOM and splits lines longer than `LINE_CAP`.
pub fn for_each_line<R: Read>(r: &mut R, mut f: impl FnMut(usize, &[u8])) -> std::io::Result<()> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut cur: Vec<u8> = Vec::new();
    let mut line_no = 1usize;
    let mut pending_cr = false;
    let mut first = true;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let mut chunk = &buf[..n];
        if first {
            first = false;
            if chunk.starts_with(&[0xEF, 0xBB, 0xBF]) {
                chunk = &chunk[3..];
            }
        }
        let mut i = 0;
        while i < chunk.len() {
            if pending_cr {
                pending_cr = false;
                if chunk[i] == b'\n' {
                    i += 1;
                    continue;
                }
            }
            match memchr::memchr2(b'\n', b'\r', &chunk[i..]) {
                Some(p) => {
                    cur.extend_from_slice(&chunk[i..i + p]);
                    f(line_no, &cur);
                    cur.clear();
                    line_no += 1;
                    if chunk[i + p] == b'\r' {
                        pending_cr = true;
                    }
                    i += p + 1;
                }
                None => {
                    cur.extend_from_slice(&chunk[i..]);
                    i = chunk.len();
                    if cur.len() >= LINE_CAP {
                        f(line_no, &cur);
                        cur.clear();
                    }
                }
            }
        }
    }
    if !cur.is_empty() {
        f(line_no, &cur);
    }
    Ok(())
}

fn clean(c: char) -> char {
    if c.is_control() {
        ' '
    } else {
        c
    }
}

/// Up to `SNIPPET_RADIUS` Unicode characters on each side of the match,
/// with `...` marking the omitted side(s).
pub fn make_snippet(line: &str, col: usize, qlen: usize) -> String {
    let chars: Vec<char> = line.chars().map(clean).collect();
    let n = chars.len();
    let start = col.min(n).saturating_sub(SNIPPET_RADIUS);
    let end = (col + qlen + SNIPPET_RADIUS).min(n);
    let mut s: String = chars[start..end].iter().collect();
    s = s.trim().to_string();
    if start > 0 {
        s = format!("...{s}");
    }
    if end < n {
        s.push_str("...");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_are_split_on_unicode_whitespace() {
        let t = parse_terms(&[
            "programming language".into(),
            "Rust\u{3000}CLI".into(),
            "rust".into(),
        ])
        .unwrap();
        assert_eq!(t, vec!["programming", "language", "Rust", "CLI", "rust"]);
        assert!(parse_terms(&["".into()]).is_err());
        assert!(parse_terms(&["  \u{3000} ".into()]).is_err());
        assert!(parse_terms(&[]).is_err());
    }

    #[test]
    fn snippet_unicode_and_ellipsis() {
        let line = format!("{}検索{}", "あ".repeat(50), "い".repeat(50));
        let s = make_snippet(&line, 50, 2);
        assert!(s.starts_with("...") && s.ends_with("..."));
        assert_eq!(s.chars().count(), 3 + 82 + 3);
        assert_eq!(make_snippet("short SQLite line", 6, 6), "short SQLite line");
    }

    #[test]
    fn lines_split_like_the_index() {
        let mut got = Vec::new();
        let data = b"\xEF\xBB\xBFone\r\ntwo\rthree\nfour";
        for_each_line(&mut &data[..], |n, b| {
            got.push((n, String::from_utf8_lossy(b).to_string()))
        })
        .unwrap();
        assert_eq!(
            got,
            vec![
                (1, "one".into()),
                (2, "two".into()),
                (3, "three".into()),
                (4, "four".into())
            ]
        );
    }

    #[test]
    fn describe_prefers_line_with_most_terms() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.txt");
        std::fs::write(
            &p,
            "alpha only\nalpha and BETA here\nbeta later\nalpha beta again\n",
        )
        .unwrap();
        let r = describe(&p, &["alpha".into(), "beta".into()]);
        let (n, snip) = r.line.unwrap();
        assert_eq!(n, 2); // first line with both terms
        assert!(snip.contains("BETA"));
        assert_eq!(r.more_lines, 3);
        let r = describe(&p, &["zzz".into()]);
        assert!(r.line.is_none() && !r.unavailable);
        assert!(describe(&d.path().join("missing.txt"), &["a".into()]).unavailable);
    }
}
