//! Directory walking: depth limit, system exclusions, no link following.

use crate::config::Excludes;
use crate::interrupt;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Standard depth limit (directory levels below the search root).
pub const DEFAULT_MAX_DEPTH: u16 = 8;

#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to the root, `/` separated.
    pub rel: String,
    pub abs: PathBuf,
    /// Depth of the directory containing the file (root = 0).
    pub depth: u16,
    pub size: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Default)]
pub struct WalkResult {
    pub files: Vec<FileEntry>,
    /// Directories (relative, `""` = root) that could not be listed completely.
    pub failed_dirs: Vec<String>,
    /// Files whose metadata could not be read.
    pub failed_files: HashSet<String>,
    pub warnings: Vec<String>,
}

impl WalkResult {
    /// True when `rel` may exist but was not seen because of a walk problem.
    pub fn is_uncertain(&self, rel: &str) -> bool {
        if self.failed_files.contains(rel) {
            return true;
        }
        self.failed_dirs
            .iter()
            .any(|d| d.is_empty() || rel.starts_with(&format!("{d}/")))
    }
}

pub fn mtime_ns(meta: &fs::Metadata) -> i64 {
    match meta.modified() {
        Ok(t) => match t.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_nanos().min(i64::MAX as u128) as i64,
            Err(e) => -(e.duration().as_nanos().min(i64::MAX as u128) as i64),
        },
        Err(_) => 0,
    }
}

/// Walk `root`. `max_depth = None` means unlimited. Symbolic links and
/// junctions are never followed; non-regular files are ignored.
pub fn walk(root: &Path, max_depth: Option<u16>, ex: &Excludes) -> Result<WalkResult> {
    let meta = fs::symlink_metadata(root)
        .with_context(|| format!("cannot access the search root {}", root.display()))?;
    if !meta.is_dir() {
        bail!("{} is not a directory", root.display());
    }
    let mut res = WalkResult::default();
    let mut stack: Vec<(PathBuf, String, u16)> = vec![(root.to_path_buf(), String::new(), 0)];

    while let Some((dir, rel_dir, depth)) = stack.pop() {
        interrupt::check()?;
        let rd = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                if rel_dir.is_empty() {
                    return Err(e).with_context(|| format!("cannot read {}", dir.display()));
                }
                res.warnings.push(format!("cannot read directory {rel_dir}: {e}"));
                res.failed_dirs.push(rel_dir);
                continue;
            }
        };
        for entry in rd {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    res.warnings.push(format!(
                        "error while listing {}: {e}",
                        if rel_dir.is_empty() { "the search root" } else { &rel_dir }
                    ));
                    res.failed_dirs.push(rel_dir.clone());
                    continue;
                }
            };
            let name_os = entry.file_name();
            let Some(name) = name_os.to_str() else {
                res.warnings.push(format!(
                    "skipped (name is not valid Unicode): {}",
                    entry.path().display()
                ));
                continue;
            };
            let rel = if rel_dir.is_empty() {
                name.to_string()
            } else {
                format!("{rel_dir}/{name}")
            };
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(e) => {
                    res.warnings.push(format!("{rel}: {e}"));
                    res.failed_files.insert(rel);
                    continue;
                }
            };
            if ft.is_symlink() {
                continue;
            }
            let abs = entry.path();
            if ft.is_dir() {
                if ex.is_excluded_name(name) || ex.is_excluded_path(&abs) {
                    continue;
                }
                let child_depth = depth + 1;
                if max_depth.map_or(true, |m| child_depth <= m) {
                    stack.push((abs, rel, child_depth));
                }
            } else if ft.is_file() {
                match entry.metadata() {
                    Ok(m) => res.files.push(FileEntry {
                        rel,
                        abs,
                        depth,
                        size: m.len(),
                        mtime_ns: mtime_ns(&m),
                    }),
                    Err(e) => {
                        res.warnings.push(format!("{rel}: {e}"));
                        res.failed_files.insert(rel);
                    }
                }
            }
        }
    }
    res.files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(res)
}
