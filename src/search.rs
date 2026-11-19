//! Substring search over the FTS5 trigram index, with verification.

use crate::interrupt;
use crate::textio::fold;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// Characters shown on each side of a match.
pub const SNIPPET_RADIUS: usize = 40;
/// Maximum matching lines shown per file.
pub const MAX_LINES_PER_FILE: usize = 3;
const CONTEXT_LINE_MAX: usize = 80;

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: usize,
    pub context: usize,
}

#[derive(Debug, Clone)]
pub struct LineHit {
    pub line_no: usize,
    pub snippet: String,
    pub before: Vec<(usize, String)>,
    pub after: Vec<(usize, String)>,
}

#[derive(Debug, Clone)]
pub struct FileHit {
    pub path: String,
    /// Empty when the file matched only through its path.
    pub lines: Vec<LineHit>,
}

#[derive(Debug, Default)]
pub struct SearchOutput {
    pub hits: Vec<FileHit>,
    /// More matching files exist beyond the limit.
    pub more: bool,
}

/// Quote a query as an FTS5 phrase (all special characters become literal).
pub fn fts_phrase(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

pub fn search(conn: &Connection, query: &str, opts: &SearchOptions) -> Result<SearchOutput> {
    let fq = fold(query);
    let qlen = query.chars().count();
    let mut out = SearchOutput::default();
    let mut content_stmt = conn.prepare("SELECT content FROM files WHERE id = ?1")?;

    let mut handle = |id: i64, path: String, out: &mut SearchOutput| -> Result<bool> {
        interrupt::check()?;
        let content: String = content_stmt.query_row(params![id], |r| r.get(0))?;
        if let Some(hit) = verify_file(&path, &content, &fq, opts.context) {
            if out.hits.len() >= opts.limit {
                out.more = true;
                return Ok(true);
            }
            out.hits.push(hit);
        }
        Ok(false)
    };

    if qlen >= 3 {
        let mut stmt = conn.prepare(
            "SELECT f.id, f.relative_path
             FROM files_fts JOIN files f ON f.id = files_fts.rowid
             WHERE files_fts MATCH ?1
             ORDER BY files_fts.rank, f.relative_path",
        )?;
        let mut rows = stmt
            .query(params![fts_phrase(query)])
            .context("search failed (if this persists, run `ctd rebuild`)")?;
        while let Some(row) = rows.next()? {
            if handle(row.get(0)?, row.get(1)?, &mut out)? {
                break;
            }
        }
    } else {
        // Fewer than three characters: the trigram index cannot be used.
        let mut stmt = conn.prepare("SELECT id, relative_path FROM files ORDER BY relative_path")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            if handle(row.get(0)?, row.get(1)?, &mut out)? {
                break;
            }
        }
    }
    Ok(out)
}

/// Verify a candidate against the real (case-insensitive) substring rule and
/// collect matching lines.
pub fn verify_file(path: &str, content: &str, fq: &str, ctx: usize) -> Option<FileHit> {
    let lines = find_line_matches(content, fq, ctx);
    if !lines.is_empty() || fold(path).contains(fq) {
        Some(FileHit {
            path: path.to_string(),
            lines,
        })
    } else {
        None
    }
}

fn find_line_matches(content: &str, fq: &str, ctx: usize) -> Vec<LineHit> {
    let folded = fold(content);
    let qchars = fq.chars().count();
    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut line_idx = 0usize;
    let mut scanned_to = 0usize;
    for (b, _) in folded.match_indices(fq) {
        line_idx += folded[scanned_to..b].bytes().filter(|&c| c == b'\n').count();
        scanned_to = b;
        if found.last().map(|x| x.0) == Some(line_idx) {
            continue;
        }
        let ls = folded[..b].rfind('\n').map(|p| p + 1).unwrap_or(0);
        found.push((line_idx, folded[ls..b].chars().count()));
        if found.len() >= MAX_LINES_PER_FILE {
            break;
        }
    }
    if found.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = content.split('\n').collect();
    found
        .into_iter()
        .map(|(idx, col)| {
            let before_start = idx.saturating_sub(ctx);
            let after_end = (idx + 1 + ctx).min(lines.len());
            LineHit {
                line_no: idx + 1,
                snippet: make_snippet(lines[idx], col, qchars),
                before: (before_start..idx)
                    .map(|i| (i + 1, truncate_line(lines[i], CONTEXT_LINE_MAX)))
                    .collect(),
                after: (idx + 1..after_end)
                    .map(|i| (i + 1, truncate_line(lines[i], CONTEXT_LINE_MAX)))
                    .collect(),
            }
        })
        .collect()
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

pub fn truncate_line(line: &str, max: usize) -> String {
    let chars: Vec<char> = line.chars().map(clean).collect();
    if chars.len() <= max {
        chars.into_iter().collect()
    } else {
        let mut s: String = chars[..max].iter().collect();
        s.push_str("...");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_unicode_and_ellipsis() {
        let line = format!("{}検索{}", "あ".repeat(50), "い".repeat(50));
        let s = make_snippet(&line, 50, 2);
        assert!(s.starts_with("..."));
        assert!(s.ends_with("..."));
        // 40 + 2 + 40 characters between the ellipses
        assert_eq!(s.chars().count(), 3 + 82 + 3);
        assert_eq!(make_snippet("short SQLite line", 6, 6), "short SQLite line");
    }

    #[test]
    fn phrase_quoting() {
        assert_eq!(fts_phrase("a\"b"), "\"a\"\"b\"");
        assert_eq!(fts_phrase("x' OR 1=1 --"), "\"x' OR 1=1 --\"");
    }

    #[test]
    fn line_numbers_and_limit() {
        let content = "one\nSQLite two\nthree\nsqlite four\nsqlite five\nSQLITE six\n";
        let hit = verify_file("a.txt", content, "sqlite", 0).unwrap();
        let nums: Vec<usize> = hit.lines.iter().map(|l| l.line_no).collect();
        assert_eq!(nums, vec![2, 4, 5]); // at most three lines
    }

    #[test]
    fn path_only_match() {
        let hit = verify_file("notes/Rust.txt", "nothing here", "rust", 0).unwrap();
        assert!(hit.lines.is_empty());
        assert!(verify_file("a.txt", "nothing", "zzz", 0).is_none());
    }

    #[test]
    fn context_lines() {
        let content = "a\nb\nTARGET\nd\ne\n";
        let hit = verify_file("x.txt", content, "target", 2).unwrap();
        let l = &hit.lines[0];
        assert_eq!(l.line_no, 3);
        assert_eq!(l.before.len(), 2);
        assert_eq!(l.after.len(), 2);
    }
}
