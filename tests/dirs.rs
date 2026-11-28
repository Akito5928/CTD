mod common;
use common::*;
use ctd::config::Excludes;
use ctd::walk;
use std::fs;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn deep_path(levels: usize, file: &str) -> String {
    let mut p = String::new();
    for i in 1..=levels {
        p.push_str(&format!("d{i}/"));
    }
    p.push_str(file);
    p
}

#[test]
fn subdirectories_are_searched() {
    let e = Env::new();
    e.write("a/b/c/file.txt", b"findme");
    e.write("top.txt", b"findme");
    assert_eq!(e.search(&["findme"]), s(&["a/b/c/file.txt", "top.txt"]));
}

#[test]
fn depth_limit_is_eight_and_s_removes_it() {
    let e = Env::new();
    for d in [0usize, 1, 7, 8, 9, 12] {
        e.write(
            &deep_path(d, "f.txt"),
            format!("needle at depth {d}").as_bytes(),
        );
    }
    let normal = e.search(&["needle"]);
    assert_eq!(normal.len(), 4, "{normal:?}"); // depths 0, 1, 7, 8
    assert!(normal.contains(&deep_path(8, "f.txt")));
    assert!(!normal.contains(&deep_path(9, "f.txt")));
    let all = e.search_all(&["needle"]);
    assert_eq!(all.len(), 6, "{all:?}");
    assert!(all.contains(&deep_path(12, "f.txt")));
}

#[test]
fn normal_search_after_s_hides_deep_files_but_keeps_them_indexed() {
    let e = Env::new();
    e.write("top.txt", b"needle");
    e.write(&deep_path(10, "deep.txt"), b"needle");
    assert_eq!(e.search_all(&["needle"]).len(), 2);
    // a normal search must neither show nor forget the deep file
    assert_eq!(e.search(&["needle"]), s(&["top.txt"]));
    assert_eq!(e.search_all(&["needle"]).len(), 2);
    let st = e.update(None);
    assert_eq!(st.ingested, 0, "nothing is re-read");
    // when the deep file is deleted, an unlimited scan removes it
    fs::remove_file(e.path(&deep_path(10, "deep.txt"))).unwrap();
    assert_eq!(e.search(&["needle"]), s(&["top.txt"]));
    let st = e.update(None);
    assert_eq!(st.removed, 1);
    assert_eq!(e.search_all(&["needle"]), s(&["top.txt"]));
}

#[test]
fn deep_file_deleted_is_not_noticed_by_a_normal_scan() {
    let e = Env::new();
    e.write(&deep_path(9, "deep.txt"), b"needle");
    assert_eq!(e.search_all(&["needle"]).len(), 1);
    fs::remove_file(e.path(&deep_path(9, "deep.txt"))).unwrap();
    let st = e.update(Some(8));
    assert_eq!(
        st.removed, 0,
        "outside the scanned depth: unknown, not deleted"
    );
    let st = e.update(None);
    assert_eq!(st.removed, 1);
}

#[test]
fn many_files() {
    let e = Env::new();
    for i in 0..3000 {
        e.write(
            &format!("d{}/f{i}.txt", i % 50),
            format!("content {i} common").as_bytes(),
        );
    }
    assert_eq!(e.search(&["common"]).len(), 3000);
    assert_eq!(e.search(&["content 2999"]).len(), 1);
    let st = e.update(Some(8));
    assert_eq!((st.ingested, st.unchanged), (0, 3000));
}

#[test]
fn excluded_names_and_paths_are_skipped() {
    let mut e = Env::new();
    e.write("keep/a.txt", b"needle");
    e.write("$Recycle.Bin/b.txt", b"needle");
    e.write("System Volume Information/c.txt", b"needle");
    e.write("sys/area/d.txt", b"needle");
    e.ex = Excludes::new(
        &[e.path("sys").to_string_lossy().to_string()],
        &["$Recycle.Bin", "System Volume Information"],
        cfg!(windows),
    );
    assert_eq!(e.search_all(&["needle"]), s(&["keep/a.txt"])); // -s does not lift exclusions
}

#[test]
fn dangerous_roots_are_refused() {
    let ex = Excludes::system();
    assert!(ex.check_root(std::path::Path::new("/")).is_err());
    let e = Env::new();
    assert!(ex.check_root(&e.cfg.root).is_ok());
    let inside = Excludes::new(
        &[e.cfg.root.to_string_lossy().to_string()],
        &[],
        cfg!(windows),
    );
    assert!(inside.check_root(&e.cfg.root.join("sub")).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let e = Env::new();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("out.txt"), b"needle outside").unwrap();
    e.write("real.txt", b"needle");
    std::os::unix::fs::symlink(outside.path().join("out.txt"), e.path("link.txt")).unwrap();
    std::os::unix::fs::symlink(outside.path(), e.path("linkdir")).unwrap();
    std::os::unix::fs::symlink(&e.cfg.root, e.path("loop")).unwrap();
    assert_eq!(e.search_all(&["needle"]), s(&["real.txt"]));
}

#[cfg(unix)]
#[test]
fn inaccessible_directory_is_skipped_and_its_files_are_not_deleted() {
    use std::os::unix::fs::PermissionsExt;
    let e = Env::new();
    e.write("ok.txt", b"needle");
    e.write("locked/secret.txt", b"needle");
    assert_eq!(e.search(&["needle"]).len(), 2);
    let locked = e.path("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok(); // true when running as root
    let st = e.update(Some(8));
    let after = e.search(&["needle"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !readable {
        assert_eq!(
            st.removed, 0,
            "unverified files must not be treated as deleted"
        );
        assert_eq!(after.len(), 2);
    }
}

#[test]
fn walk_reports_uncertain_paths() {
    let mut r = walk::WalkResult::default();
    r.failed_dirs.push("locked".into());
    assert!(r.is_uncertain("locked/x.txt"));
    assert!(r.is_uncertain("locked/deep/x.txt"));
    assert!(!r.is_uncertain("lockedx/x.txt"));
    r.failed_files.insert("bad.txt".into());
    assert!(r.is_uncertain("bad.txt"));
    r.failed_dirs.push(String::new());
    assert!(r.is_uncertain("anything.txt"));
}

#[test]
fn unicode_names() {
    let e = Env::new();
    e.write("日本語フォルダ/メモ ファイル.txt", "テスト".as_bytes());
    assert_eq!(
        e.search(&["テスト"]),
        s(&["日本語フォルダ/メモ ファイル.txt"])
    );
    assert_eq!(
        e.search(&["メモ"]),
        s(&["日本語フォルダ/メモ ファイル.txt"])
    );
    assert!(e.search(&["日本語フォルダ"]).len() == 1);
}
