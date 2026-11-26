mod common;
use common::*;
use ctd::store::{Index, LoadResult, State};
use ctd::update::{self, UpdateOptions};
use std::fs;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn load(e: &Env) -> Index {
    match Index::load(&e.cfg.cache_dir) {
        LoadResult::Ready(i) => i,
        other => panic!("index not ready: {other:?}"),
    }
}

#[test]
fn first_search_creates_the_index_outside_the_root() {
    let e = Env::new();
    e.write("a.txt", b"data");
    let before = e.snapshot_root();
    let st = e.update(Some(8));
    assert!(st.first_run && st.committed);
    assert!(e.cfg.cache_dir.join("index.bin").is_file());
    assert!(!e.cfg.cache_dir.starts_with(&e.cfg.root));
    assert_eq!(before, e.snapshot_root(), "no management files in the search root");
    let names: Vec<String> = fs::read_dir(&e.cfg.root).unwrap().map(|x| x.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(names, vec!["a.txt"]);
}

#[test]
fn second_run_reuses_the_index() {
    let e = Env::new();
    for i in 0..20 {
        e.write(&format!("f{i}.txt"), format!("text {i}").as_bytes());
    }
    let st = e.update(Some(8));
    assert_eq!((st.added, st.ingested), (20, 20));
    let st = e.update(Some(8));
    assert_eq!((st.ingested, st.unchanged, st.added, st.modified, st.removed), (0, 20, 0, 0, 0));
    assert!(!st.committed, "nothing to write");
}

#[test]
fn detects_new_modified_and_deleted_files() {
    let e = Env::new();
    e.write("a.txt", b"alpha");
    e.write("b.txt", b"bravo");
    e.update(Some(8));
    e.write("c.txt", b"charlie");
    e.write("a.txt", b"alpha changed text");
    fs::remove_file(e.path("b.txt")).unwrap();
    let st = e.update(Some(8));
    assert_eq!((st.added, st.modified, st.removed), (1, 1, 1));
    assert_eq!(e.search(&["charlie"]), s(&["c.txt"]));
    assert_eq!(e.search(&["changed"]), s(&["a.txt"]));
    assert!(e.search(&["bravo"]).is_empty());
    assert!(e.search(&["alpha"]).len() == 1);
    assert_eq!(e.update(Some(8)).ingested, 0);
}

#[test]
fn same_size_changed_timestamp_is_detected() {
    let e = Env::new();
    e.write("a.txt", b"aaaa");
    e.set_mtime("a.txt", 1_000_000);
    e.update(Some(8));
    e.write("a.txt", b"bbbb");
    e.set_mtime("a.txt", 2_000_000);
    let st = e.update(Some(8));
    assert_eq!(st.modified, 1);
    assert_eq!(e.search(&["bbbb"]), s(&["a.txt"]));
}

#[test]
fn touched_file_with_same_content_is_not_modified() {
    let e = Env::new();
    e.write("a.txt", b"same content");
    e.set_mtime("a.txt", 1_000_000);
    e.update(Some(8));
    e.set_mtime("a.txt", 3_000_000);
    let st = e.update(Some(8));
    assert_eq!((st.modified, st.touched), (0, 1));
    assert_eq!(e.update(Some(8)).ingested, 0, "metadata was refreshed");
    assert_eq!(e.search(&["same content"]), s(&["a.txt"]));
}

#[test]
fn unchanged_skipped_files_are_not_reread_or_rewarned() {
    let e = Env::new();
    e.write("sjis.txt", &[0x82, 0xA0, 0x82, 0xA2]);
    e.write("data.bin", &[0, 1, 2, 3]);
    let st = e.update(Some(8));
    assert_eq!(st.ingested, 2);
    let st = e.update(Some(8));
    assert_eq!((st.ingested, st.unchanged), (0, 2));
}

#[test]
fn rebuild_recreates_everything() {
    let e = Env::new();
    e.write("a.txt", b"alpha");
    e.write("b.txt", b"bravo");
    e.update(Some(8));
    let st = e.rebuild();
    assert_eq!(st.ingested, 2);
    assert_eq!(e.search(&["bravo"]), s(&["b.txt"]));
    assert_eq!(load(&e).entries.iter().filter(|x| x.state == State::Text).count(), 2);
}

#[test]
fn corrupt_index_is_rebuilt() {
    let e = Env::new();
    e.write("a.txt", b"alpha");
    e.update(Some(8));
    fs::write(e.cfg.cache_dir.join("index.bin"), b"this is garbage, not an index").unwrap();
    let (_, st) = update::update(&e.cfg, &e.ex, &UpdateOptions { max_depth: Some(8), rebuild: false }).unwrap();
    assert!(st.notice.is_some());
    assert_eq!(e.search(&["alpha"]), s(&["a.txt"]));
    // flipped byte -> checksum mismatch
    let p = e.cfg.cache_dir.join("index.bin");
    let mut b = fs::read(&p).unwrap();
    let mid = b.len() / 2;
    b[mid] ^= 0xFF;
    fs::write(&p, b).unwrap();
    assert_eq!(e.search(&["alpha"]), s(&["a.txt"]));
    assert!(matches!(Index::load(&e.cfg.cache_dir), LoadResult::Ready(_)));
}

#[test]
fn damaged_or_missing_segment_is_recovered() {
    let e = Env::new();
    e.write("a.txt", b"alpha");
    e.update(Some(8));
    let seg = load(&e).segments[0].name.clone();
    let p = e.cfg.cache_dir.join(&seg);
    let data = fs::read(&p).unwrap();
    fs::write(&p, &data[..data.len() - 2]).unwrap(); // truncated
    assert_eq!(e.search(&["alpha"]), s(&["a.txt"]));
    let seg = load(&e).segments[0].name.clone();
    fs::remove_file(e.cfg.cache_dir.join(&seg)).unwrap(); // missing
    assert_eq!(e.search(&["alpha"]), s(&["a.txt"]));
}

#[test]
fn leftover_work_files_do_not_matter() {
    let e = Env::new();
    e.write("a.txt", b"alpha");
    e.update(Some(8));
    fs::write(e.cfg.cache_dir.join("index.bin.tmp-deadbeef"), b"half written").unwrap();
    fs::write(e.cfg.cache_dir.join("seg-deadbeef.tmp"), b"half written").unwrap();
    e.write("b.txt", b"bravo");
    assert_eq!(e.search(&["bravo"]), s(&["b.txt"]));
    assert_eq!(e.search(&["alpha"]), s(&["a.txt"]));
}

#[test]
fn many_small_updates_are_compacted() {
    let e = Env::new();
    e.write("base.txt", b"base content");
    e.update(Some(8));
    for i in 0..30 {
        e.write(&format!("f{i}.txt"), format!("update number {i}").as_bytes());
        e.update(Some(8));
    }
    let idx = load(&e);
    assert!(idx.segments.len() <= 25, "{} segments", idx.segments.len());
    assert_eq!(e.search(&["update number"]).len(), 30);
    assert_eq!(e.search(&["base content"]), s(&["base.txt"]));
    // dead segments are removed from the cache directory
    let seg_files = fs::read_dir(&e.cfg.cache_dir)
        .unwrap()
        .filter(|x| x.as_ref().unwrap().file_name().to_string_lossy().starts_with("seg-"))
        .count();
    assert_eq!(seg_files, idx.segments.len());
}

#[test]
fn modified_file_replaces_old_text() {
    let e = Env::new();
    e.write("a.txt", b"oldword here");
    e.update(Some(8));
    e.write("a.txt", b"newword here and more");
    assert!(e.search(&["oldword"]).is_empty());
    assert_eq!(e.search(&["newword"]), s(&["a.txt"]));
}

#[test]
fn each_root_has_its_own_cache() {
    let base = tempfile::tempdir().unwrap();
    let a = ctd::config::cache_dir_for(base.path(), std::path::Path::new("/some/root/a"));
    let b = ctd::config::cache_dir_for(base.path(), std::path::Path::new("/some/root/b"));
    assert_ne!(a, b);
    assert!(a.starts_with(base.path().join("ctd").join("v1")));
}
