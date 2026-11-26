//! Search root, system-area exclusions and cache location.

use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};

/// Where to search and where the index lives.
#[derive(Debug, Clone)]
pub struct Config {
    /// Search root (the directory containing the executable).
    pub root: PathBuf,
    /// Per-root cache directory (outside the search root).
    pub cache_dir: PathBuf,
}

impl Config {
    /// Resolve from the executable's location and the user cache directory.
    pub fn from_environment() -> Result<Config> {
        let exe = std::env::current_exe().context("cannot determine the executable's location")?;
        let root = exe
            .parent()
            .ok_or_else(|| anyhow!("cannot determine the executable's directory"))?
            .to_path_buf();
        let root = strip_verbatim(&root);
        let base = cache_base()?;
        Ok(Config {
            cache_dir: cache_dir_for(&base, &root),
            root,
        })
    }
}

/// Remove the Windows `\\?\` prefix when it only wraps a drive path.
pub fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 2 && b[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    p.to_path_buf()
}

/// OS user cache base directory.
pub fn cache_base() -> Result<PathBuf> {
    cache_base_from(
        std::env::var_os("LOCALAPPDATA"),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
        cfg!(windows),
    )
}

pub fn cache_base_from(
    local_app_data: Option<std::ffi::OsString>,
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    windows: bool,
) -> Result<PathBuf> {
    if windows {
        match local_app_data {
            Some(p) if !p.is_empty() => Ok(PathBuf::from(p)),
            _ => bail!("%LOCALAPPDATA% is not set; cannot locate the cache directory"),
        }
    } else {
        if let Some(x) = xdg {
            let p = PathBuf::from(x);
            if p.is_absolute() {
                return Ok(p);
            }
        }
        match home {
            Some(h) if !h.is_empty() => Ok(PathBuf::from(h).join(".cache")),
            _ => bail!("neither $XDG_CACHE_HOME nor $HOME is set; cannot locate the cache directory"),
        }
    }
}

/// `<base>/ctd/v1/<first 16 hex digits of BLAKE3(root path)>`
pub fn cache_dir_for(base: &Path, root: &Path) -> PathBuf {
    let mut key = root.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    if cfg!(windows) {
        key = key.to_lowercase();
    }
    let hash = blake3::hash(key.as_bytes()).to_hex().to_string();
    base.join("ctd").join("v1").join(&hash[..16])
}

/// Paths that are never searched (safety rules; `-s` does not lift them).
#[derive(Debug, Clone)]
pub struct Excludes {
    paths: Vec<String>,
    names: Vec<String>,
    case_insensitive: bool,
}

fn norm_path(s: &str, lower: bool) -> String {
    let mut s = s.replace('\\', "/");
    if let Some(r) = s.strip_prefix("//?/") {
        s = r.to_string();
    }
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if lower {
        s = s.to_lowercase();
    }
    s
}

impl Excludes {
    pub fn new(paths: &[String], names: &[&str], case_insensitive: bool) -> Excludes {
        Excludes {
            paths: paths
                .iter()
                .filter(|p| !p.is_empty())
                .map(|p| norm_path(p, case_insensitive))
                .collect(),
            names: names.iter().map(|n| n.to_lowercase()).collect(),
            case_insensitive,
        }
    }

    /// The exclusions for the current OS.
    pub fn system() -> Excludes {
        if cfg!(windows) {
            let mut paths = Vec::new();
            for var in ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
                if let Ok(v) = std::env::var(var) {
                    paths.push(v);
                }
            }
            Excludes::new(&paths, &["$Recycle.Bin", "System Volume Information"], true)
        } else {
            let paths: Vec<String> = ["/proc", "/sys", "/dev", "/run"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            Excludes::new(&paths, &[], false)
        }
    }

    /// True when `p` is one of the excluded paths or lies inside one.
    pub fn is_excluded_path(&self, p: &Path) -> bool {
        let s = norm_path(&p.to_string_lossy(), self.case_insensitive);
        self.paths
            .iter()
            .any(|e| s == *e || s.starts_with(&format!("{}/", e.trim_end_matches('/'))))
    }

    /// True for directories excluded by name wherever they appear.
    pub fn is_excluded_name(&self, name: &str) -> bool {
        let n = name.to_lowercase();
        self.names.iter().any(|e| *e == n)
    }

    /// Refuse dangerous search roots (filesystem root, system areas).
    pub fn check_root(&self, root: &Path) -> Result<()> {
        if root.parent().is_none() {
            bail!(
                "the search root {} is a filesystem root; searching it is not allowed.\n  \
                 Move the executable into a regular folder.",
                root.display()
            );
        }
        if self.is_excluded_path(root) {
            bail!(
                "the search root {} is inside a system area that CTD never searches.\n  \
                 Move the executable into a regular folder.",
                root.display()
            );
        }
        if let Some(name) = root.file_name().and_then(|n| n.to_str()) {
            if self.is_excluded_name(name) {
                bail!("the search root {} is inside a system area that CTD never searches.", root.display());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_exclusions() {
        let ex = Excludes::new(&["/proc".into(), "/sys".into(), "/dev".into(), "/run".into()], &[], false);
        assert!(ex.is_excluded_path(Path::new("/proc")));
        assert!(ex.is_excluded_path(Path::new("/proc/1/status")));
        assert!(ex.is_excluded_path(Path::new("/run/user/1000")));
        assert!(!ex.is_excluded_path(Path::new("/processing")));
        assert!(!ex.is_excluded_path(Path::new("/home/u")));
        assert!(ex.check_root(Path::new("/")).is_err());
        assert!(ex.check_root(Path::new("/sys/kernel")).is_err());
        assert!(ex.check_root(Path::new("/home/u/ctd")).is_ok());
    }

    #[test]
    fn windows_style_exclusions_are_case_insensitive() {
        let ex = Excludes::new(
            &[r"C:\Windows".into(), r"C:\Program Files".into(), r"C:\Program Files (x86)".into(), r"C:\ProgramData".into()],
            &["$Recycle.Bin", "System Volume Information"],
            true,
        );
        assert!(ex.is_excluded_path(Path::new(r"c:\windows\system32")));
        assert!(ex.is_excluded_path(Path::new(r"C:/PROGRAM FILES/CTD")));
        assert!(ex.is_excluded_path(Path::new(r"\\?\C:\Windows")));
        assert!(!ex.is_excluded_path(Path::new(r"C:\Program Files Extra")));
        assert!(!ex.is_excluded_path(Path::new(r"C:\Users\me\Downloads")));
        assert!(ex.is_excluded_name("$recycle.bin"));
        assert!(ex.is_excluded_name("System Volume Information"));
        assert!(!ex.is_excluded_name("notes"));
        assert!(ex.check_root(Path::new(r"C:\Program Files\CTD")).is_err());
    }

    #[test]
    fn filesystem_roots_are_refused() {
        let ex = Excludes::new(&[], &[], false);
        assert!(ex.check_root(Path::new("/")).is_err());
        if cfg!(windows) {
            assert!(ex.check_root(Path::new(r"C:\")).is_err());
        }
    }

    #[test]
    fn cache_locations() {
        let w = cache_base_from(Some(r"C:\Users\u\AppData\Local".into()), None, None, true).unwrap();
        assert_eq!(w, PathBuf::from(r"C:\Users\u\AppData\Local"));
        assert!(cache_base_from(None, None, None, true).is_err());
        let x = cache_base_from(None, Some("/xdg".into()), Some("/home/u".into()), false).unwrap();
        assert_eq!(x, PathBuf::from("/xdg"));
        let h = cache_base_from(None, None, Some("/home/u".into()), false).unwrap();
        assert_eq!(h, PathBuf::from("/home/u/.cache"));
        let rel = cache_base_from(None, Some("relative".into()), Some("/home/u".into()), false).unwrap();
        assert_eq!(rel, PathBuf::from("/home/u/.cache"));
        let d = cache_dir_for(Path::new("/x"), Path::new("/a/b"));
        assert!(d.starts_with("/x/ctd/v1"));
        assert_eq!(d.file_name().unwrap().len(), 16);
        assert_ne!(d, cache_dir_for(Path::new("/x"), Path::new("/a/c")));
        assert_eq!(d, cache_dir_for(Path::new("/x"), Path::new("/a/b/")));
    }
}
