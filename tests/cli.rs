mod common;
use common::tree_snapshot;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

const EXE: &str = env!("CARGO_BIN_EXE_ctd");
/// The release version; the release workflow names its files after it.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `ETXTBSY` ("Text file busy"): the kernel refuses to execute a file that is
/// still open for writing somewhere.
fn is_text_file_busy(e: &io::Error) -> bool {
    cfg!(unix) && e.raw_os_error() == Some(26)
}

/// Run a command, retrying while the kernel reports "Text file busy".
///
/// The tests copy the executable and start the copy right away, and they run
/// in parallel. A child process that another test thread has just forked can
/// still hold the write descriptor of the copy for a moment (until it calls
/// `exec`), which makes our own `exec` fail with ETXTBSY. The condition is
/// transient, so the start is simply repeated; nothing is ever overwritten.
fn output_retry(cmd: &mut Command) -> Output {
    let mut attempt = 0u64;
    loop {
        match cmd.output() {
            Ok(o) => return o,
            Err(e) if is_text_file_busy(&e) && attempt < 100 => {
                attempt += 1;
                thread::sleep(Duration::from_millis(10 * attempt.min(10)));
            }
            Err(e) => panic!("cannot run {cmd:?}: {e}"),
        }
    }
}

/// A search root that contains its own copy of the executable.
struct Install {
    root: TempDir,
    cache: TempDir,
    exe: PathBuf,
}

impl Install {
    fn new() -> Install {
        let root = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) { "ctd.exe" } else { "ctd" };
        let exe = root.path().join(name);
        fs::copy(EXE, &exe).unwrap();
        Install { root, cache, exe }
    }

    fn write(&self, rel: &str, bytes: &[u8]) {
        let p = self.root.path().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    fn run_in(&self, cwd: &Path, args: &[&str]) -> Output {
        output_retry(
            Command::new(&self.exe)
                .args(args)
                .current_dir(cwd)
                .env("XDG_CACHE_HOME", self.cache.path())
                .env("LOCALAPPDATA", self.cache.path())
                .stdin(Stdio::null()),
        )
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(self.root.path(), args)
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn version_and_help() {
    let i = Install::new();
    for args in [&["version"][..], &["--version"][..], &["-V"][..]] {
        let o = i.run(args);
        assert_eq!(o.status.code(), Some(0), "{args:?}");
        assert_eq!(out(&o).trim(), format!("CTD v{VERSION}"));
        assert_eq!(out(&o).trim(), "CTD v1.2.1");
    }
    for args in [&["help"][..], &["--help"][..]] {
        let o = i.run(args);
        assert_eq!(o.status.code(), Some(0));
        assert!(out(&o).contains("Usage") || out(&o).contains("ctd [-s]"));
    }
}

#[test]
fn usage_errors_exit_2() {
    let i = Install::new();
    let o = i.run(&[]);
    assert_eq!(o.status.code(), Some(2));
    assert!(err(&o).contains("ctd"));
    assert_eq!(i.run(&[""]).status.code(), Some(2));
    assert_eq!(i.run(&["   "]).status.code(), Some(2));
    assert_eq!(i.run(&["-s"]).status.code(), Some(2));
    assert_eq!(i.run(&["--bogus"]).status.code(), Some(2));
    // no cache was created by usage errors
    assert!(fs::read_dir(i.cache.path()).unwrap().next().is_none());
}

#[test]
fn search_output_streams_and_exit_codes() {
    let i = Install::new();
    i.write(
        "notes/programming.txt",
        b"Learning a new programming language\nsecond",
    );
    i.write("other.txt", b"only programming");
    let o = i.run(&["programming", "language"]);
    assert_eq!(o.status.code(), Some(0));
    let so = out(&o);
    let se = err(&o);
    assert!(so.contains("[1] notes/programming.txt"));
    assert!(so.contains("Line 1: Learning a new programming language"));
    assert!(!so.contains("other.txt"));
    assert!(
        !so.contains(&format!("CTD v{VERSION}"))
            && !so.contains("Searching")
            && !so.contains("Root:"),
        "stdout = results only: {so}"
    );
    assert!(
        se.contains("Searching: \"programming language\"")
            && se.contains("Root: ")
            && se.contains("Found 1 file.")
    );

    let o = i.run(&["zzz-nothing"]);
    assert_eq!(o.status.code(), Some(0), "no results is not an error");
    assert!(out(&o).contains("No matching files found."));
}

#[test]
fn full_width_space_separates_terms() {
    let i = Install::new();
    i.write("a.txt", "日本語 テスト".as_bytes());
    i.write("b.txt", "日本語だけ".as_bytes());
    let o = i.run(&["日本語\u{3000}テスト"]);
    assert!(out(&o).contains("a.txt") && !out(&o).contains("b.txt"));
}

#[test]
fn reserved_words_only_act_as_commands_when_alone() {
    let i = Install::new();
    i.write("a.txt", b"status report for the team");
    i.write("b.txt", b"the status page");
    // single argument: command
    let o = i.run(&["status"]);
    assert!(out(&o).contains("Cache directory"));
    // several arguments: search
    let o = i.run(&["status", "report"]);
    assert!(out(&o).contains("[1] a.txt") && !out(&o).contains("b.txt"));
    // -- forces a search
    let o = i.run(&["--", "status"]);
    assert!(out(&o).contains("[1] a.txt") && out(&o).contains("[2] b.txt"));
    // -s also forces a search
    let o = i.run(&["-s", "status"]);
    assert!(out(&o).contains("[1] a.txt"));
}

#[test]
fn search_depth_option() {
    let i = Install::new();
    let deep = "a/b/c/d/e/f/g/h/i/deep.txt"; // depth 9
    let ok = "a/b/c/d/e/f/g/h/ok.txt"; // depth 8
    i.write(deep, b"needle");
    i.write(ok, b"needle");
    let o = i.run(&["needle"]);
    assert!(out(&o).contains("ok.txt") && !out(&o).contains("deep.txt"));
    let o = i.run(&["-s", "needle"]);
    assert!(out(&o).contains("ok.txt") && out(&o).contains("deep.txt"));
    assert!(err(&o).contains("Searching with unlimited directory depth..."));
    // back to a normal search: deep file hidden again
    let o = i.run(&["needle"]);
    assert!(!out(&o).contains("deep.txt"));
}

#[test]
fn more_than_twenty_files_reports_the_rest() {
    let i = Install::new();
    for n in 0..27 {
        i.write(&format!("f{n:02}.txt"), b"needle");
    }
    let o = i.run(&["needle"]);
    assert_eq!(out(&o).matches("] f").count(), 20);
    assert!(err(&o).contains("Found 27 files."));
    assert!(err(&o).contains("... and 7 more files."));
}

#[test]
fn status_does_not_touch_the_index() {
    let i = Install::new();
    i.write("a.txt", b"data");
    let o = i.run(&["status"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).contains("Not created yet"));
    assert!(
        fs::read_dir(i.cache.path()).unwrap().next().is_none(),
        "status must not create anything"
    );
    i.run(&["data"]);
    let o = i.run(&["status"]);
    let so = out(&o);
    assert!(
        so.contains("Ready") && so.contains("Indexed text files:\n  1"),
        "{so}"
    );
    assert!(so.contains(&i.cache.path().to_string_lossy().to_string()));
}

#[test]
fn rebuild_command() {
    let i = Install::new();
    i.write("a.txt", b"alpha");
    i.run(&["alpha"]);
    let o = i.run(&["rebuild"]);
    assert_eq!(o.status.code(), Some(0));
    // the root also holds the executable itself (binary, skipped)
    assert!(
        err(&o).contains("Index rebuilt: 2 files scanned, 1 indexed."),
        "{}",
        err(&o)
    );
    assert!(out(&o).is_empty());
    let o = i.run(&["alpha"]);
    assert!(out(&o).contains("a.txt"));
}

#[test]
fn works_from_another_directory_and_after_moving_the_executable() {
    let i = Install::new();
    i.write("a.txt", b"findme here");
    let elsewhere = tempfile::tempdir().unwrap();
    // The search root is where the executable is, not the current directory.
    fs::write(elsewhere.path().join("local.txt"), b"findme in cwd").unwrap();
    let o = i.run_in(elsewhere.path(), &["findme"]);
    assert!(out(&o).contains("a.txt") && !out(&o).contains("local.txt"));

    // Move the executable: the new location is the new search root.
    let moved = tempfile::tempdir().unwrap();
    let new_exe = moved.path().join(i.exe.file_name().unwrap());
    fs::copy(&i.exe, &new_exe).unwrap();
    fs::write(moved.path().join("b.txt"), b"findme moved").unwrap();
    let o = output_retry(
        Command::new(&new_exe)
            .arg("findme")
            .env("XDG_CACHE_HOME", i.cache.path())
            .env("LOCALAPPDATA", i.cache.path())
            .stdin(Stdio::null()),
    );
    assert!(
        out(&o).contains("b.txt") && !out(&o).contains("a.txt"),
        "{}",
        out(&o)
    );
    // two roots -> two cache directories
    let n = fs::read_dir(i.cache.path().join("ctd").join("v1"))
        .unwrap()
        .count();
    assert_eq!(n, 2);
}

#[test]
fn nothing_is_created_in_the_search_root_and_originals_are_untouched() {
    let i = Install::new();
    i.write("a.txt", b"crlf\r\ntext");
    i.write("sub/b.md", "日本語".as_bytes());
    i.write("bad.txt", &[0x82, 0xA0]);
    let before = tree_snapshot(i.root.path());
    i.run(&["crlf"]);
    i.run(&["-s", "日本"]);
    i.run(&["rebuild"]);
    i.run(&["status"]);
    assert_eq!(before, tree_snapshot(i.root.path()));
    assert!(
        i.cache.path().join("ctd").join("v1").is_dir(),
        "index lives in the user cache"
    );
}

#[test]
fn skipped_files_warn_once() {
    let i = Install::new();
    i.write("ok.txt", b"findme");
    i.write("sjis.txt", &[0x82, 0xA0, 0x82, 0xA2]);
    let o = i.run(&["findme"]);
    assert!(err(&o).contains("Warning: skipped sjis.txt"));
    let o = i.run(&["findme"]);
    assert!(!err(&o).contains("Warning"), "cached: no repeated warning");
    let st = out(&i.run(&["status"]));
    assert!(st.contains("not UTF-8: 1"), "{st}");
}

#[test]
fn recovers_from_a_corrupt_index() {
    let i = Install::new();
    i.write("a.txt", b"alpha");
    i.run(&["alpha"]);
    let v1 = i.cache.path().join("ctd").join("v1");
    let dir = fs::read_dir(&v1).unwrap().next().unwrap().unwrap().path();
    fs::write(dir.join("index.bin"), b"corrupt").unwrap();
    let st = out(&i.run(&["status"]));
    assert!(st.contains("Damaged"));
    let o = i.run(&["alpha"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).contains("a.txt") && err(&o).contains("rebuilt"));
}

#[test]
fn dangerous_root_is_refused() {
    // The executable placed inside a system area must refuse to search.
    if cfg!(windows) {
        return; // system folders cannot be written to by the test
    }
    let ex = ctd::config::Excludes::system();
    assert!(ex.check_root(Path::new("/proc")).is_err());
    assert!(ex.check_root(Path::new("/")).is_err());
}

#[test]
fn no_network_or_database_dependencies() {
    let lock = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock")).unwrap();
    let names: Vec<&str> = lock
        .lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .map(|l| l.trim_end_matches('"'))
        .collect();
    for banned in [
        "rusqlite",
        "libsqlite3-sys",
        "sqlite",
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "tokio",
        "socket2",
        "native-tls",
        "rustls",
        "openssl",
        "h2",
        "isahc",
        "attohttpc",
        "mio",
    ] {
        assert!(
            !names.contains(&banned),
            "dependency {banned} must not be used"
        );
    }
}
