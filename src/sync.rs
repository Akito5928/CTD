//! Index synchronization: change detection, confirmation and transactional update.

use crate::db::{self, IndexState, IndexedMeta};
use crate::error::CtdError;
use crate::scan::{self};
use crate::textio;
use crate::workspace::Workspace;
use crate::interrupt;
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// Decoded content of pending files is kept in memory up to this budget;
/// beyond it, files are re-read when the update is applied.
const CONTENT_CACHE_BUDGET: usize = 256 * 1024 * 1024;
/// Per-file progress lines are printed for all files up to this many changes.
const FULL_PROGRESS_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    /// Hash every file, even if size and timestamp are unchanged.
    pub verify: bool,
    /// Skip the confirmation prompt.
    pub yes: bool,
    /// Skip synchronization entirely (takes precedence over `yes`).
    pub no_sync: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
}

/// Abstraction over the terminal so tests can drive the prompts.
pub trait Prompter {
    fn interactive(&self) -> bool;
    fn ask_yes_no(&mut self, question: &str) -> Result<Answer>;
}

pub struct TerminalPrompter;

impl Prompter for TerminalPrompter {
    fn interactive(&self) -> bool {
        io::stdin().is_terminal()
    }

    fn ask_yes_no(&mut self, question: &str) -> Result<Answer> {
        loop {
            eprint!("{question} [Y/n] ");
            let _ = io::stderr().flush();
            interrupt::set_prompting(true);
            let mut line = String::new();
            let n = io::stdin().lock().read_line(&mut line);
            interrupt::set_prompting(false);
            let n = n.context("cannot read from standard input")?;
            if n == 0 {
                eprintln!();
                return Ok(Answer::No); // EOF: do not change anything
            }
            match parse_answer(&line) {
                Some(a) => return Ok(a),
                None => eprintln!("Please answer Y or N."),
            }
        }
    }
}

/// Parse a Y/n answer. Enter (empty input) means YES.
pub fn parse_answer(line: &str) -> Option<Answer> {
    match line.trim().to_lowercase().as_str() {
        "" | "y" | "yes" => Some(Answer::Yes),
        "n" | "no" => Some(Answer::No),
        _ => None,
    }
}

#[derive(Debug)]
pub struct PlannedFile {
    pub rel: String,
    pub abs: PathBuf,
    pub hash: String,
    pub size: i64,
    pub mtime_ns: i64,
    pub content: Option<String>,
}

#[derive(Debug, Default)]
pub struct Plan {
    pub scanned: usize,
    pub added: Vec<PlannedFile>,
    pub modified: Vec<PlannedFile>,
    pub deleted: Vec<String>,
    /// Same content, different size/timestamp: metadata is refreshed silently.
    pub meta_only: Vec<(String, i64, i64)>,
    pub skipped: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

impl Plan {
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.modified.is_empty() || !self.deleted.is_empty()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ApplyStats {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub skipped: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SyncOutcome {
    UpToDate,
    Updated(ApplyStats),
    /// The user answered N; the index may be outdated.
    Declined,
    /// `--no-sync`: nothing was checked.
    NotChecked,
    /// The index was missing/invalid and has been rebuilt.
    Rebuilt(ApplyStats),
}

/// Compare the TXT files on disk with the index.
pub fn plan(ws: &Workspace, conn: &Connection, verify: bool) -> Result<Plan> {
    let scan = scan::scan(&ws.txts_dir())?;
    let meta: HashMap<String, IndexedMeta> = db::load_meta(conn)?;
    let mut plan = Plan {
        scanned: scan.files.len(),
        warnings: scan.warnings.clone(),
        ..Default::default()
    };
    let mut cached = 0usize;
    let present: HashSet<&str> = scan.files.iter().map(|f| f.rel.as_str()).collect();

    for f in &scan.files {
        interrupt::check()?;
        let existing = meta.get(&f.rel);
        let candidate = match existing {
            None => true,
            Some(m) => verify || m.size != f.size as i64 || m.mtime_ns != f.mtime_ns,
        };
        if !candidate {
            continue;
        }
        let data = match scan::read_stable(&f.abs) {
            Ok(d) => d,
            Err(e) => {
                plan.skipped.push((f.rel.clone(), e));
                continue;
            }
        };
        let hash = textio::hash_hex(&data.bytes);
        if let Some(m) = existing {
            if m.hash == hash {
                if data.size != m.size || data.mtime_ns != m.mtime_ns {
                    plan.meta_only.push((f.rel.clone(), data.size, data.mtime_ns));
                }
                continue;
            }
        }
        let text = match textio::decode(data.bytes) {
            Ok(t) => t,
            Err(e) => {
                plan.skipped.push((f.rel.clone(), e));
                continue;
            }
        };
        let keep = cached + text.len() <= CONTENT_CACHE_BUDGET;
        if keep {
            cached += text.len();
        }
        let pf = PlannedFile {
            rel: f.rel.clone(),
            abs: f.abs.clone(),
            hash,
            size: data.size,
            mtime_ns: data.mtime_ns,
            content: if keep { Some(text) } else { None },
        };
        if existing.is_some() {
            plan.modified.push(pf);
        } else {
            plan.added.push(pf);
        }
    }

    for path in meta.keys() {
        if present.contains(path.as_str()) || scan.is_uncertain(path) {
            continue;
        }
        plan.deleted.push(path.clone());
    }
    plan.deleted.sort();
    Ok(plan)
}

fn apply_meta_only(conn: &mut Connection, items: &[(String, i64, i64)]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "UPDATE files SET file_size = ?2, modified_time_ns = ?3 WHERE relative_path = ?1",
        )?;
        for (rel, size, mtime) in items {
            stmt.execute(params![rel, size, mtime])?;
        }
    }
    tx.commit()?;
    Ok(())
}

struct Loaded {
    hash: String,
    size: i64,
    mtime_ns: i64,
    text: String,
}

fn load_file(path: &Path) -> std::result::Result<Loaded, String> {
    let data = scan::read_stable(path)?;
    let hash = textio::hash_hex(&data.bytes);
    let text = textio::decode(data.bytes)?;
    Ok(Loaded {
        hash,
        size: data.size,
        mtime_ns: data.mtime_ns,
        text,
    })
}

const UPSERT_SQL: &str = "INSERT INTO files (relative_path, file_size, modified_time_ns, content_hash, content)
    VALUES (?1, ?2, ?3, ?4, ?5)
    ON CONFLICT(relative_path) DO UPDATE SET
        file_size = excluded.file_size,
        modified_time_ns = excluded.modified_time_ns,
        content_hash = excluded.content_hash,
        content = excluded.content";

/// Apply a plan in ONE transaction. On any error (or Ctrl+C) everything is
/// rolled back and the previous index stays intact.
pub fn apply(conn: &mut Connection, mut plan: Plan) -> Result<ApplyStats> {
    let total = plan.added.len() + plan.modified.len() + plan.deleted.len();
    let mut stats = ApplyStats::default();
    let mut i = 0usize;
    let tx = conn
        .transaction()
        .context("cannot start a database transaction")?;
    {
        let mut upsert = tx.prepare(UPSERT_SQL)?;
        let mut delete = tx.prepare("DELETE FROM files WHERE relative_path = ?1")?;

        for (label, list, is_add) in [
            ("Added", std::mem::take(&mut plan.added), true),
            ("Updated", std::mem::take(&mut plan.modified), false),
        ] {
            for mut f in list {
                interrupt::check()?;
                i += 1;
                let loaded = match f.content.take() {
                    Some(text) => Loaded {
                        hash: f.hash.clone(),
                        size: f.size,
                        mtime_ns: f.mtime_ns,
                        text,
                    },
                    None => match load_file(&f.abs) {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("Warning: skipped {}: {e}", f.rel);
                            stats.skipped += 1;
                            continue;
                        }
                    },
                };
                upsert
                    .execute(params![
                        f.rel,
                        loaded.size,
                        loaded.mtime_ns,
                        loaded.hash,
                        loaded.text
                    ])
                    .with_context(|| format!("database error while indexing {}", f.rel))?;
                if is_add {
                    stats.added += 1;
                } else {
                    stats.updated += 1;
                }
                progress(i, total, label, &f.rel);
            }
        }
        for rel in &plan.deleted {
            interrupt::check()?;
            i += 1;
            delete
                .execute(params![rel])
                .with_context(|| format!("database error while removing {rel}"))?;
            stats.removed += 1;
            progress(i, total, "Removed", rel);
        }
    }
    tx.commit()
        .context("cannot commit the index update (the previous index was kept)")?;
    Ok(stats)
}

fn progress(i: usize, total: usize, label: &str, rel: &str) {
    if total <= FULL_PROGRESS_LIMIT || i % 100 == 0 || i == total {
        eprintln!("[{i}/{total}] {label}: {rel}");
    }
}

fn print_apply_summary(stats: &ApplyStats, headline: &str) {
    eprintln!("\n{headline}\n");
    eprintln!("  Added:   {}", stats.added);
    eprintln!("  Updated: {}", stats.updated);
    eprintln!("  Removed: {}", stats.removed);
    if stats.skipped > 0 {
        eprintln!("  Skipped: {}", stats.skipped);
    }
}

/// Detect changes and (after confirmation) apply them to an existing index.
pub fn synchronize(
    ws: &Workspace,
    conn: &mut Connection,
    opts: &SyncOptions,
    prompter: &mut dyn Prompter,
) -> Result<SyncOutcome> {
    let plan = plan(ws, conn, opts.verify)?;
    for w in &plan.warnings {
        eprintln!("Warning: {w}");
    }
    for (path, reason) in &plan.skipped {
        eprintln!("Warning: skipped {path}: {reason}");
    }
    apply_meta_only(conn, &plan.meta_only)?;

    if !plan.has_changes() {
        eprintln!("Index is up to date.");
        return Ok(SyncOutcome::UpToDate);
    }

    eprintln!("Checking txts...");
    eprintln!("  Scanned: {} files\n", plan.scanned);
    eprintln!("Changes detected:\n");
    eprintln!("  New:       {}", plan.added.len());
    eprintln!("  Modified:  {}", plan.modified.len());
    eprintln!("  Deleted:   {}", plan.deleted.len());
    if !plan.skipped.is_empty() {
        eprintln!("  Skipped:   {}", plan.skipped.len());
    }
    eprintln!();

    let go = if opts.yes || !prompter.interactive() {
        true
    } else {
        prompter.ask_yes_no("Update index?")? == Answer::Yes
    };
    if !go {
        eprintln!("Index update skipped. Search results may be outdated.");
        return Ok(SyncOutcome::Declined);
    }

    eprintln!("Updating index...\n");
    let pre_skipped = plan.skipped.len();
    let mut stats = apply(conn, plan)?;
    stats.skipped += pre_skipped;
    print_apply_summary(&stats, "Index updated successfully.");
    Ok(SyncOutcome::Updated(stats))
}

/// Open a ready-to-use index, recovering or synchronizing as needed.
pub fn prepare(
    ws: &Workspace,
    opts: &SyncOptions,
    prompter: &mut dyn Prompter,
) -> Result<(Connection, SyncOutcome)> {
    let db_path = ws.db_path();
    match db::inspect(&db_path) {
        IndexState::Ready => {}
        IndexState::Unsupported(v) => bail!(
            "the index database uses schema version {v}, but this CTD supports version {}.\n  \
             Use a newer CTD, or run `ctd rebuild` to replace the index (TXT files are never changed).",
            db::SCHEMA_VERSION
        ),
        state @ (IndexState::Missing | IndexState::Invalid(_)) => {
            let reason = match &state {
                IndexState::Invalid(r) => format!("is invalid ({r})"),
                _ => "is missing".to_string(),
            };
            eprintln!("The search index {reason} and needs to be rebuilt.");
            eprintln!("(Your TXT files are not affected.)");
            if opts.no_sync {
                bail!("cannot search without an index; run `ctd rebuild` first");
            }
            let go = if opts.yes || !prompter.interactive() {
                true
            } else {
                prompter.ask_yes_no("Rebuild the index now?")? == Answer::Yes
            };
            if !go {
                bail!("cannot continue without an index; run `ctd rebuild`");
            }
            let stats = rebuild(ws)?;
            let conn = db::open(&db_path)?;
            return Ok((conn, SyncOutcome::Rebuilt(stats)));
        }
    }
    let mut conn = db::open(&db_path)?;
    if opts.no_sync {
        return Ok((conn, SyncOutcome::NotChecked));
    }
    let outcome = synchronize(ws, &mut conn, opts, prompter)?;
    Ok((conn, outcome))
}

fn cleanup_tmp(tmp: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let mut name = tmp.as_os_str().to_owned();
        name.push(suffix);
        let _ = fs::remove_file(PathBuf::from(name));
    }
}

/// Rebuild the whole index into a temporary database, then swap it in.
/// On failure or interruption the existing database is left untouched.
pub fn rebuild(ws: &Workspace) -> Result<ApplyStats> {
    fs::create_dir_all(ws.ctd_dir())?;
    let tmp = ws.ctd_dir().join("index.db.rebuild-tmp");
    cleanup_tmp(&tmp);
    match rebuild_into(ws, &tmp) {
        Ok(stats) => {
            if let Err(e) = fs::rename(&tmp, ws.db_path()) {
                cleanup_tmp(&tmp);
                return Err(anyhow!(e)).context(
                    "cannot replace the index database (is another ctd process using it?); the old index was kept",
                );
            }
            Ok(stats)
        }
        Err(e) => {
            cleanup_tmp(&tmp);
            Err(e)
        }
    }
}

fn rebuild_into(ws: &Workspace, tmp: &Path) -> Result<ApplyStats> {
    let scan = scan::scan(&ws.txts_dir())?;
    for w in &scan.warnings {
        eprintln!("Warning: {w}");
    }
    eprintln!("Rebuilding index ({} files)...", scan.files.len());
    let total = scan.files.len();
    let mut stats = ApplyStats::default();
    {
        let mut conn = Connection::open(tmp)
            .with_context(|| format!("cannot create {}", tmp.display()))?;
        // The temporary database is discarded on failure, so durability
        // settings are relaxed here; it is fsynced before being swapped in.
        conn.execute_batch("PRAGMA journal_mode = OFF; PRAGMA synchronous = OFF; PRAGMA cache_size = -65536;")?;
        db::create_schema(&conn).context("cannot initialize the database schema")?;
        let tx = conn.transaction()?;
        {
            let mut upsert = tx.prepare(UPSERT_SQL)?;
            for (i, f) in scan.files.iter().enumerate() {
                interrupt::check()?;
                match load_file(&f.abs) {
                    Ok(l) => {
                        upsert
                            .execute(params![f.rel, l.size, l.mtime_ns, l.hash, l.text])
                            .with_context(|| format!("database error while indexing {}", f.rel))?;
                        stats.added += 1;
                    }
                    Err(e) => {
                        eprintln!("Warning: skipped {}: {e}", f.rel);
                        stats.skipped += 1;
                    }
                }
                if total > FULL_PROGRESS_LIMIT && (i + 1) % 5000 == 0 {
                    eprintln!("  {}/{} files", i + 1, total);
                }
            }
        }
        tx.commit()?;
    }
    // Flush the finished temporary database to disk before it replaces the old one.
    fs::OpenOptions::new()
        .write(true)
        .open(tmp)
        .and_then(|f| f.sync_all())
        .context("cannot flush the new index to disk")?;
    Ok(stats)
}

pub fn is_interrupted_error(e: &anyhow::Error) -> bool {
    matches!(e.downcast_ref::<CtdError>(), Some(CtdError::Interrupted))
}
