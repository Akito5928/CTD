//! Command line interface and dispatch.

use crate::error::{EXIT_ERROR, EXIT_INTERRUPTED, EXIT_OK};
use crate::search::{self, SearchOptions};
use crate::sync::{self, SyncOptions, SyncOutcome, TerminalPrompter};
use crate::workspace::{self, Workspace};
use crate::{interrupt, output, status, VERSION};
use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use std::io::{self, BufWriter, Write};

#[derive(Parser, Debug)]
#[command(
    name = "ctd",
    about = "CTD - fast local full-text search for TXT files",
    disable_version_flag = true,
    arg_required_else_help = true
)]
struct Cli {
    /// Update the index without asking for confirmation
    #[arg(long, global = true)]
    yes: bool,

    /// Skip the startup synchronization check and use the existing index
    #[arg(long = "no-sync", global = true)]
    no_sync: bool,

    /// Print version
    #[arg(short = 'V', long = "version", global = true)]
    version: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize a workspace in the current directory
    Init,
    /// Search TXT files (literal, case-insensitive substring)
    Search {
        /// Text to search for (a phrase with spaces must be quoted)
        #[arg(allow_hyphen_values = true, value_parser = non_empty)]
        query: String,
        /// Maximum number of files to show
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..))]
        limit: u32,
        /// Lines of context before and after each match
        #[arg(long, default_value_t = 0)]
        context: u32,
    },
    /// Detect TXT changes and update the index
    Sync {
        /// Hash every file to detect changes that keep size and timestamp
        #[arg(long)]
        verify: bool,
    },
    /// Rebuild the entire search index
    Rebuild,
    /// Show workspace and index status
    Status,
    /// Show version
    Version,
}

fn non_empty(s: &str) -> std::result::Result<String, String> {
    if s.is_empty() {
        Err("the search query must not be empty".to_string())
    } else {
        Ok(s.to_string())
    }
}

pub fn main_entry() -> i32 {
    output::init_console();
    let cli = Cli::parse(); // exits with code 2 on usage errors, 0 for --help
    interrupt::install();
    match execute(cli) {
        Ok(()) => EXIT_OK,
        Err(e) => report_error(&e),
    }
}

fn report_error(e: &anyhow::Error) -> i32 {
    if sync::is_interrupted_error(e) {
        eprintln!("\nInterrupted. No changes were committed.");
        return EXIT_INTERRUPTED;
    }
    for cause in e.chain() {
        if let Some(io_err) = cause.downcast_ref::<io::Error>() {
            if io_err.kind() == io::ErrorKind::BrokenPipe {
                return EXIT_OK;
            }
        }
    }
    eprintln!("Error: {e:#}");
    EXIT_ERROR
}

fn execute(cli: Cli) -> Result<()> {
    if cli.version {
        println!("CTD v{VERSION}");
        return Ok(());
    }
    let Some(command) = cli.command else {
        let _ = Cli::command().print_help();
        std::process::exit(crate::error::EXIT_USAGE);
    };
    let opts = SyncOptions {
        verify: false,
        yes: cli.yes,
        no_sync: cli.no_sync,
    };
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    match command {
        Command::Version => writeln!(out, "CTD v{VERSION}")?,
        Command::Init => {
            let cwd = std::env::current_dir().context("cannot determine the current directory")?;
            let (ws, outcome) = workspace::init(&cwd)?;
            if outcome.created {
                writeln!(out, "Initialized CTD workspace:\n  {}", ws.root.display())?;
                writeln!(out, "\nPut your TXT files into txts/ and run `ctd search <query>`.")?;
            } else {
                writeln!(out, "CTD workspace is already initialized:\n  {}", ws.root.display())?;
            }
        }
        Command::Status => {
            let ws = Workspace::find()?;
            status::run(&ws, &mut out)?;
        }
        Command::Rebuild => {
            let ws = Workspace::find()?;
            let stats = sync::rebuild(&ws)?;
            eprintln!("\nIndex rebuilt successfully.\n");
            eprintln!("  Indexed: {}", stats.added);
            if stats.skipped > 0 {
                eprintln!("  Skipped: {}", stats.skipped);
            }
        }
        Command::Sync { verify } => {
            let ws = Workspace::find()?;
            let opts = SyncOptions { verify, ..opts };
            if opts.no_sync {
                eprintln!("Synchronization skipped (--no-sync).");
                return Ok(());
            }
            let _ = sync::prepare(&ws, &opts, &mut TerminalPrompter)?;
        }
        Command::Search {
            query,
            limit,
            context,
        } => {
            let ws = Workspace::find()?;
            let (conn, outcome) = sync::prepare(&ws, &opts, &mut TerminalPrompter)?;
            eprintln!("Searching...");
            let sopts = SearchOptions {
                limit: limit as usize,
                context: context as usize,
            };
            let res = search::search(&conn, &query, &sopts)?;
            if res.hits.is_empty() {
                writeln!(out, "No results found for \"{query}\".")?;
            } else {
                output::print_results(&mut out, &res, sopts.context)?;
                out.flush()?;
                let n = res.hits.len();
                let noun = if n == 1 { "file" } else { "files" };
                if res.more {
                    eprintln!("{n} {noun} shown (more files match; use --limit to show more).");
                } else {
                    eprintln!("{n} {noun} matched.");
                }
            }
            if outcome == SyncOutcome::Declined {
                eprintln!("Note: the index update was skipped, so results may be outdated.");
            }
        }
    }
    out.flush()?;
    Ok(())
}
