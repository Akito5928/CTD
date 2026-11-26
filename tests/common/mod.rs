#![allow(dead_code)]
use ctd::config::{Config, Excludes};
use ctd::search;
use ctd::update::{self, UpdateOptions, UpdateStats};
use ctd::walk::DEFAULT_MAX_DEPTH;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

pub struct Env {
    pub root_dir: TempDir,
    pub cache_dir: TempDir,
    pub cfg: Config,
    pub ex: Excludes,
}

impl Env {
    pub fn new() -> Env {
        let root_dir = tempfile::tempdir().unwrap();
        let cache_dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            root: root_dir.path().to_path_buf(),
            cache_dir: cache_dir.path().join("idx"),
        };
        Env { root_dir, cache_dir, cfg, ex: Excludes::new(&[], &[], cfg!(windows)) }
    }

    pub fn path(&self, rel: &str) -> std::path::PathBuf {
        self.cfg.root.join(rel)
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) {
        let p = self.path(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    pub fn set_mtime(&self, rel: &str, secs: u64) {
        let f = fs::OpenOptions::new().write(true).open(self.path(rel)).unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }

    pub fn update(&self, depth: Option<u16>) -> UpdateStats {
        update::update(&self.cfg, &self.ex, &UpdateOptions { max_depth: depth, rebuild: false }).unwrap().1
    }

    pub fn rebuild(&self) -> UpdateStats {
        update::update(
            &self.cfg,
            &self.ex,
            &UpdateOptions { max_depth: Some(DEFAULT_MAX_DEPTH), rebuild: true },
        )
        .unwrap()
        .1
    }

    /// Update (like a real search) and return all matching relative paths.
    pub fn search(&self, terms: &[&str]) -> Vec<String> {
        self.search_depth(terms, Some(DEFAULT_MAX_DEPTH))
    }

    pub fn search_all(&self, terms: &[&str]) -> Vec<String> {
        self.search_depth(terms, None)
    }

    fn search_depth(&self, terms: &[&str], depth: Option<u16>) -> Vec<String> {
        let (idx, _) =
            update::update(&self.cfg, &self.ex, &UpdateOptions { max_depth: depth, rebuild: false }).unwrap();
        let t: Vec<String> = terms.iter().map(|s| s.to_string()).collect();
        search::find(&self.cfg.root, &self.cfg.cache_dir, &idx, &t, depth, usize::MAX)
            .unwrap()
            .hits
            .into_iter()
            .map(|h| h.rel)
            .collect()
    }

    pub fn snapshot_root(&self) -> Vec<(String, Vec<u8>)> {
        tree_snapshot(&self.cfg.root)
    }
}

pub fn tree_snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(d: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, base, out);
            } else {
                out.push((p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"), fs::read(&p).unwrap()));
            }
        }
    }
    let mut v = vec![];
    walk(dir, dir, &mut v);
    v.sort();
    v
}
