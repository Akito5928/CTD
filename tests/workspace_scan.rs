mod common;
use common::*;
use ctd::error::CtdError;
use ctd::workspace::{self, Workspace};
use std::fs;

#[test]
fn init_creates_structure() {
    let dir = tempfile::tempdir().unwrap();
    let (ws, out) = workspace::init(dir.path()).unwrap();
    assert!(out.created);
    assert!(ws.txts_dir().is_dir());
    assert!(ws.ctd_dir().is_dir());
    assert!(ws.db_path().is_file());
    assert_eq!(ctd::db::inspect(&ws.db_path()), ctd::db::IndexState::Ready);
}

#[test]
fn init_does_not_overwrite_existing_workspace() {
    let (dir, ws) = new_ws();
    write(&ws, "a.txt", b"keep me");
    let mut p = Fake::non_interactive();
    ctd::sync::prepare(&ws, &Default::default(), &mut p).unwrap();
    assert_eq!(count(&ws), 1);
    let (_, out) = workspace::init(dir.path()).unwrap();
    assert!(!out.created);
    assert_eq!(count(&ws), 1);
    assert_eq!(fs::read(ws.txts_dir().join("a.txt")).unwrap(), b"keep me");
}

#[test]
fn discovers_workspace_from_nested_directory() {
    let (_dir, ws) = new_ws();
    let nested = ws.txts_dir().join("a/b/c");
    fs::create_dir_all(&nested).unwrap();
    let found = Workspace::find_from(&nested).unwrap();
    assert_eq!(found.root, ws.root);
}

#[test]
fn closest_workspace_wins() {
    let (_dir, outer) = new_ws();
    let inner_dir = outer.root.join("sub");
    fs::create_dir_all(&inner_dir).unwrap();
    let (inner, _) = workspace::init(&inner_dir).unwrap();
    assert_eq!(Workspace::find_from(&inner_dir.join("txts")).unwrap().root, inner.root);
}

#[test]
fn no_workspace_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let err = Workspace::find_from(dir.path()).unwrap_err();
    assert!(matches!(err.downcast_ref::<CtdError>(), Some(CtdError::NoWorkspace)));
    assert!(err.to_string().contains("ctd init"));
}

#[test]
fn scan_finds_only_txt_case_insensitive_nested_unicode_empty() {
    let (_dir, ws) = new_ws();
    write(&ws, "a.txt", b"a");
    write(&ws, "B.TXT", b"b");
    write(&ws, "c.Txt", b"c");
    write(&ws, "deep/er/est/d.txt", b"d");
    write(&ws, "日本語/メモ.txt", "メモ".as_bytes());
    write(&ws, "empty.txt", b"");
    write(&ws, "ignore.md", b"x");
    write(&ws, "ignore.txt.bak", b"x");
    write(&ws, "noext", b"x");
    let res = ctd::scan::scan(&ws.txts_dir()).unwrap();
    let names: Vec<&str> = res.files.iter().map(|f| f.rel.as_str()).collect();
    assert_eq!(
        names,
        vec!["B.TXT", "a.txt", "c.Txt", "deep/er/est/d.txt", "empty.txt", "日本語/メモ.txt"]
    );
    assert!(res.files.iter().all(|f| !f.rel.contains('\\')));
}

#[cfg(unix)]
#[test]
fn scan_ignores_symlinks() {
    let (dir, ws) = new_ws();
    write(&ws, "real.txt", b"real");
    let outside = dir.path().join("outside.txt");
    fs::write(&outside, b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, ws.txts_dir().join("link.txt")).unwrap();
    std::os::unix::fs::symlink(ws.txts_dir(), ws.txts_dir().join("loop")).unwrap();
    let res = ctd::scan::scan(&ws.txts_dir()).unwrap();
    let names: Vec<&str> = res.files.iter().map(|f| f.rel.as_str()).collect();
    assert_eq!(names, vec!["real.txt"]);
}

#[cfg(unix)]
#[test]
fn inaccessible_directory_is_handled_and_not_treated_as_deleted() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, ws) = new_ws();
    write(&ws, "ok.txt", b"ok");
    write(&ws, "locked/secret.txt", b"secret");
    let mut p = Fake::non_interactive();
    ctd::sync::prepare(&ws, &Default::default(), &mut p).unwrap();
    assert_eq!(count(&ws), 2);

    let locked = ws.txts_dir().join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable = fs::read_dir(&locked).is_ok(); // true when running as root
    let conn = ctd::db::open(&ws.db_path()).unwrap();
    let plan = ctd::sync::plan(&ws, &conn, false).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !readable {
        assert!(plan.deleted.is_empty(), "files in unreadable dirs must not be deleted");
        assert!(plan.warnings.iter().any(|w| w.contains("locked")));
    }
}

#[test]
fn uncertain_paths_protect_from_deletion() {
    let mut r = ctd::scan::ScanResult::default();
    r.failed_dirs.push("locked".into());
    assert!(r.is_uncertain("locked/x.txt"));
    assert!(r.is_uncertain("locked/deep/x.txt"));
    assert!(!r.is_uncertain("lockedx/x.txt"));
    r.failed_dirs.push(String::new());
    assert!(r.is_uncertain("anything.txt"));
}

#[test]
fn missing_txts_dir_is_an_error_not_mass_deletion() {
    let (_dir, ws) = new_ws();
    write(&ws, "a.txt", b"a");
    let mut p = Fake::non_interactive();
    ctd::sync::prepare(&ws, &Default::default(), &mut p).unwrap();
    fs::remove_dir_all(ws.txts_dir()).unwrap();
    assert!(ctd::sync::prepare(&ws, &Default::default(), &mut Fake::non_interactive()).is_err());
    assert_eq!(count(&ws), 1);
}
