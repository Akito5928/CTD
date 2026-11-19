mod common;
use common::*;
use ctd::db::{self, IndexState};
use ctd::search::{search, SearchOptions};
use ctd::sync::{self, SyncOptions, SyncOutcome};
use std::fs;

fn setup() -> (tempfile::TempDir, ctd::workspace::Workspace) {
    let (d, ws) = new_ws();
    write(&ws, "programming/rust.txt", b"SQLite is a lightweight database engine\nsecond line\nThe SQLite database is stored locally");
    write(&ws, "ideas/project.txt", "Use SQLite for the search index\r\n日本語の全文検索を試す\r\n".as_bytes());
    write(&ws, "daily/today.txt", b"performance testing\nspecial: 100% _under_ 'quoted' \"dq\" a*b -x NEAR(\n");
    write(&ws, "notes/メモ.txt", "ひらがなとカタカナ\n".as_bytes());
    write(&ws, "ab.txt", b"xy and AB here\nq");
    sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    (d, ws)
}

fn run(ws: &ctd::workspace::Workspace, q: &str, limit: usize, ctx: usize) -> Vec<(String, Vec<usize>)> {
    let conn = db::open(&ws.db_path()).unwrap();
    let r = search(&conn, q, &SearchOptions { limit, context: ctx }).unwrap();
    r.hits
        .into_iter()
        .map(|h| (h.path, h.lines.iter().map(|l| l.line_no).collect()))
        .collect()
}

fn paths(v: &[(String, Vec<usize>)]) -> Vec<&str> {
    v.iter().map(|x| x.0.as_str()).collect()
}

#[test]
fn english_search_with_line_numbers_and_one_result_per_file() {
    let (_d, ws) = setup();
    let r = run(&ws, "sqlite", 20, 0);
    assert_eq!(paths(&r), vec!["programming/rust.txt", "ideas/project.txt"]);
    assert_eq!(r[0].1, vec![1, 3]); // two lines, one file entry
    assert_eq!(r[1].1, vec![1]);
}

#[test]
fn japanese_search_and_crlf_line_numbers() {
    let (_d, ws) = setup();
    let r = run(&ws, "全文検索", 20, 0);
    assert_eq!(paths(&r), vec!["ideas/project.txt"]);
    assert_eq!(r[0].1, vec![2]);
    assert_eq!(paths(&run(&ws, "カタカナ", 20, 0)), vec!["notes/メモ.txt"]);
}

#[test]
fn filename_and_nested_path_search() {
    let (_d, ws) = setup();
    let r = run(&ws, "メモ", 20, 0);
    assert_eq!(paths(&r), vec!["notes/メモ.txt"]);
    assert!(r[0].1.is_empty(), "path-only match has no lines");
    assert_eq!(paths(&run(&ws, "programming/rust", 20, 0)), vec!["programming/rust.txt"]);
    assert_eq!(paths(&run(&ws, "TODAY.txt", 20, 0)), vec!["daily/today.txt"]);
}

#[test]
fn phrases_with_spaces_are_exact() {
    let (_d, ws) = setup();
    assert_eq!(paths(&run(&ws, "lightweight database", 20, 0)), vec!["programming/rust.txt"]);
    assert!(run(&ws, "database lightweight", 20, 0).is_empty());
    assert!(run(&ws, "lightweight  database", 20, 0).is_empty()); // two spaces
}

#[test]
fn short_queries_use_fallback() {
    let (_d, ws) = setup();
    assert_eq!(paths(&run(&ws, "ひ", 20, 0)), vec!["notes/メモ.txt"]); // 1 char
    assert_eq!(run(&ws, "ひ", 20, 0)[0].1, vec![1]);
    assert_eq!(paths(&run(&ws, "日", 20, 0)), vec!["ideas/project.txt"]);
    assert_eq!(paths(&run(&ws, "xy", 20, 0)), vec!["ab.txt"]); // 2 chars
    assert_eq!(paths(&run(&ws, "XY", 20, 0)), vec!["ab.txt"]); // case-insensitive
    assert_eq!(paths(&run(&ws, "日本", 20, 0)), vec!["ideas/project.txt"]);
    assert_eq!(paths(&run(&ws, "and", 20, 0)), vec!["ab.txt"]); // exactly 3 -> indexed path
    assert_eq!(run(&ws, "q", 20, 0).iter().filter(|h| h.0 == "ab.txt").count(), 1);
}

#[test]
fn case_insensitive_everywhere() {
    let (_d, ws) = setup();
    assert_eq!(run(&ws, "SQLITE", 20, 0), run(&ws, "sqlite", 20, 0));
    assert_eq!(run(&ws, "SqLiTe", 20, 0), run(&ws, "sqlite", 20, 0));
}

#[test]
fn special_and_sql_sensitive_characters_are_literal() {
    let (_d, ws) = setup();
    for (q, expect) in [
        ("100%", true),
        ("_under_", true),
        ("'quoted'", true),
        ("\"dq\"", true),
        ("a*b", true),
        ("-x", true),
        ("NEAR(", true),
        ("x' OR '1'='1", false),
        ("'; DROP TABLE files; --", false),
        ("\" OR 1=1 --", false),
        ("%%%", false),
        ("a OR b", false),
    ] {
        let r = run(&ws, q, 20, 0);
        assert_eq!(!r.is_empty(), expect, "query {q:?}");
    }
    assert_eq!(count(&ws), 5, "table must be intact");
}

#[test]
fn limit_and_more_flag() {
    let (_d, ws) = new_ws();
    for i in 0..30 {
        write(&ws, &format!("f{i:02}.txt"), b"needle in a haystack");
    }
    sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    let conn = db::open(&ws.db_path()).unwrap();
    let r = search(&conn, "needle", &SearchOptions { limit: 20, context: 0 }).unwrap();
    assert_eq!(r.hits.len(), 20);
    assert!(r.more);
    let r = search(&conn, "needle", &SearchOptions { limit: 50, context: 0 }).unwrap();
    assert_eq!(r.hits.len(), 30);
    assert!(!r.more);
    // Equal relevance -> deterministic path order.
    let names: Vec<String> = r.hits.iter().map(|h| h.path.clone()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

#[test]
fn context_lines_and_no_results() {
    let (_d, ws) = setup();
    let conn = db::open(&ws.db_path()).unwrap();
    let r = search(&conn, "second", &SearchOptions { limit: 20, context: 1 }).unwrap();
    let l = &r.hits[0].lines[0];
    assert_eq!(l.line_no, 2);
    assert_eq!(l.before[0].0, 1);
    assert_eq!(l.after[0].0, 3);
    assert!(run(&ws, "no such text anywhere", 20, 0).is_empty());
}

#[test]
fn encoding_bom_and_crlf_are_normalized_in_index() {
    let (_d, ws) = new_ws();
    write(&ws, "bom.txt", b"\xEF\xBB\xBFfirst\r\nsecond\rthird");
    sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    assert_eq!(content_of(&ws, "bom.txt").unwrap(), "first\nsecond\nthird");
}

#[test]
fn fts_stays_in_sync_on_update_and_delete() {
    let (_d, ws) = new_ws();
    write(&ws, "a.txt", b"oldword here");
    sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    assert_eq!(run(&ws, "oldword", 20, 0).len(), 1);
    write(&ws, "a.txt", b"newword here, longer");
    sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    assert!(run(&ws, "oldword", 20, 0).is_empty());
    assert_eq!(run(&ws, "newword", 20, 0).len(), 1);
    fs::remove_file(ws.txts_dir().join("a.txt")).unwrap();
    sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap();
    assert!(run(&ws, "newword", 20, 0).is_empty());
    let conn = db::open(&ws.db_path()).unwrap();
    conn.execute("INSERT INTO files_fts(files_fts) VALUES('integrity-check')", []).unwrap();
}

#[test]
fn rebuild_recreates_index() {
    let (_d, ws) = setup();
    let stats = sync::rebuild(&ws).unwrap();
    assert_eq!(stats.added, 5);
    assert_eq!(count(&ws), 5);
    assert_eq!(run(&ws, "sqlite", 20, 0).len(), 2);
}

#[test]
fn failed_rebuild_keeps_old_index() {
    let (_d, ws) = setup();
    fs::remove_dir_all(ws.txts_dir()).unwrap();
    assert!(sync::rebuild(&ws).is_err());
    assert_eq!(count(&ws), 5);
    assert!(!ws.ctd_dir().join("index.db.rebuild-tmp").exists());
}

#[test]
fn missing_and_invalid_database_are_recovered() {
    let (_d, ws) = setup();
    fs::remove_file(ws.db_path()).unwrap();
    assert_eq!(db::inspect(&ws.db_path()), IndexState::Missing);
    // Take only the outcome so the returned connection is closed at once:
    // Windows cannot replace a database file while another handle is open.
    let o = sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap().1;
    assert!(matches!(o, SyncOutcome::Rebuilt(_)));
    assert_eq!(count(&ws), 5);

    fs::write(ws.db_path(), b"this is not a database at all, just garbage text").unwrap();
    assert!(matches!(db::inspect(&ws.db_path()), IndexState::Invalid(_)));
    // Take only the outcome so the returned connection is closed at once:
    // Windows cannot replace a database file while another handle is open.
    let o = sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).unwrap().1;
    assert!(matches!(o, SyncOutcome::Rebuilt(_)));
    assert_eq!(count(&ws), 5);
    // TXT files untouched
    assert_eq!(fs::read(ws.txts_dir().join("ab.txt")).unwrap(), b"xy and AB here\nq");
}

#[test]
fn recovery_respects_no_sync_and_prompt_decline() {
    let (_d, ws) = setup();
    fs::remove_file(ws.db_path()).unwrap();
    let opts = SyncOptions { no_sync: true, ..Default::default() };
    assert!(sync::prepare(&ws, &opts, &mut Fake::non_interactive()).is_err());
    let mut p = Fake::interactive(vec![sync::Answer::No]);
    assert!(sync::prepare(&ws, &SyncOptions::default(), &mut p).is_err());
    assert!(!ws.db_path().exists());
}

#[test]
fn schema_version_handling() {
    let (_d, ws) = setup();
    {
        let c = db::open(&ws.db_path()).unwrap();
        c.execute_batch("PRAGMA user_version = 2").unwrap();
    }
    assert_eq!(db::inspect(&ws.db_path()), IndexState::Unsupported(2));
    let err = sync::prepare(&ws, &SyncOptions::default(), &mut Fake::non_interactive()).err().unwrap();
    assert!(err.to_string().contains("schema version 2"));
    {
        let c = db::open(&ws.db_path()).unwrap();
        c.execute_batch("PRAGMA user_version = 0").unwrap();
    }
    assert!(matches!(db::inspect(&ws.db_path()), IndexState::Invalid(_)));
    {
        let c = db::open(&ws.db_path()).unwrap();
        c.execute_batch("PRAGMA user_version = 1").unwrap();
    }
    assert_eq!(db::inspect(&ws.db_path()), IndexState::Ready);
}
