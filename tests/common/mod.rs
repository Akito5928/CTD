#![allow(dead_code)]
use ctd::sync::{Answer, Prompter};
use ctd::workspace::{self, Workspace};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

pub fn new_ws() -> (TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let (ws, _) = workspace::init(dir.path()).unwrap();
    (dir, ws)
}

pub fn write(ws: &Workspace, rel: &str, bytes: &[u8]) {
    let p = ws.txts_dir().join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

pub fn set_mtime(ws: &Workspace, rel: &str, secs_after_epoch: u64) {
    let f = fs::OpenOptions::new()
        .write(true)
        .open(ws.txts_dir().join(rel))
        .unwrap();
    f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(secs_after_epoch))
        .unwrap();
}

pub struct Fake {
    pub interactive: bool,
    pub answers: Vec<Answer>,
    pub asked: usize,
}

impl Fake {
    pub fn non_interactive() -> Self {
        Fake { interactive: false, answers: vec![], asked: 0 }
    }
    pub fn interactive(answers: Vec<Answer>) -> Self {
        Fake { interactive: true, answers, asked: 0 }
    }
}

impl Prompter for Fake {
    fn interactive(&self) -> bool {
        self.interactive
    }
    fn ask_yes_no(&mut self, _q: &str) -> anyhow::Result<Answer> {
        self.asked += 1;
        Ok(self.answers.remove(0))
    }
}

pub fn count(ws: &Workspace) -> i64 {
    let c = ctd::db::open(&ws.db_path()).unwrap();
    ctd::db::count_files(&c).unwrap()
}

pub fn content_of(ws: &Workspace, rel: &str) -> Option<String> {
    let c = ctd::db::open(&ws.db_path()).unwrap();
    c.query_row("SELECT content FROM files WHERE relative_path=?1", [rel], |r| r.get(0))
        .ok()
}

pub fn tree_snapshot(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(d: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(d).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            if p.is_dir() {
                walk(&p, base, out);
            } else {
                out.push((
                    p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"),
                    fs::read(&p).unwrap(),
                ));
            }
        }
    }
    let mut v = vec![];
    walk(dir, dir, &mut v);
    v.sort();
    v
}
