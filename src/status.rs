//! `ctd status`: show workspace and index information without scanning TXT files.

use crate::db::{self, IndexState};
use crate::workspace::Workspace;
use crate::VERSION;
use anyhow::Result;
use std::io::Write;

pub fn run<W: Write>(ws: &Workspace, out: &mut W) -> Result<()> {
    let state = db::inspect(&ws.db_path());
    let count = if state == IndexState::Ready {
        db::count_files(&db::open(&ws.db_path())?)?.to_string()
    } else {
        "-".to_string()
    };
    writeln!(out, "CTD v{VERSION}\n")?;
    writeln!(out, "Workspace:\n  {}\n", ws.root.display())?;
    writeln!(out, "TXT files indexed:\n  {count}\n")?;
    writeln!(out, "Index database:\n  .ctd/index.db\n")?;
    writeln!(out, "Index status:\n  {}", state.label())?;
    match &state {
        IndexState::Invalid(reason) => {
            writeln!(out, "  ({reason})\n  Run `ctd rebuild` to recreate the index.")?
        }
        IndexState::Missing => writeln!(out, "  Run `ctd rebuild` to create the index.")?,
        IndexState::Unsupported(v) => writeln!(
            out,
            "  (index schema version {v}; this CTD supports version {})",
            db::SCHEMA_VERSION
        )?,
        IndexState::Ready => {}
    }
    Ok(())
}
