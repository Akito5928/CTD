mod common;
use common::*;
use ctd::db;
use ctd::sync::{self, Answer, SyncOptions, SyncOutcome};
use std::fs;

fn sync_yes(ws: &ctd::workspace::Workspace) -> SyncOutcome {
    let (_c, o) = sync::prepare(ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    o
}

#[test]
fn detects_new_modified_deleted() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"alpha");
    write(&ws, "b.txt", b"bravo");
    sync_yes(&ws);
    assert_eq!(count(&ws), 2);

    write(&ws, "c.txt", b"charlie");
    write(&ws, "a.txt", b"alpha changed");
    fs::remove_file(ws.txts_dir().join("b.txt")).unwrap();
    let conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    assert_eq!(plan.added.len(), 1);
    assert_eq!(plan.modified.len(), 1);
    assert_eq!(plan.deleted, vec!["b.txt".to_string()]);
    drop(conn);
    match sync_yes(&ws) {
        SyncOutcome::Updated(s) => {
            assert_eq!((s.added, s.updated, s.removed), (1, 1, 1));
        }
        o => panic!("unexpected {o:?}"),
    }
    assert_eq!(content_of(&ws, "a.txt").unwrap(), "alpha changed");
    assert!(content_of(&ws, "b.txt").is_none());
    assert_eq!(sync_yes(&ws), SyncOutcome::UpToDate);
}

#[test]
fn same_size_changed_timestamp_is_detected() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"aaaa");
    set_mtime(&ws, "a.txt", 1_000_000);
    sync_yes(&ws);
    write(&ws, "a.txt", b"bbbb"); // same size
    set_mtime(&ws, "a.txt", 2_000_000);
    let conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    assert_eq!(plan.modified.len(), 1);
}

#[test]
fn unchanged_files_are_not_reprocessed_and_touch_updates_metadata_silently() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"same");
    set_mtime(&ws, "a.txt", 1_000_000);
    sync_yes(&ws);
    // Unchanged: nothing planned, nothing read as a candidate.
    let conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    assert!(!plan.has_changes() && plan.meta_only.is_empty());
    drop(conn);
    // Touch only: same content, new timestamp -> not "Modified"; metadata refreshed.
    set_mtime(&ws, "a.txt", 3_000_000);
    let conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    assert!(!plan.has_changes());
    assert_eq!(plan.meta_only.len(), 1);
    drop(conn);
    assert_eq!(sync_yes(&ws), SyncOutcome::UpToDate);
    let conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    assert!(plan.meta_only.is_empty(), "metadata must have been refreshed");
}

#[test]
fn verify_detects_changes_that_keep_size_and_timestamp() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"aaaa");
    set_mtime(&ws, "a.txt", 1_000_000);
    sync_yes(&ws);
    write(&ws, "a.txt", b"bbbb");
    set_mtime(&ws, "a.txt", 1_000_000);
    let conn = db::open(&ws.db_path()).unwrap();
    assert!(!sync::plan(&ws, &conn, false).unwrap().has_changes());
    let plan = sync::plan(&ws, &conn, true).unwrap();
    assert_eq!(plan.modified.len(), 1);
}

#[test]
fn read_failure_keeps_old_entry_and_is_not_a_deletion() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", "valid UTF-8 ok".as_bytes());
    sync_yes(&ws);
    write(&ws, "a.txt", &[0x82, 0xA0, 0x82, 0xA2, 0x82, 0xA4]); // Shift_JIS
    write(&ws, "new_bad.txt", &[0x82, 0xA0]);
    let conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    assert_eq!(plan.skipped.len(), 2);
    assert!(plan.deleted.is_empty());
    assert!(!plan.has_changes());
    drop(conn);
    sync_yes(&ws);
    assert_eq!(content_of(&ws, "a.txt").unwrap(), "valid UTF-8 ok");
    assert!(content_of(&ws, "new_bad.txt").is_none());
}

#[test]
fn answering_n_skips_and_y_or_yes_updates() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"a");
    let mut p = Fake::interactive(vec![Answer::No]);
    let (_c, o) = sync::prepare(&ws, &SyncOptions::default(), &mut p).unwrap();
    assert_eq!(o, SyncOutcome::Declined);
    assert_eq!(p.asked, 1);
    assert_eq!(count(&ws), 0);

    let mut p = Fake::interactive(vec![Answer::Yes]);
    let (_c, o) = sync::prepare(&ws, &SyncOptions::default(), &mut p).unwrap();
    assert!(matches!(o, SyncOutcome::Updated(_)));
    assert_eq!(count(&ws), 1);
}

#[test]
fn answer_parsing_enter_means_yes() {
    assert_eq!(sync::parse_answer("\n"), Some(Answer::Yes));
    assert_eq!(sync::parse_answer("Y\n"), Some(Answer::Yes));
    assert_eq!(sync::parse_answer(" yes "), Some(Answer::Yes));
    assert_eq!(sync::parse_answer("n\r\n"), Some(Answer::No));
    assert_eq!(sync::parse_answer("maybe"), None);
}

#[test]
fn non_interactive_updates_automatically_and_yes_skips_prompt() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"a");
    let mut p = Fake::non_interactive();
    sync::prepare(&ws, &SyncOptions::default(), &mut p).unwrap();
    assert_eq!((p.asked, count(&ws)), (0, 1));

    write(&ws, "b.txt", b"b");
    let mut p = Fake::interactive(vec![]);
    let opts = SyncOptions { yes: true, ..Default::default() };
    sync::prepare(&ws, &opts, &mut p).unwrap();
    assert_eq!((p.asked, count(&ws)), (0, 2));
}

#[test]
fn no_sync_skips_and_takes_precedence_over_yes() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"a");
    let opts = SyncOptions { yes: true, no_sync: true, ..Default::default() };
    let (_c, o) = sync::prepare(&ws, &opts, &mut Fake::non_interactive()).unwrap();
    assert_eq!(o, SyncOutcome::NotChecked);
    assert_eq!(count(&ws), 0);
}

#[test]
fn failed_transaction_rolls_back_and_preserves_index() {
    let (_d, ws) = new_ws();
    write(&ws, "keep.txt", b"keep");
    sync_yes(&ws);
    {
        let c = db::open(&ws.db_path()).unwrap();
        c.execute_batch(
            "CREATE TRIGGER boom BEFORE INSERT ON files WHEN new.relative_path='z.txt'
             BEGIN SELECT RAISE(ABORT, 'boom'); END;",
        )
        .unwrap();
    }
    write(&ws, "a.txt", b"a");
    write(&ws, "z.txt", b"z");
    fs::remove_file(ws.txts_dir().join("keep.txt")).unwrap();
    let err = sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive());
    assert!(err.is_err());
    assert_eq!(count(&ws), 1);
    assert_eq!(content_of(&ws, "keep.txt").unwrap(), "keep");
    assert!(content_of(&ws, "a.txt").is_none());
}

#[test]
fn file_modified_after_planning_is_picked_up_next_time() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"v1");
    set_mtime(&ws, "a.txt", 1_000_000);
    let mut conn = db::open(&ws.db_path()).unwrap();
    let plan = sync::plan(&ws, &conn, false).unwrap();
    write(&ws, "a.txt", b"version two"); // changes while the prompt is open
    sync::apply(&mut conn, plan).unwrap();
    drop(conn);
    // Stored metadata matches the content that was indexed, so the change is detected next time.
    let conn = db::open(&ws.db_path()).unwrap();
    assert_eq!(sync::plan(&ws, &conn, false).unwrap().modified.len(), 1);
}

#[test]
fn stable_read_returns_consistent_data() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"stable");
    let d = ctd::scan::read_stable(&ws.txts_dir().join("a.txt")).unwrap();
    assert_eq!(d.bytes, b"stable");
    assert_eq!(d.size, 6);
}

#[test]
fn original_txt_files_are_never_modified() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"crlf\r\nline\r\n");
    write(&ws, "bom.txt", b"\xEF\xBB\xBFbom text");
    write(&ws, "bad.txt", &[0x82, 0xA0]);
    set_mtime(&ws, "a.txt", 5);
    let before = tree_snapshot(&ws.txts_dir());
    sync_yes(&ws);
    sync::rebuild(&ws).unwrap();
    sync_yes(&ws);
    assert_eq!(before, tree_snapshot(&ws.txts_dir()));
}
