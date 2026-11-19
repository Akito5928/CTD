//! Workspace discovery and initialization.

use crate::db;
use crate::error::CtdError;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
}

pub struct InitOutcome {
    /// `true` when the database was newly created.
    pub created: bool,
}

impl Workspace {
    /// Find the closest workspace from the current directory upwards.
    pub fn find() -> Result<Self> {
        let cwd = std::env::current_dir().context("cannot determine the current directory")?;
        Self::find_from(&cwd)
    }

    pub fn find_from(start: &Path) -> Result<Self> {
        for dir in start.ancestors() {
            if dir.join(".ctd").is_dir() {
                return Ok(Workspace {
                    root: dir.to_path_buf(),
                });
            }
        }
        Err(CtdError::NoWorkspace.into())
    }

    pub fn txts_dir(&self) -> PathBuf {
        self.root.join("txts")
    }

    pub fn ctd_dir(&self) -> PathBuf {
        self.root.join(".ctd")
    }

    pub fn db_path(&self) -> PathBuf {
        self.ctd_dir().join("index.db")
    }
}

/// Initialize a workspace in `dir`. Never overwrites an existing database.
pub fn init(dir: &Path) -> Result<(Workspace, InitOutcome)> {
    let ws = Workspace {
        root: dir.to_path_buf(),
    };
    for sub in [ws.txts_dir(), ws.ctd_dir()] {
        if sub.exists() && !sub.is_dir() {
            bail!("{} exists but is not a directory", sub.display());
        }
        fs::create_dir_all(&sub).with_context(|| format!("cannot create {}", sub.display()))?;
    }
    if ws.db_path().exists() {
        return Ok((ws, InitOutcome { created: false }));
    }
    db::create_new(&ws.db_path())?;
    Ok((ws, InitOutcome { created: true }))
}
