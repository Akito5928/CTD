//! Recursive TXT scanning and stable file reading.

use crate::interrupt;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Path relative to `txts/`, using `/` as separator.
    pub rel: String,
    pub abs: PathBuf,
    pub size: u64,
    pub mtime_ns: i64,
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<ScannedFile>,
    /// Directories (relative, `""` = root) whose contents could not be fully listed.
    pub failed_dirs: Vec<String>,
    /// TXT files that were listed but whose metadata could not be read.
    pub failed_files: HashSet<String>,
    pub warnings: Vec<String>,
}

impl ScanResult {
    /// True when `rel` may exist but was not seen because of a scan problem.
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

fn is_txt_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".txt")
}

/// Scan `txts` recursively. Symbolic links are never followed.
pub fn scan(txts: &Path) -> Result<ScanResult> {
    let root_meta = fs::symlink_metadata(txts).with_context(|| {
        format!(
            "cannot access {} (run `ctd init` to recreate it)",
            txts.display()
        )
    })?;
    if !root_meta.is_dir() {
        bail!("{} is not a directory", txts.display());
    }

    let mut res = ScanResult::default();
    let mut stack: Vec<(PathBuf, String)> = vec![(txts.to_path_buf(), String::new())];

    while let Some((dir, rel_dir)) = stack.pop() {
        interrupt::check()?;
        let rd = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                if rel_dir.is_empty() {
                    return Err(e).with_context(|| format!("cannot read {}", dir.display()));
                }
                res.warnings
                    .push(format!("cannot read directory {rel_dir}: {e}"));
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
                        if rel_dir.is_empty() { "txts" } else { &rel_dir }
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
                    if is_txt_name(name) {
                        res.failed_files.insert(rel.clone());
                    }
                    res.warnings.push(format!("{rel}: {e}"));
                    continue;
                }
            };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push((entry.path(), rel));
            } else if ft.is_file() && is_txt_name(name) {
                match entry.metadata() {
                    Ok(m) => res.files.push(ScannedFile {
                        rel,
                        abs: entry.path(),
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

pub struct FileData {
    pub bytes: Vec<u8>,
    pub size: i64,
    pub mtime_ns: i64,
}

/// Read a file, verifying that it did not change while being read.
/// Retries once; never returns partially read content.
pub fn read_stable(path: &Path) -> std::result::Result<FileData, String> {
    for _ in 0..2 {
        let m1 = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if m1.file_type().is_symlink() {
            return Err("is a symbolic link".to_string());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let m2 = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if m1.len() == m2.len()
            && mtime_ns(&m1) == mtime_ns(&m2)
            && bytes.len() as u64 == m2.len()
        {
            return Ok(FileData {
                bytes,
                size: m2.len() as i64,
                mtime_ns: mtime_ns(&m2),
            });
        }
    }
    Err("file changed while it was being read".to_string())
}
