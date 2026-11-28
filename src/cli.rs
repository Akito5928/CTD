//! Command line interface and dispatch.

use crate::config::{Config, Excludes};
use crate::error::{CtdError, EXIT_ERROR, EXIT_INTERRUPTED, EXIT_OK, EXIT_USAGE};
use crate::search::{self, MAX_FILES};
use crate::store::State;
use crate::update::{self, UpdateOptions, UpdateStats};
use crate::walk::DEFAULT_MAX_DEPTH;
use crate::{interrupt, output, status, VERSION};
use anyhow::Result;
use clap::{CommandFactory, Parser};
use std::io::{self, BufWriter, Write};

#[derive(Parser, Debug)]
#[command(
    name = "ctd",
    about = "CTD - fast local text search. Searches the folder that contains this executable.",
    override_usage = "ctd [-s] <search terms>...\n       ctd status | rebuild | help | version",
    after_help = "Several terms are combined with AND; matching ignores case.\n\
                  Use `ctd -- status` to search for the word \"status\" itself.",
    disable_version_flag = true,
    disable_help_subcommand = true
)]
struct Cli {
    /// Search without the directory depth limit (default: 8 levels)
    #[arg(short = 's')]
    unlimited: bool,

    /// Print version
    #[arg(short = 'V', long = "version")]
    version: bool,

    /// Search terms (all must match, in the path or the text)
    #[arg(value_name = "TERM")]
    terms: Vec<String>,
}

pub fn main_entry() -> i32 {
    output::init_console();
    let raw: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    if raw.is_empty() {
        eprintln!("{}", Cli::command().render_help());
        return EXIT_USAGE;
    }
    interrupt::install();
    let result = if raw.len() == 1 {
        match raw[0].as_str() {
            "status" => cmd_status(),
            "rebuild" => cmd_rebuild(),
            "version" => {
                println!("CTD v{VERSION}");
                Ok(())
            }
            "help" => {
                println!("{}", Cli::command().render_help());
                Ok(())
            }
            _ => run_cli(),
        }
    } else {
        run_cli()
    };
    match result {
        Ok(()) => EXIT_OK,
        Err(e) => report_error(&e),
    }
}

fn run_cli() -> Result<()> {
    let cli = Cli::parse(); // exits with code 2 on usage errors, 0 for --help
    if cli.version {
        println!("CTD v{VERSION}");
        return Ok(());
    }
    let terms = search::parse_terms(&cli.terms)?;
    let cfg = Config::from_environment()?;
    let ex = Excludes::system();
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    do_search(&cfg, &ex, &terms, cli.unlimited, &mut out)?;
    out.flush()?;
    Ok(())
}

fn cmd_status() -> Result<()> {
    let cfg = Config::from_environment()?;
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    status::run(&cfg, &mut out)?;
    out.flush()?;
    Ok(())
}

fn cmd_rebuild() -> Result<()> {
    let cfg = Config::from_environment()?;
    do_rebuild(&cfg, &Excludes::system())
}

fn report_error(e: &anyhow::Error) -> i32 {
    match e.downcast_ref::<CtdError>() {
        Some(CtdError::Interrupted) => {
            eprintln!("\nInterrupted. The previous index was kept.");
            return EXIT_INTERRUPTED;
        }
        Some(CtdError::Usage(m)) => {
            eprintln!("Error: {m}");
            return EXIT_USAGE;
        }
        None => {}
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

fn report_update(stats: &UpdateStats) {
    if let Some(n) = &stats.notice {
        eprintln!("{n}");
    }
    if stats.changed() {
        eprintln!(
            "Index updated: {} new, {} modified, {} removed.",
            stats.added, stats.modified, stats.removed
        );
    }
}

/// Search (after bringing the index up to date). Results go to `out`;
/// messages go to standard error.
pub fn do_search<W: Write>(
    cfg: &Config,
    ex: &Excludes,
    terms: &[String],
    unlimited: bool,
    out: &mut W,
) -> Result<()> {
    ex.check_root(&cfg.root)?;
    eprintln!("CTD v{VERSION}\n");
    eprintln!("Searching: \"{}\"", terms.join(" "));
    eprintln!("Root: {}", output::display_root(&cfg.root));
    let max_depth = if unlimited {
        None
    } else {
        Some(DEFAULT_MAX_DEPTH)
    };
    if unlimited {
        eprintln!("Searching with unlimited directory depth...");
    }
    eprintln!();

    let (idx, stats) = update::update(
        cfg,
        ex,
        &UpdateOptions {
            max_depth,
            rebuild: false,
        },
    )?;
    report_update(&stats);

    let found = search::find(&cfg.root, &cfg.cache_dir, &idx, terms, max_depth, MAX_FILES)?;
    if found.total == 0 {
        writeln!(out, "No matching files found.")?;
        return Ok(());
    }
    let noun = if found.total == 1 { "file" } else { "files" };
    eprintln!("Found {} {noun}.\n", found.total);
    for (i, hit) in found.hits.iter().enumerate() {
        interrupt::check()?;
        let d = search::describe(&hit.abs, terms);
        output::print_hit(out, i + 1, &hit.rel, &d)?;
    }
    if found.total > found.hits.len() {
        eprintln!("... and {} more files.", found.total - found.hits.len());
    }
    Ok(())
}

/// Discard the stored index and scan the search root again.
pub fn do_rebuild(cfg: &Config, ex: &Excludes) -> Result<()> {
    ex.check_root(&cfg.root)?;
    eprintln!("CTD v{VERSION}\n");
    eprintln!("Root: {}", output::display_root(&cfg.root));
    eprintln!("Rebuilding the index...\n");
    let (idx, stats) = update::update(
        cfg,
        ex,
        &UpdateOptions {
            max_depth: Some(DEFAULT_MAX_DEPTH),
            rebuild: true,
        },
    )?;
    let text = idx
        .entries
        .iter()
        .filter(|e| e.state == State::Text)
        .count();
    eprintln!(
        "Index rebuilt: {} files scanned, {text} indexed.",
        stats.scanned
    );
    Ok(())
}
