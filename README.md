# CTD — CLI Text Discovery

**Find the information you need from a large collection of local TXT files as quickly as possible.**

CTD is a fast, local, command-line full-text search tool for TXT files. Put your TXT files into a `txts/` folder, run a search command, and CTD finds the text across your whole collection almost instantly. It keeps its search index up to date automatically.

CTD is not a note-taking application or an editor. It never modifies or deletes your TXT files.

> **v1.1 is a complete redesign in Rust.** The Python memo manager (v1.0) is kept as the Git tag `v1.0` and the branch `v1.0-python`. v1.1 does not use v1.0 data.

## Features

- Literal, case-insensitive substring search (English, Japanese and any Unicode text), including file names and relative paths
- SQLite FTS5 trigram index — fast on large collections
- Automatic change detection at startup (new / modified / deleted files) with a single confirmation
- Single standalone executable per OS — no Rust, Python or database server needed
- Works fully offline; no background process

## Supported operating systems

- Windows x86_64
- Linux x86_64

## Installation

Download the executable for your OS and put it anywhere (for example into a folder on your `PATH`):

| OS | File |
|---|---|
| Windows | `ctd-v1.1.0-windows-x86_64.exe` (rename to `ctd.exe` if you like) |
| Linux | `ctd-v1.1.0-linux-x86_64` (`chmod +x`, rename to `ctd`) |

The workspace is **not** tied to the executable's location.

## Quick start

```text
mkdir my-notes
cd my-notes
ctd init                 # creates txts/ and .ctd/
# copy your .txt files into my-notes/txts/ (sub-folders are fine)
ctd search sqlite
```

## Directory structure

```text
my-notes/
├── txts/              your original TXT files (never modified by CTD)
│   ├── programming/rust.txt
│   └── ideas/project.txt
└── .ctd/
    └── index.db       the search index (disposable, can be rebuilt)
```

CTD finds the workspace by looking for `.ctd/` in the current directory and its parents, so commands work from any sub-folder.

## Commands

| Command | Description |
|---|---|
| `ctd init` | Initialize a workspace in the current directory |
| `ctd search <query>` | Search TXT files |
| `ctd sync` | Detect changes and update the index |
| `ctd sync --verify` | Re-hash every file (detects changes that keep size and timestamp) |
| `ctd rebuild` | Rebuild the entire index |
| `ctd status` | Show workspace and index status (does not scan) |
| `ctd help` / `ctd version` | Help / version |

Global options: `--yes` (update without asking), `--no-sync` (do not check for changes; takes precedence over `--yes`).

### Search examples

```text
ctd search SQLite                      # case-insensitive substring
ctd search "database performance"      # exact phrase (quote it)
ctd search 全文検索                     # Japanese
ctd search rust.txt                    # file name / path
ctd search SQLite --limit 50           # show up to 50 files (default 20)
ctd search SQLite --context 2          # 2 lines before/after each match
ctd search SQLite --no-sync            # skip the change check
```

The query is always a literal string — not a regular expression or a query language. Special characters (`*`, `"`, `-`, `'`, `%` …) are searched as typed. Each matching file appears once, with up to three matching lines. A file that matches only through its path shows `(path match)`.

Search results go to standard output; progress and messages go to standard error, so `ctd search foo > result.txt` captures just the results.

### Synchronization

CTD has no background watcher. Before a search (and for `ctd sync`) it compares `txts/` with the index using file size and modification time. Only changed files are read and hashed (BLAKE3). If something changed you see a summary and one question:

```text
Changes detected:

  New:       3
  Modified:  2
  Deleted:   1

Update index? [Y/n]
```

- Enter or `Y` updates the index; `N` skips (the index may then be outdated); Ctrl+C cancels (exit code 130).
- When standard input is not a terminal (pipes, scripts), CTD updates automatically. `--yes` always skips the question; `--no-sync` skips the check.
- The whole update is one database transaction: on any error or Ctrl+C the previous index is preserved.
- Files that are not valid UTF-8 (for example Shift_JIS) are reported on every run as `Warning: skipped …` and counted as *Skipped*. Their existing index entries are never replaced with partial content. Save such files as UTF-8 (UTF-8 with BOM is fine).
- Files in directories CTD cannot read are never mistaken for deleted files.

### Rebuild

If the index is missing or damaged, CTD tells you and offers a safe rebuild. You can also run `ctd rebuild` yourself at any time. The new index is built in a temporary file inside `.ctd/` and swapped in only when complete; your TXT files are never touched. Deleting `.ctd/` is also safe — run `ctd init` and `ctd sync` again.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Runtime or operational error |
| 2 | Invalid command or arguments |
| 130 | Cancelled with Ctrl+C |

## Building from source

Requires Rust (stable, 1.75 or newer) and a C compiler (SQLite is compiled into the executable).

```text
cargo test --release
cargo build --release          # target/release/ctd
cargo run --release --example bench -- --sizes 1000,10000,100000
```

Helper scripts:

- Linux: `scripts/build-linux.sh [build|test|bench]` → `dist/ctd-v1.1.0-linux-x86_64`
- Windows: `scripts\build-windows.bat [build|test|bench]` → `dist\ctd-v1.1.0-windows-x86_64.exe`.
  It downloads a portable Rust toolchain and MinGW-w64 into the project's `tools\` folder (no system-wide install; delete the folder to remove everything).

Benchmark results measured on real hardware are in `bench/`.

## Limitations

- Only `.txt` files in UTF-8 (with or without BOM) are indexed. Other encodings are reported and skipped, never guessed.
- Case-insensitive matching uses simple per-character Unicode case folding; accents are not ignored.
- Queries shorter than three characters cannot use the trigram index and are slower on very large collections.
- The index stores a copy of every file's text, so `.ctd/index.db` is several times the size of the TXT collection.

## License

MIT — see [LICENSE](LICENSE).
