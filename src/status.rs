//! `ctd status`: show index information without scanning the search root.

use crate::config::Config;
use crate::store::{Index, LoadResult, State, INDEX_FILE};
use crate::walk::DEFAULT_MAX_DEPTH;
use crate::{output, VERSION};
use anyhow::Result;
use std::io::Write;

/// Civil date from days since 1970-01-01 (proleptic Gregorian).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn format_utc(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let rem = unix % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn cache_size(cfg: &Config) -> u64 {
    std::fs::read_dir(&cfg.cache_dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

pub fn run<W: Write>(cfg: &Config, out: &mut W) -> Result<()> {
    writeln!(out, "CTD v{VERSION}\n")?;
    writeln!(out, "Search root:\n  {}\n", output::display_root(&cfg.root))?;
    writeln!(out, "Cache directory:\n  {}\n", cfg.cache_dir.display())?;
    match Index::load(&cfg.cache_dir) {
        LoadResult::Missing => {
            writeln!(
                out,
                "Index status:\n  Not created yet (it is built on the first search)"
            )?;
        }
        LoadResult::Corrupt(reason) => {
            writeln!(
                out,
                "Index status:\n  Damaged ({reason})\n  It is rebuilt automatically on the next search, or run `ctd rebuild`."
            )?;
        }
        LoadResult::Ready(idx) => {
            let text = idx
                .entries
                .iter()
                .filter(|e| e.state == State::Text)
                .count();
            let count = |s: State| idx.entries.iter().filter(|e| e.state == s).count();
            let deep = idx
                .entries
                .iter()
                .filter(|e| e.state == State::Text && e.depth > DEFAULT_MAX_DEPTH)
                .count();
            writeln!(out, "Index status:\n  Ready\n")?;
            writeln!(out, "Indexed text files:\n  {text}\n")?;
            writeln!(
                out,
                "Skipped files:\n  binary: {}, not UTF-8: {}, UTF-16: {}\n",
                count(State::Binary),
                count(State::NotUtf8),
                count(State::Utf16)
            )?;
            if deep > 0 {
                writeln!(
                    out,
                    "Deeper than {DEFAULT_MAX_DEPTH} levels (searched only with -s):\n  {deep}\n"
                )?;
            }
            writeln!(out, "Last updated:\n  {}\n", format_utc(idx.updated_unix))?;
            writeln!(
                out,
                "Index size:\n  {} KB ({INDEX_FILE} + {} data file(s))",
                cache_size(cfg) / 1024,
                idx.segments.len()
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formatting() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13:20 UTC");
        assert_eq!(format_utc(1_709_164_800), "2024-02-29 00:00:00 UTC");
    }
}
