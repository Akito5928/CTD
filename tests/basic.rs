mod common;
use common::*;
use ctd::search;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn word_partial_case_and_languages() {
    let e = Env::new();
    e.write("a.txt", b"Hello World\nsecond line");
    e.write("b.txt", "日本語のテキスト\nプログラミング".as_bytes());
    e.write("c.txt", "ＡＢＣ　全角テスト".as_bytes());
    assert_eq!(e.search(&["hello"]), s(&["a.txt"])); // case-insensitive
    assert_eq!(e.search(&["WORLD"]), s(&["a.txt"]));
    assert_eq!(e.search(&["ell"]), s(&["a.txt"])); // partial
    assert_eq!(e.search(&["日本語"]), s(&["b.txt"]));
    assert_eq!(e.search(&["プログラ"]), s(&["b.txt"]));
    assert_eq!(e.search(&["ａｂｃ"]), s(&["c.txt"])); // full-width, case-folded
    assert_eq!(e.search(&["全角"]), s(&["c.txt"]));
    assert!(e.search(&["abc"]).is_empty()); // full-width letters are not ASCII
    assert!(e.search(&["nothing-matches-this"]).is_empty());
}

#[test]
fn multiple_terms_are_anded() {
    let e = Env::new();
    e.write("both.txt", b"programming language");
    e.write("one.txt", b"programming only");
    e.write("two.txt", b"language only");
    e.write("sep.txt", b"programming here\nlanguage there");
    assert_eq!(e.search(&["programming", "language"]), s(&["both.txt", "sep.txt"]));
    assert_eq!(e.search(&["language", "programming"]), s(&["both.txt", "sep.txt"])); // order independent
    assert_eq!(e.search(&["programming"]).len(), 3);
}

#[test]
fn terms_may_match_path_or_text_independently() {
    let e = Env::new();
    e.write("notes/rust.txt", b"a language for systems");
    e.write("notes/other.txt", b"a language only");
    assert_eq!(e.search(&["rust", "language"]), s(&["notes/rust.txt"])); // path + body
    assert_eq!(e.search(&["notes/rust"]), s(&["notes/rust.txt"]));
    assert_eq!(e.search(&["RUST.TXT"]), s(&["notes/rust.txt"]));
    // all terms in the path only
    assert_eq!(e.search(&["notes", "other"]), s(&["notes/other.txt"]));
}

#[test]
fn results_are_sorted_by_path() {
    let e = Env::new();
    for n in ["z.txt", "a/b.txt", "m.txt", "a.txt", "B.txt"] {
        e.write(n, b"needle");
    }
    let r = e.search(&["needle"]);
    let mut sorted = r.clone();
    sorted.sort();
    assert_eq!(r, sorted);
    assert_eq!(r.len(), 5);
}

#[test]
fn any_extension_and_no_extension() {
    let e = Env::new();
    e.write("a.txt", b"findme");
    e.write("b.md", b"# findme");
    e.write("c.json", b"{\"k\": \"findme\"}");
    e.write("d.csv", b"x,findme,y\n1,2,3");
    e.write("e.rs", b"fn main() { /* findme */ }");
    e.write("noext", b"findme");
    e.write("log.LOG", b"findme");
    e.write(".hidden", b"findme");
    let r = e.search(&["findme"]);
    assert_eq!(r.len(), 8, "{r:?}");
}

#[test]
fn utf8_bom_and_crlf() {
    let e = Env::new();
    e.write("bom.txt", b"\xEF\xBB\xBFfirst line\r\nsecond\rthird");
    assert_eq!(e.search(&["first"]), s(&["bom.txt"]));
    // BOM bytes are not part of the text
    assert!(e.search(&["\u{feff}first"]).is_empty());
    let (idx, _) = ctd::update::update(&e.cfg, &e.ex, &Default::default()).unwrap();
    let d = search::describe(&e.path("bom.txt"), &s(&["third"]));
    assert_eq!(d.line.unwrap().0, 3, "CR and CRLF count as line breaks");
    assert_eq!(idx.entries.len(), 1);
}

#[test]
fn binary_and_other_encodings_are_skipped() {
    let e = Env::new();
    e.write("ok.txt", b"findme");
    let mut bin = b"findme ".to_vec();
    bin.extend_from_slice(&[0, 1, 2, 3, 0, 0, 0]);
    e.write("data.bin", &bin);
    e.write("sjis.txt", &[0x82, 0xA0, 0x82, 0xA2, 0x82, 0xA4]);
    let mut utf16 = vec![0xFF, 0xFE];
    for c in "findme".encode_utf16() {
        utf16.extend_from_slice(&c.to_le_bytes());
    }
    e.write("utf16.txt", &utf16);
    assert_eq!(e.search(&["findme"]), s(&["ok.txt"]));
    let (idx, _) = ctd::update::update(&e.cfg, &e.ex, &Default::default()).unwrap();
    let state = |n: &str| idx.entries.iter().find(|x| x.rel == n).unwrap().state;
    use ctd::store::State;
    assert_eq!(state("data.bin"), State::Binary);
    assert_eq!(state("sjis.txt"), State::NotUtf8);
    assert_eq!(state("utf16.txt"), State::Utf16);
    assert_eq!(state("ok.txt"), State::Text);
}

#[test]
fn nul_after_the_first_8kb_does_not_make_a_file_binary() {
    let e = Env::new();
    let mut data = vec![b'a'; 9000];
    data.push(0);
    data.extend_from_slice(b" findme");
    e.write("late-nul.txt", &data);
    assert_eq!(e.search(&["findme"]), s(&["late-nul.txt"]));
}

#[test]
fn empty_files_and_empty_directories() {
    let e = Env::new();
    e.write("empty.txt", b"");
    std::fs::create_dir_all(e.path("emptydir/inner")).unwrap();
    e.write("x.txt", b"data");
    assert_eq!(e.search(&["data"]), s(&["x.txt"]));
    assert_eq!(e.search(&["empty"]), s(&["empty.txt"])); // by name
}

#[test]
fn large_file_is_streamed() {
    let e = Env::new();
    // ~12 MB: several chunks; the term sits across a chunk boundary region.
    let mut data = Vec::new();
    for i in 0..400_000 {
        data.extend_from_slice(format!("line number {i} with filler text\n").as_bytes());
    }
    data.extend_from_slice(b"the UNIQUE-marker-xyz at the end\n");
    e.write("big.txt", &data);
    assert_eq!(e.search(&["unique-marker-xyz"]), s(&["big.txt"]));
    assert_eq!(e.search(&["line number 399999", "unique-marker"]), s(&["big.txt"]));
    assert!(e.search(&["line number 400000"]).is_empty());
}

#[test]
fn originals_are_never_modified() {
    let e = Env::new();
    e.write("a.txt", b"crlf\r\nline\r\n");
    e.write("bom.txt", b"\xEF\xBB\xBFbom");
    e.write("bad.txt", &[0x82, 0xA0]);
    e.write("sub/b.bin", &[0, 1, 2]);
    e.set_mtime("a.txt", 12345);
    let before = e.snapshot_root();
    e.search(&["crlf"]);
    e.rebuild();
    e.search_all(&["bom"]);
    assert_eq!(before, e.snapshot_root());
}
