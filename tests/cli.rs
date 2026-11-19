mod common;
use common::*;
use std::fs;
use std::process::{Command, Output, Stdio};

fn ctd(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctd"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn s(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_string()
}

#[test]
fn version_and_help() {
    let d = tempfile::tempdir().unwrap();
    for args in [&["version"][..], &["--version"][..]] {
        let o = ctd(d.path(), args);
        assert_eq!(o.status.code(), Some(0));
        assert_eq!(s(&o.stdout).trim(), format!("CTD v{}", env!("CARGO_PKG_VERSION")));
    }
    assert_eq!(env!("CARGO_PKG_VERSION"), "1.1.0");
    assert_eq!(ctd(d.path(), &["--help"]).status.code(), Some(0));
    assert_eq!(ctd(d.path(), &["help"]).status.code(), Some(0));
}

#[test]
fn exit_codes_for_usage_errors() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(ctd(d.path(), &[]).status.code(), Some(2));
    assert_eq!(ctd(d.path(), &["bogus"]).status.code(), Some(2));
    assert_eq!(ctd(d.path(), &["search"]).status.code(), Some(2));
    assert_eq!(ctd(d.path(), &["search", ""]).status.code(), Some(2));
    assert_eq!(ctd(d.path(), &["search", "x", "--limit", "0"]).status.code(), Some(2));
    assert_eq!(ctd(d.path(), &["search", "x", "--limit", "-3"]).status.code(), Some(2));
}

#[test]
fn no_workspace_gives_exit_1_and_suggests_init() {
    let d = tempfile::tempdir().unwrap();
    for cmd in [&["search", "x"][..], &["sync"][..], &["status"][..], &["rebuild"][..]] {
        let o = ctd(d.path(), cmd);
        assert_eq!(o.status.code(), Some(1), "{cmd:?}");
        assert!(s(&o.stderr).contains("ctd init"));
    }
}

#[test]
fn init_search_flow_and_stream_separation() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(ctd(d.path(), &["init"]).status.code(), Some(0));
    let again = ctd(d.path(), &["init"]);
    assert!(s(&again.stdout).contains("already initialized"));
    fs::write(d.path().join("txts/a.txt"), "Hello SQLite\n日本語").unwrap();

    let o = ctd(d.path(), &["search", "sqlite"]);
    assert_eq!(o.status.code(), Some(0));
    let out = s(&o.stdout);
    let err = s(&o.stderr);
    assert!(out.contains("[1] a.txt") && out.contains("Line 1:") && out.contains("Hello SQLite"));
    assert!(!out.contains("Checking") && !out.contains("Searching"), "stdout holds results only");
    assert!(err.contains("Changes detected") && err.contains("Index updated successfully"));

    let o = ctd(d.path(), &["search", "日本語"]);
    assert!(s(&o.stdout).contains("日本語"));
    assert_eq!(s(&o.stderr).matches("Index is up to date.").count(), 1);

    let o = ctd(d.path(), &["search", "nothing-like-this"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(s(&o.stdout).contains("No results found for \"nothing-like-this\"."));
}

#[test]
fn works_from_subdirectories() {
    let d = tempfile::tempdir().unwrap();
    ctd(d.path(), &["init"]);
    fs::create_dir_all(d.path().join("txts/sub")).unwrap();
    fs::write(d.path().join("txts/sub/a.txt"), "findme").unwrap();
    let o = ctd(&d.path().join("txts/sub"), &["search", "findme"]);
    assert!(s(&o.stdout).contains("[1] sub/a.txt"));
}

#[test]
fn yes_no_sync_and_precedence() {
    let d = tempfile::tempdir().unwrap();
    ctd(d.path(), &["init"]);
    fs::write(d.path().join("txts/a.txt"), "alpha").unwrap();
    let o = ctd(d.path(), &["search", "alpha", "--no-sync"]);
    assert!(s(&o.stdout).contains("No results"));
    let o = ctd(d.path(), &["search", "alpha", "--yes", "--no-sync"]);
    assert!(s(&o.stdout).contains("No results"), "--no-sync wins over --yes");
    let o = ctd(d.path(), &["--yes", "search", "alpha"]);
    assert!(s(&o.stdout).contains("[1] a.txt"));
    let o = ctd(d.path(), &["sync", "--no-sync"]);
    assert_eq!(o.status.code(), Some(0));
}

#[test]
fn sync_verify_rebuild_status() {
    let d = tempfile::tempdir().unwrap();
    ctd(d.path(), &["init"]);
    fs::write(d.path().join("txts/a.txt"), "alpha").unwrap();
    let st = ctd(d.path(), &["status"]);
    assert!(s(&st.stdout).contains("Ready"));
    assert!(s(&st.stdout).contains("  0"), "status must not sync");
    assert_eq!(ctd(d.path(), &["sync"]).status.code(), Some(0));
    assert!(s(&ctd(d.path(), &["status"]).stdout).contains("  1\n"));
    let v = ctd(d.path(), &["sync", "--verify"]);
    assert!(s(&v.stderr).contains("Index is up to date."));
    let r = ctd(d.path(), &["rebuild"]);
    assert_eq!(r.status.code(), Some(0));
    assert!(s(&r.stderr).contains("Index rebuilt successfully"));

    fs::remove_file(d.path().join(".ctd/index.db")).unwrap();
    assert!(s(&ctd(d.path(), &["status"]).stdout).contains("Missing"));
    fs::write(d.path().join(".ctd/index.db"), "garbage garbage garbage").unwrap();
    assert!(s(&ctd(d.path(), &["status"]).stdout).contains("Invalid"));
    fs::remove_file(d.path().join(".ctd/index.db")).unwrap();
    let o = ctd(d.path(), &["search", "alpha"]); // non-interactive: auto rebuild
    assert!(s(&o.stdout).contains("[1] a.txt"));
}

#[test]
fn invalid_utf8_warns_every_run_and_counts_skipped() {
    let d = tempfile::tempdir().unwrap();
    ctd(d.path(), &["init"]);
    fs::write(d.path().join("txts/sjis.txt"), [0x82u8, 0xA0, 0x82, 0xA2]).unwrap();
    fs::write(d.path().join("txts/ok.txt"), "fine").unwrap();
    for _ in 0..2 {
        let o = ctd(d.path(), &["search", "fine"]);
        assert_eq!(o.status.code(), Some(0));
        assert!(s(&o.stderr).contains("Warning: skipped sjis.txt"));
        assert!(s(&o.stdout).contains("ok.txt"));
    }
    let first = ctd(d.path(), &["rebuild"]);
    assert!(s(&first.stderr).contains("Skipped: 1"));
}

#[test]
fn context_and_limit_options() {
    let d = tempfile::tempdir().unwrap();
    ctd(d.path(), &["init"]);
    for i in 0..5 {
        fs::write(d.path().join(format!("txts/f{i}.txt")), "a\nneedle\nc").unwrap();
    }
    let o = ctd(d.path(), &["search", "needle", "--limit", "2", "--context", "1"]);
    let out = s(&o.stdout);
    assert_eq!(out.matches("] f").count(), 2);
    assert!(out.contains("> 2: needle"));
    assert!(s(&o.stderr).contains("more files match"));
    assert_eq!(out.matches("1: a").count(), 2);
}

#[test]
fn tmp_workspace_is_the_only_thing_touched() {
    // The tests above only use temp dirs; this asserts originals are byte-identical.
    let (_d, ws) = new_ws();
    write(&ws, "x.txt", "日本語\r\n".as_bytes());
    let before = tree_snapshot(&ws.txts_dir());
    let o = ctd(&ws.root, &["--yes", "search", "日本"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(before, tree_snapshot(&ws.txts_dir()));
}
