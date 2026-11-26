# CTD — fast local text search

**Put the executable next to your text files, type a word, find it.**

CTD searches every text file in the folder that contains the executable (and its sub-folders) and shows the matching files with the matching line. There is nothing to initialize or configure. It works fully offline and never changes your files.

> v1.2 is a redesign of v1.1 (which used a `txts/` workspace and SQLite). v1.1 is kept as the tag `v1.1` and the branch `v1.1-rust`; the Python memo manager v1.0 as the tag `v1.0` and branch `v1.0-python`.

## Quick start

1. Download the executable for your OS from the [Releases](../../releases) page (`ctd-v1.2.0-windows-x86_64.exe` or `ctd-v1.2.0-linux-x86_64`).
2. Put it into the folder whose text you want to search (rename it to `ctd` / `ctd.exe` if you like).
3. Open a terminal in any folder and run it by its path, or put that folder on your `PATH`:

```text
ctd hello
ctd programming language          # both words must match (AND)
ctd -s programming                # no directory depth limit
```

The **search root is the folder that contains the executable** — not the current directory. If you move the executable, the new folder becomes the search root (and gets its own index).

## Commands

| Command | Description |
|---|---|
| `ctd <terms>` | Search (several terms are combined with AND) |
| `ctd -s <terms>` | Search without the 8-level depth limit |
| `ctd status` | Show search root, cache location and index information (no scanning) |
| `ctd rebuild` | Discard the index and scan everything again |
| `ctd help` / `ctd --help` | Help |
| `ctd version` / `ctd --version` | Version |

`status`, `rebuild`, `help` and `version` are commands only when they are the **only** argument. `ctd status report` searches for both words; to search for the word itself use `ctd -- status`.

## Search rules

- Literal, case-insensitive substring search (English, Japanese, full-width characters, any Unicode text). No regular expressions, no fuzzy search.
- Terms are split at whitespace, including the full-width space. Quotes do not make a phrase.
- Every term must occur in the file's relative path **or** its text (different terms may match in different places).
- Results are sorted by relative path. At most 20 files are shown; the rest is counted (`... and N more files.`).
- Each file is shown once with a representative line (the line containing the most distinct terms, otherwise the first one), `(+N more lines)` when there are more, or `(path match)` when only the path matched.

Output: **results go to standard output**; the banner, root, progress, counts and warnings go to standard error, so `ctd foo > result.txt` captures just the results.

## Which files are searched

- Any file that can be read as text, whatever its extension (or none).
- Binary files (a NUL byte in the first 8 KB) are skipped silently.
- UTF-8 and UTF-8 with BOM are supported. Other encodings (Shift_JIS, UTF-16, …) are skipped with a warning the first time they are read — save such files as UTF-8.
- Sub-folders up to **8 levels** below the root are searched (files in the root are level 0). `-s` removes this limit only.
- Never searched, even with `-s`: symbolic links and junctions (not followed), and system areas — Windows: `%SystemRoot%`, `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%ProgramData%`, `$Recycle.Bin`, `System Volume Information`; Linux: `/proc`, `/sys`, `/dev`, `/run`. Hidden folders such as `.git` are **not** excluded.
- CTD refuses to run when its folder is a filesystem root (`C:\`, `/`) or inside a system area.

Be careful where you put the executable: everything readable below it (up to 8 levels) is indexed on the first search.

## The index

- Stored in the OS user cache directory, **never** in the searched folder: `%LOCALAPPDATA%\ctd\v1\<id>\` on Windows, `$XDG_CACHE_HOME/ctd/v1/<id>/` (or `~/.cache/ctd/v1/<id>/`) on Linux. `ctd status` shows the exact path.
- Built automatically on the first search and updated incrementally at the start of every search: new files are added, changed files (size / timestamp, confirmed with a BLAKE3 hash) are re-read, deleted files are removed, unchanged files are reused.
- Files that cannot be read are skipped with a warning and never mistaken for deleted ones; an incomplete scan never removes anything.
- Updates are written to temporary files and committed atomically. If a run is interrupted (Ctrl+C, crash, power loss) the previous index stays valid; a damaged index is detected and rebuilt automatically.
- Old caches of folders you no longer use are not deleted automatically — remove the `ctd` folder in the cache directory to reclaim the space.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (also when nothing matched) |
| 1 | Runtime error |
| 2 | Invalid arguments (including no arguments or an empty search) |
| 130 | Cancelled with Ctrl+C |

## Building from source

Requires Rust (stable, 1.75 or newer). No C compiler or database is needed.

```text
cargo test --release
cargo build --release          # target/release/ctd (ctd.exe on Windows)
cargo run --release --example bench -- --sizes 1000,10000,100000
```

- Linux: `scripts/build-linux.sh [build|test|bench]` → `dist/ctd-v1.2.0-linux-x86_64`
- CI: `.github/workflows/build.yml` builds and tests Windows and Linux on every push; pushing a tag such as `v1.2` publishes a GitHub Release with both executables and SHA-256 checksums.

Benchmark results measured on real hardware are in `bench/`.

## Limitations

- Case-insensitive matching uses simple per-character Unicode case folding; accents and full-width/half-width forms are different characters.
- The index keeps a (case-folded) copy of every text file, so the cache is about as large as the text itself.
- Showing the representative line re-reads the original file; for files of several hundred MB with tens of millions of lines this takes a few seconds.
- Two `ctd` processes updating the same index at the same moment may disturb each other; the next run recovers automatically.

## License

MIT — see [LICENSE](LICENSE).
