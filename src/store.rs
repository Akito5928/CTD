//! CTD's own index format (no database).
//!
//! Cache directory layout:
//!   index.bin          committed metadata (replaced atomically)
//!   seg-<id>.dat       immutable segments holding the case-folded text
//!   *.tmp / *.tmp-*    work files that are never read as an index

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const FORMAT_VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"CTDIDX\0\x01";
pub const INDEX_FILE: &str = "index.bin";
/// Work files and unreferenced segments older than this are removed.
const ORPHAN_MAX_AGE: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Text; content stored in a segment.
    Text = 0,
    /// Binary (NUL in the first 8 KB); skipped silently.
    Binary = 1,
    /// Not valid UTF-8; skipped with a warning.
    NotUtf8 = 2,
    /// UTF-16 (BOM); skipped with a warning.
    Utf16 = 3,
}

impl State {
    fn from_u8(v: u8) -> Option<State> {
        match v {
            0 => Some(State::Text),
            1 => Some(State::Binary),
            2 => Some(State::NotUtf8),
            3 => Some(State::Utf16),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub rel: String,
    pub depth: u16,
    pub size: u64,
    pub mtime_ns: i64,
    pub hash: [u8; 32],
    pub state: State,
    /// Index into `Index::segments` (Text only).
    pub seg: u32,
    pub off: u64,
    pub len: u64,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub name: String,
    pub len: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Index {
    pub root: String,
    pub updated_unix: u64,
    pub segments: Vec<Segment>,
    /// Sorted by `rel`.
    pub entries: Vec<Entry>,
}

#[derive(Debug)]
pub enum LoadResult {
    Missing,
    Corrupt(String),
    Ready(Index),
}

struct W(Vec<u8>);
impl W {
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.p + n > self.b.len() {
            return Err("unexpected end of index".to_string());
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn str(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "invalid text in index".to_string())
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A unique suffix for work files (several ctd processes may run at once).
pub fn unique_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let h = blake3::hash(format!("{}-{}", std::process::id(), nanos).as_bytes()).to_hex().to_string();
    h[..16].to_string()
}

impl Index {
    pub fn new(root: &str) -> Index {
        Index { root: root.to_string(), ..Default::default() }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut w = W(Vec::with_capacity(64 + self.entries.len() * 110));
        w.0.extend_from_slice(MAGIC);
        w.u32(FORMAT_VERSION);
        w.str(&self.root);
        w.u64(self.updated_unix);
        w.u32(self.segments.len() as u32);
        for s in &self.segments {
            w.str(&s.name);
            w.u64(s.len);
        }
        w.u64(self.entries.len() as u64);
        for e in &self.entries {
            w.str(&e.rel);
            w.u16(e.depth);
            w.u64(e.size);
            w.i64(e.mtime_ns);
            w.0.extend_from_slice(&e.hash);
            w.0.push(e.state as u8);
            w.u32(e.seg);
            w.u64(e.off);
            w.u64(e.len);
        }
        let sum = blake3::hash(&w.0);
        w.0.extend_from_slice(sum.as_bytes());
        w.0
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Index, String> {
        if bytes.len() < MAGIC.len() + 4 + 32 {
            return Err("index file is too short".to_string());
        }
        let (body, sum) = bytes.split_at(bytes.len() - 32);
        if blake3::hash(body).as_bytes() != sum {
            return Err("checksum mismatch".to_string());
        }
        let mut r = R { b: body, p: 0 };
        if r.take(8)? != MAGIC {
            return Err("not a CTD index".to_string());
        }
        let ver = r.u32()?;
        if ver != FORMAT_VERSION {
            return Err(format!("unsupported index format version {ver}"));
        }
        let root = r.str()?;
        let updated_unix = r.u64()?;
        let nseg = r.u32()? as usize;
        let mut segments = Vec::with_capacity(nseg.min(1 << 16));
        for _ in 0..nseg {
            segments.push(Segment { name: r.str()?, len: r.u64()? });
        }
        let n = r.u64()? as usize;
        let mut entries = Vec::with_capacity(n.min(1 << 24));
        for _ in 0..n {
            let rel = r.str()?;
            let depth = r.u16()?;
            let size = r.u64()?;
            let mtime_ns = r.i64()?;
            let hash: [u8; 32] = r.take(32)?.try_into().unwrap();
            let state = State::from_u8(r.u8()?).ok_or("invalid entry state")?;
            let seg = r.u32()?;
            let off = r.u64()?;
            let len = r.u64()?;
            if state == State::Text {
                let s = segments.get(seg as usize).ok_or("entry refers to a missing segment")?;
                if off.checked_add(len).map_or(true, |end| end > s.len) {
                    return Err("entry exceeds its segment".to_string());
                }
            }
            entries.push(Entry { rel, depth, size, mtime_ns, hash, state, seg, off, len });
        }
        if r.p != body.len() {
            return Err("trailing data in index".to_string());
        }
        Ok(Index { root, updated_unix, segments, entries })
    }

    /// Load and validate the committed index of `cache_dir`.
    pub fn load(cache_dir: &Path) -> LoadResult {
        let path = cache_dir.join(INDEX_FILE);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LoadResult::Missing,
            Err(e) => return LoadResult::Corrupt(format!("cannot read index: {e}")),
        };
        let idx = match Index::deserialize(&bytes) {
            Ok(i) => i,
            Err(e) => return LoadResult::Corrupt(e),
        };
        for s in &idx.segments {
            match fs::metadata(cache_dir.join(&s.name)) {
                Ok(m) if m.len() == s.len => {}
                Ok(_) => return LoadResult::Corrupt(format!("segment {} has the wrong size", s.name)),
                Err(_) => return LoadResult::Corrupt(format!("segment {} is missing", s.name)),
            }
        }
        LoadResult::Ready(idx)
    }

    /// Commit: write a temporary file, flush it, then rename it over `index.bin`.
    pub fn save_atomic(&self, cache_dir: &Path) -> Result<()> {
        fs::create_dir_all(cache_dir)
            .with_context(|| format!("cannot create the cache directory {}", cache_dir.display()))?;
        let tmp = cache_dir.join(format!("{INDEX_FILE}.tmp-{}", unique_id()));
        let result = (|| -> Result<()> {
            let mut f = File::create(&tmp)?;
            f.write_all(&self.serialize())?;
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, cache_dir.join(INDEX_FILE))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.context("cannot save the index (the previous index was kept)")
    }
}

/// Appends case-folded text to a new segment file.
pub struct SegmentWriter {
    pub name: String,
    tmp_path: PathBuf,
    final_path: PathBuf,
    file: File,
    buf: Vec<u8>,
    /// Logical end of data (flushed + buffered).
    pos: u64,
}

impl SegmentWriter {
    pub fn create(cache_dir: &Path) -> Result<SegmentWriter> {
        fs::create_dir_all(cache_dir)
            .with_context(|| format!("cannot create the cache directory {}", cache_dir.display()))?;
        let id = unique_id();
        let name = format!("seg-{id}.dat");
        let tmp_path = cache_dir.join(format!("seg-{id}.tmp"));
        let file = OpenOptions::new().create_new(true).write(true).open(&tmp_path)?;
        Ok(SegmentWriter { name, final_path: cache_dir.join(format!("seg-{id}.dat")), tmp_path, file, buf: Vec::new(), pos: 0 })
    }

    pub fn pos(&self) -> u64 {
        self.pos
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.buf.extend_from_slice(data);
        self.pos += data.len() as u64;
        if self.buf.len() >= 1 << 20 {
            self.flush_buf()?;
        }
        Ok(())
    }

    fn flush_buf(&mut self) -> Result<()> {
        if !self.buf.is_empty() {
            self.file.write_all(&self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }

    /// Discard everything written after `mark` (a value of `pos()`).
    pub fn rewind(&mut self, mark: u64) -> Result<()> {
        self.flush_buf()?;
        self.file.set_len(mark)?;
        self.file.seek(SeekFrom::Start(mark))?;
        self.pos = mark;
        Ok(())
    }

    /// Finish: flush to disk and publish under the final name.
    pub fn finish(mut self) -> Result<Option<Segment>> {
        self.flush_buf()?;
        if self.pos == 0 {
            drop(self.file);
            let _ = fs::remove_file(&self.tmp_path);
            return Ok(None);
        }
        self.file.sync_all()?;
        drop(self.file);
        fs::rename(&self.tmp_path, &self.final_path)?;
        Ok(Some(Segment { name: self.name, len: self.pos }))
    }

    /// Throw the segment away.
    pub fn discard(self) {
        drop(self.file);
        let _ = fs::remove_file(&self.tmp_path);
    }
}

/// Remove segments no longer referenced and stale work files.
pub fn cleanup(cache_dir: &Path, idx: &Index, dropped: &[String]) {
    for name in dropped {
        let _ = fs::remove_file(cache_dir.join(name));
    }
    let live: HashSet<&str> = idx.segments.iter().map(|s| s.name.as_str()).collect();
    let Ok(rd) = fs::read_dir(cache_dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let is_work = name.contains(".tmp");
        let is_seg = name.starts_with("seg-") && name.ends_with(".dat");
        if !(is_work || (is_seg && !live.contains(name.as_str()))) {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map_or(false, |age| age >= ORPHAN_MAX_AGE);
        if old {
            let _ = fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Index {
        Index {
            root: "/r".into(),
            updated_unix: 5,
            segments: vec![Segment { name: "seg-a.dat".into(), len: 10 }],
            entries: vec![
                Entry { rel: "a.txt".into(), depth: 0, size: 3, mtime_ns: -7, hash: [1; 32], state: State::Text, seg: 0, off: 0, len: 10 },
                Entry { rel: "日本/b.bin".into(), depth: 1, size: 9, mtime_ns: 2, hash: [0; 32], state: State::Binary, seg: 0, off: 0, len: 0 },
            ],
        }
    }

    #[test]
    fn roundtrip_and_corruption() {
        let idx = sample();
        let bytes = idx.serialize();
        let back = Index::deserialize(&bytes).unwrap();
        assert_eq!(back.entries.len(), 2);
        assert_eq!(back.entries[1].rel, "日本/b.bin");
        assert_eq!(back.entries[0].mtime_ns, -7);
        let mut bad = bytes.clone();
        bad[20] ^= 0xFF;
        assert!(Index::deserialize(&bad).is_err());
        assert!(Index::deserialize(&bytes[..bytes.len() - 5]).is_err());
        assert!(Index::deserialize(b"garbage").is_err());
    }

    #[test]
    fn segment_writer_rewind_and_finish() {
        let d = tempfile::tempdir().unwrap();
        let mut w = SegmentWriter::create(d.path()).unwrap();
        w.write(b"hello").unwrap();
        let mark = w.pos();
        w.write(b"discard me").unwrap();
        w.rewind(mark).unwrap();
        w.write(b" world").unwrap();
        let seg = w.finish().unwrap().unwrap();
        assert_eq!(seg.len, 11);
        assert_eq!(fs::read(d.path().join(&seg.name)).unwrap(), b"hello world");
        let empty = SegmentWriter::create(d.path()).unwrap();
        assert!(empty.finish().unwrap().is_none());
    }
}
