//! Failure injection lives in its own test binary because the hook is global.
mod common;
use common::*;
use ctd::store::{Index, LoadResult};
use ctd::update::{self, UpdateOptions};
use std::fs;

fn names(e: &Env) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(&e.cfg.cache_dir)
        .map(|rd| {
            rd.flatten()
                .map(|x| x.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn failed_update_keeps_the_previous_index_and_leaves_no_partial_data() {
    let e = Env::new();
    e.write("keep.txt", b"original text");
    e.update(Some(8));
    let index_before = fs::read(e.cfg.cache_dir.join("index.bin")).unwrap();
    let files_before = names(&e);

    for i in 0..10 {
        e.write(&format!("new{i}.txt"), b"new data");
    }
    e.write("keep.txt", b"changed original text!");
    update::set_test_abort_after(3);
    let r = update::update(
        &e.cfg,
        &e.ex,
        &UpdateOptions {
            max_depth: Some(8),
            rebuild: false,
        },
    );
    update::set_test_abort_after(usize::MAX);
    assert!(r.is_err());

    assert_eq!(
        fs::read(e.cfg.cache_dir.join("index.bin")).unwrap(),
        index_before,
        "index untouched"
    );
    assert_eq!(names(&e), files_before, "temporary segment removed");
    assert!(matches!(
        Index::load(&e.cfg.cache_dir),
        LoadResult::Ready(_)
    ));

    // the next run completes the work
    let st = e.update(Some(8));
    assert_eq!((st.added, st.modified), (10, 1));
    assert_eq!(e.search(&["new data"]).len(), 10);
}

#[test]
fn failed_first_build_leaves_no_index() {
    let e = Env::new();
    for i in 0..5 {
        e.write(&format!("f{i}.txt"), b"data");
    }
    update::set_test_abort_after(2);
    let r = update::update(
        &e.cfg,
        &e.ex,
        &UpdateOptions {
            max_depth: Some(8),
            rebuild: false,
        },
    );
    update::set_test_abort_after(usize::MAX);
    assert!(r.is_err());
    assert!(matches!(Index::load(&e.cfg.cache_dir), LoadResult::Missing));
    assert_eq!(e.search(&["data"]).len(), 5);
}

#[test]
fn stale_work_files_from_a_killed_run_are_ignored() {
    // Stale temporary files from an earlier killed run must not matter.
    let e = Env::new();
    e.write("a.txt", b"alpha");
    e.update(Some(8));
    fs::write(e.cfg.cache_dir.join("seg-0123456789abcdef.tmp"), b"partial").unwrap();
    fs::write(
        e.cfg.cache_dir.join("index.bin.tmp-0123456789abcdef"),
        b"partial",
    )
    .unwrap();
    e.write("b.txt", b"bravo");
    assert_eq!(e.search(&["bravo"]), vec!["b.txt".to_string()]);
    assert_eq!(e.search(&["alpha"]), vec!["a.txt".to_string()]);
}
