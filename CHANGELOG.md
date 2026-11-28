# Changelog

## v1.2.1

Maintenance release. Searching, the command line and the index format are unchanged from v1.2.0; indexes built by v1.2.0 keep working.

### Fixed

- Test suite (Linux): the CLI integration tests failed intermittently with "Text file busy" (`ETXTBSY`) when run in parallel. A child process forked by another test thread could briefly hold the write handle of the freshly copied executable. The tests now repeat the start in that transient case.
- Test suite (Windows): the cache-location unit test checked Linux-style paths (`/xdg`), which Windows does not treat as absolute. The tests are now split per OS and build paths for the host system. The production code is unchanged: Windows keeps using `%LOCALAPPDATA%`, Linux prefers `$XDG_CACHE_HOME`.

### Changed

- The source is formatted with rustfmt (`cargo fmt --all -- --check` passes).
- Version 1.2.1; release files are named `ctd-v1.2.1-windows-x86_64.exe` and `ctd-v1.2.1-linux-x86_64`.
- v1.2.0 is not modified and stays available.

## v1.2.0

CTD v1.2 is a redesign around "put the executable next to your text and search".

### Changed

- **No workspace, no `init`.** The search root is the folder that contains the executable. Move the executable and the new location becomes the search root.
- **Any text file is searchable**, not only `.txt`: `.md`, `.log`, `.json`, `.csv`, source code, files without an extension, …. Binary files are skipped.
- **Command line:** `ctd <terms>` searches. Several terms are combined with AND; a term may match the path or the text. `ctd -s <terms>` removes the 8-level depth limit. Other commands: `status`, `rebuild`, `help`, `version`.
- **No SQLite.** CTD uses its own index: a metadata file plus immutable text segments, stored in the OS user cache directory (never in the searched folder).
- **No confirmation prompt, no `--yes` / `--no-sync`.** The index is brought up to date automatically at the start of every search.
- Results are sorted by relative path; at most 20 files are shown, the rest is counted.
- Large files are processed as streams; memory use does not depend on file size.

### Safety

- Original files are never modified, moved or deleted, and nothing is created in the searched folder.
- Symbolic links / junctions are not followed. System areas (Windows: `%SystemRoot%`, `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%ProgramData%`, `$Recycle.Bin`, `System Volume Information`; Linux: `/proc`, `/sys`, `/dev`, `/run`) are never searched, also with `-s`.
- A filesystem root (`C:\`, `/`) or a system area is refused as search root.
- An interrupted or failed update (Ctrl+C, crash) keeps the previous index.

### Removed

- `init`, `sync`, `--yes`, `--no-sync`, `--limit`, `--context` and the `.ctd/` workspace.

## v1.1.0

Rust rewrite with a workspace (`txts/`, `.ctd/index.db`), SQLite FTS5 trigram search, `init` / `search` / `sync` / `rebuild` / `status`.

## v1.0

Python memo manager (kept as tag `v1.0` and branch `v1.0-python`).
