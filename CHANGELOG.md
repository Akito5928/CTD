# Changelog

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
