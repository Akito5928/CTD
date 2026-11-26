//! Index update: ingest changed files, drop deleted ones, commit atomically.

use crate::config::{Config, Excludes};
use crate::error::CtdError;
use crate::interrupt;
use crate::store::{self, Entry, Index, LoadResult, Segment, SegmentWriter, State};
use crate::textio::Normalizer;
use crate::walk::{self, FileEntry};
use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bytes inspected to decide whether a file is binary.
pub const HEAD_BYTES: usize = 8192;
const CHUNK: usize = 256 * 1024;
const NEW_SEGMENT: u32 = u32::MAX;
const COMPACT_MAX_SEGMENTS: usize = 24;
const COMPACT_MIN_BYTES: u64 = 16 * 1024 * 1024;
const PROGRESS_EVERY: usize = 5000;
const PROGRESS_MIN_FILES: usize = 500;

static TEST_ABORT_AFTER: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Test hook: make the update fail after `n` ingested files (simulates a crash).
#[doc(hidden)]
pub fn set_test_abort_after(n: usize) {
    TEST_ABORT_AFTER.store(n, Ordering::SeqCst);
}

#[derive(Debug, Clone, Default)]
pub struct UpdateOptions {
    /// `None` = unlimited depth (`-s`).
    pub max_depth: Option<u16>,
    /// Discard the stored index and scan everything again.
    pub rebuild: bool,
}

#[derive(Debug, Default, Clone)]
pub struct UpdateStats {
    pub scanned: usize,
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Files whose metadata changed but whose content did not.
    pub touched: usize,
    /// Files read during this update.
    pub ingested: usize,
    pub first_run: bool,
    pub notice: Option<String>,
    pub committed: bool,
}

impl UpdateStats {
    pub fn changed(&self) -> bool {
        self.added + self.modified + self.removed > 0
    }
}

pub enum Ingested {
    Text { hash: [u8; 32], off: u64, len: u64, size: u64, mtime_ns: i64 },
    Skipped { state: State, size: u64, mtime_ns: i64, reason: Option<String> },
}

pub enum IngestError {
    Unreadable(String),
    Unstable,
    Interrupted,
    Fatal(anyhow::Error),
}

impl From<std::io::Error> for IngestError {
    fn from(e: std::io::Error) -> Self {
        IngestError::Unreadable(e.to_string())
    }
}

/// Read one file in a single streaming pass: classify it, hash it (BLAKE3)
/// and append its normalized text to `seg`. Memory use is independent of the
/// file size. A file that changes while being read is retried once.
pub fn ingest(path: &Path, seg: &mut SegmentWriter) -> Result<Ingested, IngestError> {
    for _ in 0..2 {
        if let Some(r) = ingest_once(path, seg)? {
            return Ok(r);
        }
    }
    Err(IngestError::Unstable)
}

fn read_fill(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let k = f.read(&mut buf[n..])?;
        if k == 0 {
            break;
        }
        n += k;
    }
    Ok(n)
}

fn ingest_once(path: &Path, seg: &mut SegmentWriter) -> Result<Option<Ingested>, IngestError> {
    let m1 = fs::symlink_metadata(path)?;
    if m1.file_type().is_symlink() {
        return Err(IngestError::Unreadable("is a symbolic link".into()));
    }
    let mut f = File::open(path)?;
    let mut head = vec![0u8; HEAD_BYTES];
    let n = read_fill(&mut f, &mut head)?;
    head.truncate(n);

    let size1 = m1.len();
    let mt1 = walk::mtime_ns(&m1);
    // UTF-16 text always contains NUL bytes, so its BOM is checked first
    // (otherwise it would be skipped silently as binary).
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        return Ok(Some(Ingested::Skipped {
            state: State::Utf16,
            size: size1,
            mtime_ns: mt1,
            reason: Some("UTF-16 is not supported".into()),
        }));
    }
    if head.contains(&0) {
        return Ok(Some(Ingested::Skipped { state: State::Binary, size: size1, mtime_ns: mt1, reason: None }));
    }

    let mark = seg.pos();
    let mut hasher = blake3::Hasher::new();
    let mut norm = Normalizer::new();
    let mut out: Vec<u8> = Vec::with_capacity(CHUNK + 16);
    let mut total = head.len() as u64;
    let not_utf8 = |seg: &mut SegmentWriter, reason: String| -> Result<Option<Ingested>, IngestError> {
        seg.rewind(mark).map_err(IngestError::Fatal)?;
        Ok(Some(Ingested::Skipped { state: State::NotUtf8, size: size1, mtime_ns: mt1, reason: Some(reason) }))
    };

    hasher.update(&head);
    if let Err(e) = norm.feed(&head, &mut out) {
        return not_utf8(seg, e);
    }
    seg.write(&out).map_err(IngestError::Fatal)?;
    out.clear();

    if n == HEAD_BYTES {
        let mut chunk = vec![0u8; CHUNK];
        loop {
            if interrupt::is_interrupted() {
                seg.rewind(mark).map_err(IngestError::Fatal)?;
                return Err(IngestError::Interrupted);
            }
            let k = f.read(&mut chunk)?;
            if k == 0 {
                break;
            }
            total += k as u64;
            hasher.update(&chunk[..k]);
            if let Err(e) = norm.feed(&chunk[..k], &mut out) {
                return not_utf8(seg, e);
            }
            seg.write(&out).map_err(IngestError::Fatal)?;
            out.clear();
        }
    }
    if let Err(e) = norm.finish() {
        return not_utf8(seg, e);
    }

    let m2 = fs::symlink_metadata(path)?;
    if m2.len() != size1 || walk::mtime_ns(&m2) != mt1 || total != size1 {
        seg.rewind(mark).map_err(IngestError::Fatal)?;
        return Ok(None); // changed while reading: retry
    }
    Ok(Some(Ingested::Text {
        hash: *hasher.finalize().as_bytes(),
        off: mark,
        len: seg.pos() - mark,
        size: size1,
        mtime_ns: mt1,
    }))
}

/// Copy `len` bytes at `off` of `src` into `dst`.
fn copy_range(src: &mut File, off: u64, len: u64, dst: &mut SegmentWriter) -> Result<()> {
    src.seek(SeekFrom::Start(off))?;
    let mut left = len;
    let mut buf = vec![0u8; 1 << 20];
    while left > 0 {
        let want = left.min(buf.len() as u64) as usize;
        src.read_exact(&mut buf[..want])?;
        dst.write(&buf[..want])?;
        left -= want as u64;
    }
    Ok(())
}

enum Action {
    Keep(usize),
    Ingest(Option<usize>),
}

/// Bring the index in line with the files on disk and commit it.
/// Returns the index to search plus statistics.
pub fn update(cfg: &Config, ex: &Excludes, opts: &UpdateOptions) -> Result<(Index, UpdateStats)> {
    let root_str = cfg.root.to_string_lossy().to_string();
    let mut stats = UpdateStats::default();

    let old = match Index::load(&cfg.cache_dir) {
        LoadResult::Ready(i) if i.root == root_str => i,
        LoadResult::Ready(_) => {
            stats.notice = Some("The stored index belongs to another location and is being rebuilt.".into());
            stats.first_run = true;
            Index::new(&root_str)
        }
        LoadResult::Missing => {
            stats.first_run = true;
            Index::new(&root_str)
        }
        LoadResult::Corrupt(reason) => {
            stats.notice = Some(format!("The index was damaged ({reason}) and is being rebuilt."));
            stats.first_run = true;
            Index::new(&root_str)
        }
    };
    let reuse = !opts.rebuild;

    let w = walk::walk(&cfg.root, opts.max_depth, ex)?;
    for warn in &w.warnings {
        eprintln!("Warning: {warn}");
    }
    stats.scanned = w.files.len();

    // ---- plan -------------------------------------------------------
    let old_map: HashMap<&str, usize> = if reuse {
        old.entries.iter().enumerate().map(|(i, e)| (e.rel.as_str(), i)).collect()
    } else {
        HashMap::new()
    };
    let mut actions: Vec<(&FileEntry, Action)> = Vec::with_capacity(w.files.len());
    let mut to_ingest = 0usize;
    for f in &w.files {
        let a = match old_map.get(f.rel.as_str()) {
            Some(&oi) if old.entries[oi].size == f.size && old.entries[oi].mtime_ns == f.mtime_ns => {
                stats.unchanged += 1;
                Action::Keep(oi)
            }
            other => {
                to_ingest += 1;
                Action::Ingest(other.copied())
            }
        };
        actions.push((f, a));
    }

    // ---- execute ----------------------------------------------------
    let mut dirty = stats.first_run || opts.rebuild;
    let mut new_entries: Vec<Entry> = Vec::with_capacity(w.files.len());
    let mut seg = if to_ingest > 0 { Some(SegmentWriter::create(&cfg.cache_dir)?) } else { None };
    if to_ingest >= PROGRESS_MIN_FILES {
        eprintln!("Indexing {to_ingest} files...");
    }
    let mut done = 0usize;
    let mut fail: Option<anyhow::Error> = None;

    for (f, action) in &actions {
        let oi = match action {
            Action::Keep(oi) => {
                new_entries.push(old.entries[*oi].clone());
                continue;
            }
            Action::Ingest(oi) => *oi,
        };
        if interrupt::is_interrupted() {
            fail = Some(CtdError::Interrupted.into());
            break;
        }
        if done >= TEST_ABORT_AFTER.load(Ordering::SeqCst) {
            fail = Some(anyhow!("simulated failure (test)"));
            break;
        }
        done += 1;
        stats.ingested += 1;
        if to_ingest >= PROGRESS_MIN_FILES && done % PROGRESS_EVERY == 0 {
            eprintln!("  {done}/{to_ingest} files");
        }
        let s = seg.as_mut().expect("segment exists when files must be read");
        let keep_old = |new_entries: &mut Vec<Entry>| {
            if let Some(oi) = oi {
                new_entries.push(old.entries[oi].clone());
            }
        };
        match ingest(&f.abs, s) {
            Ok(Ingested::Text { hash, off, len, size, mtime_ns }) => {
                dirty = true;
                if let Some(oi) = oi {
                    let o = &old.entries[oi];
                    if o.state == State::Text && o.hash == hash {
                        // Same content, new timestamp: reuse the stored text.
                        s.rewind(off).map_err(|e| anyhow!(e))?;
                        let mut e = o.clone();
                        e.size = size;
                        e.mtime_ns = mtime_ns;
                        new_entries.push(e);
                        stats.touched += 1;
                        continue;
                    }
                    stats.modified += 1;
                } else {
                    stats.added += 1;
                }
                new_entries.push(Entry {
                    rel: f.rel.clone(),
                    depth: f.depth,
                    size,
                    mtime_ns,
                    hash,
                    state: State::Text,
                    seg: NEW_SEGMENT,
                    off,
                    len,
                });
            }
            Ok(Ingested::Skipped { state, size, mtime_ns, reason }) => {
                dirty = true;
                if let Some(r) = reason {
                    eprintln!("Warning: skipped {}: {r}", f.rel);
                }
                new_entries.push(Entry {
                    rel: f.rel.clone(),
                    depth: f.depth,
                    size,
                    mtime_ns,
                    hash: [0; 32],
                    state,
                    seg: 0,
                    off: 0,
                    len: 0,
                });
            }
            Err(IngestError::Unreadable(msg)) => {
                eprintln!("Warning: skipped {}: {msg}", f.rel);
                keep_old(&mut new_entries);
            }
            Err(IngestError::Unstable) => {
                eprintln!("Warning: {} changed while it was being read; it will be retried on the next search", f.rel);
                keep_old(&mut new_entries);
            }
            Err(IngestError::Interrupted) => {
                fail = Some(CtdError::Interrupted.into());
                break;
            }
            Err(IngestError::Fatal(e)) => {
                fail = Some(e);
                break;
            }
        }
    }
    if let Some(e) = fail {
        if let Some(s) = seg {
            s.discard();
        }
        return Err(e);
    }

    // Entries that were not walked: deleted, or simply outside this scan.
    if reuse {
        let walked: HashSet<&str> = w.files.iter().map(|f| f.rel.as_str()).collect();
        for e in &old.entries {
            if walked.contains(e.rel.as_str()) {
                continue;
            }
            let in_scope = opts.max_depth.map_or(true, |m| e.depth <= m);
            if in_scope && !w.is_uncertain(&e.rel) {
                dirty = true;
                if e.state == State::Text {
                    stats.removed += 1;
                }
            } else {
                new_entries.push(e.clone());
            }
        }
    }

    let new_seg: Option<Segment> = match seg {
        Some(s) => s.finish()?,
        None => None,
    };
    if !dirty {
        if let Some(s) = &new_seg {
            let _ = fs::remove_file(cfg.cache_dir.join(&s.name));
        }
        return Ok((old, stats));
    }

    new_entries.sort_by(|a, b| a.rel.cmp(&b.rel));

    // Segment table: referenced old segments + the new one.
    let mut remap: HashMap<u32, u32> = HashMap::new();
    let mut segments: Vec<Segment> = Vec::new();
    for e in &mut new_entries {
        if e.state != State::Text {
            continue;
        }
        if e.seg == NEW_SEGMENT {
            continue;
        }
        let ni = *remap.entry(e.seg).or_insert_with(|| {
            segments.push(old.segments[e.seg as usize].clone());
            (segments.len() - 1) as u32
        });
        e.seg = ni;
    }
    if let Some(s) = &new_seg {
        segments.push(s.clone());
        let idx = (segments.len() - 1) as u32;
        for e in &mut new_entries {
            if e.state == State::Text && e.seg == NEW_SEGMENT {
                e.seg = idx;
            }
        }
    }
    let mut idx = Index { root: root_str, updated_unix: store::unix_now(), segments, entries: new_entries };

    // Compaction: merge segments when many exist or most of the data is dead.
    let total: u64 = idx.segments.iter().map(|s| s.len).sum();
    let live: u64 = idx.entries.iter().filter(|e| e.state == State::Text).map(|e| e.len).sum();
    let mut intermediate: Vec<String> = Vec::new();
    if idx.segments.len() > COMPACT_MAX_SEGMENTS || (total > COMPACT_MIN_BYTES && live * 2 < total) {
        compact(cfg, &mut idx)?;
        intermediate = new_seg.iter().map(|s| s.name.clone()).collect();
    }

    if let Err(e) = idx.save_atomic(&cfg.cache_dir) {
        if let Some(s) = &new_seg {
            let _ = fs::remove_file(cfg.cache_dir.join(&s.name));
        }
        return Err(e);
    }
    stats.committed = true;

    let live_names: HashSet<&str> = idx.segments.iter().map(|s| s.name.as_str()).collect();
    let mut dropped: Vec<String> = old
        .segments
        .iter()
        .filter(|s| !live_names.contains(s.name.as_str()))
        .map(|s| s.name.clone())
        .collect();
    dropped.extend(intermediate.into_iter().filter(|n| !live_names.contains(n.as_str())));
    store::cleanup(&cfg.cache_dir, &idx, &dropped);
    Ok((idx, stats))
}

fn compact(cfg: &Config, idx: &mut Index) -> Result<()> {
    let mut w = SegmentWriter::create(&cfg.cache_dir)?;
    let mut files: HashMap<u32, File> = HashMap::new();
    let mut result: Result<()> = Ok(());
    for e in idx.entries.iter_mut() {
        if e.state != State::Text {
            continue;
        }
        if interrupt::is_interrupted() {
            result = Err(CtdError::Interrupted.into());
            break;
        }
        let name = &idx.segments[e.seg as usize].name;
        let f = match files.entry(e.seg) {
            std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
            std::collections::hash_map::Entry::Vacant(v) => match File::open(cfg.cache_dir.join(name)) {
                Ok(f) => v.insert(f),
                Err(err) => {
                    result = Err(err).with_context(|| format!("cannot open segment {name}"));
                    break;
                }
            },
        };
        let new_off = w.pos();
        if let Err(err) = copy_range(f, e.off, e.len, &mut w) {
            result = Err(err);
            break;
        }
        e.off = new_off;
        e.seg = 0;
    }
    if let Err(e) = result {
        w.discard();
        return Err(e);
    }
    drop(files);
    idx.segments = match w.finish()? {
        Some(s) => vec![s],
        None => Vec::new(),
    };
    Ok(())
}
