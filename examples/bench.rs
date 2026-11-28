//! Repeatable benchmark for CTD.
//!
//! Usage (after `cargo build --release`):
//!   cargo run --release --example bench -- [--sizes 1000,10000,100000] [--keep]
//!
//! Generates deterministic TXT datasets under bench/work/, runs the release
//! `ctd` binary against them and prints a Markdown report. All numbers come
//! from real measurements on the machine that runs it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const EN: &[&str] = &[
    "database",
    "index",
    "search",
    "engine",
    "rust",
    "python",
    "memory",
    "network",
    "thread",
    "cache",
    "query",
    "table",
    "schema",
    "performance",
    "testing",
    "design",
    "pattern",
    "module",
    "compile",
    "buffer",
    "stream",
    "parser",
    "token",
    "file",
    "system",
    "kernel",
    "driver",
    "protocol",
    "packet",
    "server",
    "client",
    "request",
    "response",
    "latency",
    "throughput",
    "benchmark",
    "profile",
    "algorithm",
    "structure",
    "vector",
    "matrix",
    "graph",
    "tree",
    "hash",
    "sort",
    "merge",
    "split",
    "daily",
    "note",
    "idea",
    "project",
    "meeting",
    "memo",
    "todo",
    "plan",
    "review",
    "report",
    "sqlite",
    "text",
    "line",
    "word",
    "phrase",
    "result",
    "error",
    "warning",
    "release",
    "version",
    "update",
];
const JA: &[&str] = &[
    "全文検索",
    "データベース",
    "性能",
    "測定",
    "日本語",
    "文字列",
    "索引",
    "高速",
    "設計",
    "実装",
    "試験",
    "仕様",
    "議事録",
    "予定",
    "計画",
    "報告",
    "資料",
    "調査",
    "分析",
    "改善",
    "ファイル",
    "フォルダ",
    "メモ",
    "アイデア",
    "プログラム",
    "ネットワーク",
    "サーバ",
    "クライアント",
];

fn gen_file(rng: &mut Rng, i: usize, bytes: &mut Vec<u8>) {
    bytes.clear();
    let lines = 20 + rng.below(40);
    for l in 0..lines {
        let words = 6 + rng.below(12);
        for w in 0..words {
            if w > 0 {
                bytes.push(b' ');
            }
            if rng.below(10) < 3 {
                bytes.extend_from_slice(JA[rng.below(JA.len())].as_bytes());
            } else {
                bytes.extend_from_slice(EN[rng.below(EN.len())].as_bytes());
            }
        }
        if i % 997 == 0 && l == 3 {
            bytes.extend_from_slice(b" rarequery7731");
        }
        bytes.push(b'\n');
    }
}

fn file_path(root: &Path, i: usize) -> PathBuf {
    root.join(format!("dir{:03}", i % 200))
        .join(format!("sub{}", (i / 200) % 10))
        .join(format!("file_{i:06}.txt"))
}

fn generate(root: &Path, n: usize) -> u64 {
    let mut rng = Rng(0x9E3779B97F4A7C15 ^ n as u64);
    let mut buf = Vec::new();
    let mut total = 0u64;
    for i in 0..n {
        gen_file(&mut rng, i, &mut buf);
        let p = file_path(root, i);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, &buf).unwrap();
        total += buf.len() as u64;
    }
    total
}

fn ctd_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap(); // target/release
    let name = if cfg!(windows) { "ctd.exe" } else { "ctd" };
    let p = dir.join(name);
    assert!(
        p.exists(),
        "build the release binary first: cargo build --release ({})",
        p.display()
    );
    p
}

fn run(bin: &Path, cwd: &Path, args: &[&str]) -> f64 {
    let t = Instant::now();
    let st = Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .env("XDG_CACHE_HOME", cwd.join("..").join("cache"))
        .env("LOCALAPPDATA", cwd.join("..").join("cache"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let el = t.elapsed().as_secs_f64();
    assert!(st.success(), "ctd {args:?} failed");
    el
}

fn median(bin: &Path, cwd: &Path, args: &[&str], reps: usize) -> f64 {
    let mut v: Vec<f64> = (0..reps).map(|_| run(bin, cwd, args)).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn ms(s: f64) -> String {
    if s >= 1.0 {
        format!("{s:.2} s")
    } else {
        format!("{:.1} ms", s * 1000.0)
    }
}

fn dir_size(p: &Path) -> u64 {
    let mut t = 0;
    if let Ok(rd) = fs::read_dir(p) {
        for e in rd.flatten() {
            let m = e.metadata().unwrap();
            t += if m.is_dir() {
                dir_size(&e.path())
            } else {
                m.len()
            };
        }
    }
    t
}

fn hardware() -> String {
    let mut s = String::new();
    if let Ok(c) = fs::read_to_string("/proc/cpuinfo") {
        if let Some(l) = c.lines().find(|l| l.starts_with("model name")) {
            s += &format!("CPU: {}; ", l.split(':').nth(1).unwrap_or("").trim());
        }
        s += &format!(
            "logical CPUs: {}; ",
            c.lines().filter(|l| l.starts_with("processor")).count()
        );
    }
    if let Ok(m) = fs::read_to_string("/proc/meminfo") {
        if let Some(l) = m.lines().next() {
            s += &format!("{}; ", l.replace("MemTotal:", "RAM:").trim());
        }
    }
    if let Ok(p) = std::env::var("PROCESSOR_IDENTIFIER") {
        s += &format!(
            "CPU: {p}; logical CPUs: {}; ",
            std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_default()
        );
    }
    s += &format!("OS: {}", std::env::consts::OS);
    s
}

fn drop_caches() -> bool {
    fs::write("/proc/sys/vm/drop_caches", "3").is_ok()
}

fn bench_size(src_bin: &Path, base: &Path, n: usize) {
    let root = base.join(format!("ws{n}")).join("root");
    let _ = fs::remove_dir_all(base.join(format!("ws{n}")));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(base.join(format!("ws{n}")).join("cache")).unwrap();
    let bin = root.join(src_bin.file_name().unwrap());
    fs::copy(src_bin, &bin).unwrap();
    eprintln!("[{n}] generating dataset...");
    let t = Instant::now();
    let bytes = generate(&root, n);
    eprintln!("[{n}] generated in {:.1}s", t.elapsed().as_secs_f64());

    let batch = (n / 100).clamp(10, 1000);
    let cache = base.join(format!("ws{n}")).join("cache");
    let t_first = run(&bin, &root, &["rarequery7731"]);
    let cache_bytes = dir_size(&cache);
    eprintln!("[{n}] first search (index build) done");
    let t_rebuild = run(&bin, &root, &["rebuild"]);
    eprintln!("[{n}] rebuild done");

    let cold_ok = drop_caches();
    let t_cold = run(&bin, &root, &["rarequery7731"]);
    let t_uptodate = median(&bin, &root, &["rarequery7731"], 5);
    let t_status = median(&bin, &root, &["status"], 5);

    // new files
    let mut rng = Rng(42);
    let mut buf = Vec::new();
    for k in 0..batch {
        gen_file(&mut rng, n + k, &mut buf);
        let p = file_path(&root, n + k);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, &buf).unwrap();
    }
    let t_new = run(&bin, &root, &["rarequery7731"]);
    // modified files
    for k in 0..batch {
        gen_file(&mut rng, k * 7 % n, &mut buf);
        buf.extend_from_slice(b"modified\n");
        fs::write(file_path(&root, k * 7 % n), &buf).unwrap();
    }
    let t_mod = run(&bin, &root, &["rarequery7731"]);
    // deleted files
    for k in 0..batch {
        let _ = fs::remove_file(file_path(&root, n + k));
    }
    let t_del = run(&bin, &root, &["rarequery7731"]);

    let queries: [(&str, Vec<&str>); 8] = [
        ("rare English word (~0.1% of files)", vec!["rarequery7731"]),
        ("common English word", vec!["database"]),
        ("Japanese word", vec!["全文検索"]),
        ("two terms (AND), both common", vec!["database", "index"]),
        ("three terms (AND)", vec!["database", "index", "全文検索"]),
        ("path (file name)", vec!["file_000042"]),
        ("1 character", vec!["z"]),
        (
            "unlimited depth (-s), rare word",
            vec!["-s", "rarequery7731"],
        ),
    ];
    let mut qrows = String::new();
    for (label, q) in queries.iter() {
        let t = median(&bin, &root, q, 5);
        qrows += &format!("| {label} | `{}` | {} |\n", q.join(" "), ms(t));
    }

    println!("\n### {n} files\n");
    println!(
        "Dataset: {n} files, {:.1} MB of text; index (cache directory): {:.1} MB\n",
        bytes as f64 / 1e6,
        cache_bytes as f64 / 1e6
    );
    println!("| Measurement | Time |\n|---|---|");
    println!(
        "| First search on a new root (builds the full index) | {} |",
        ms(t_first)
    );
    println!("| `rebuild` | {} |", ms(t_rebuild));
    println!(
        "| First search on an up-to-date index ({}) | {} |",
        if cold_ok {
            "OS cache dropped"
        } else {
            "OS cache NOT dropped"
        },
        ms(t_cold)
    );
    println!(
        "| Search on an up-to-date index, rare word (median of 5) | {} |",
        ms(t_uptodate)
    );
    println!("| `status` (no scan, median of 5) | {} |", ms(t_status));
    println!(
        "| Incremental: {batch} new files (search incl. update) | {} |",
        ms(t_new)
    );
    println!("| Incremental: {batch} modified files | {} |", ms(t_mod));
    println!("| Incremental: {batch} deleted files | {} |", ms(t_del));
    println!("\nSearch time on an up-to-date index (process start and change check included, median of 5):\n");
    println!("| Query type | Query | Time |\n|---|---|---|");
    print!("{qrows}");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut sizes = vec![1000usize, 10000];
    let mut keep = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--sizes" => {
                i += 1;
                sizes = args[i]
                    .split(',')
                    .map(|s| s.trim().parse().unwrap())
                    .collect();
            }
            "--keep" => keep = true,
            _ => {}
        }
        i += 1;
    }
    let bin = ctd_bin();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench")
        .join("work");
    fs::create_dir_all(&base).unwrap();
    println!("## CTD benchmark\n");
    println!("Hardware: {}\n", hardware());
    println!("Binary: {}\n", bin.display());
    for n in sizes {
        bench_size(&bin, &base, n);
        if !keep {
            let _ = fs::remove_dir_all(base.join(format!("ws{n}")));
        }
    }
}
