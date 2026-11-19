## CTD benchmark

Hardware: CPU: Intel(R) Xeon(R) Processor @ 2.30GHz; logical CPUs: 1; RAM:        4088100 kB; OS: linux

Binary: /home/claude/ctd-rs/target/release/ctd


### 1000 files

Dataset: 1000 files, 3.6 MB of text; index database: 14.3 MB

| Measurement | Time |
|---|---|
| Full indexing (first `sync` on empty index) | 468.6 ms |
| Full indexing (`rebuild`) | 500.5 ms |
| First start on up-to-date index (OS cache dropped) | 43.6 ms |
| Start with up-to-date index (`sync`, median of 5) | 7.6 ms |
| `sync --verify` (hash all files) | 13.2 ms |
| Incremental: 10 new files | 16.2 ms |
| Incremental: 10 modified files | 27.9 ms |
| Incremental: 10 deleted files | 17.2 ms |
| `search database` incl. startup check (median of 5) | 9.8 ms |

Search time with `--no-sync` (process start included, median of 5):

| Query type | Query | Time |
|---|---|---|
| rare English word (~0.1% of files) | `rarequery7731` | 2.6 ms |
| common English word | `database` | 6.2 ms |
| Japanese word | `全文検索` | 4.7 ms |
| phrase with spaces | `database index` | 4.4 ms |
| path (file name) | `file_000042` | 2.2 ms |
| 2 characters (no trigram) | `検索` | 2.1 ms |
| 1 character (no trigram) | `z` | 21.7 ms |

### 10000 files

Dataset: 10000 files, 35.7 MB of text; index database: 128.0 MB

| Measurement | Time |
|---|---|
| Full indexing (first `sync` on empty index) | 5.17 s |
| Full indexing (`rebuild`) | 5.30 s |
| First start on up-to-date index (OS cache dropped) | 359.7 ms |
| Start with up-to-date index (`sync`, median of 5) | 51.7 ms |
| `sync --verify` (hash all files) | 112.5 ms |
| Incremental: 100 new files | 126.5 ms |
| Incremental: 100 modified files | 131.9 ms |
| Incremental: 100 deleted files | 111.4 ms |
| `search database` incl. startup check (median of 5) | 79.4 ms |

Search time with `--no-sync` (process start included, median of 5):

| Query type | Query | Time |
|---|---|---|
| rare English word (~0.1% of files) | `rarequery7731` | 3.6 ms |
| common English word | `database` | 43.6 ms |
| Japanese word | `全文検索` | 35.1 ms |
| phrase with spaces | `database index` | 19.5 ms |
| path (file name) | `file_000042` | 3.4 ms |
| 2 characters (no trigram) | `検索` | 2.2 ms |
| 1 character (no trigram) | `z` | 193.2 ms |

### 100000 files

Dataset: 100000 files, 357.0 MB of text; index database: 1275.3 MB

| Measurement | Time |
|---|---|
| Full indexing (first `sync` on empty index) | 60.28 s |
| Full indexing (`rebuild`) | 60.22 s |
| First start on up-to-date index (OS cache dropped) | 2.99 s |
| Start with up-to-date index (`sync`, median of 5) | 386.1 ms |
| `sync --verify` (hash all files) | 3.94 s |
| Incremental: 1000 new files | 1.04 s |
| Incremental: 1000 modified files | 1.40 s |
| Incremental: 1000 deleted files | 1.09 s |
| `search database` incl. startup check (median of 5) | 736.2 ms |

Search time with `--no-sync` (process start included, median of 5):

| Query type | Query | Time |
|---|---|---|
| rare English word (~0.1% of files) | `rarequery7731` | 8.3 ms |
| common English word | `database` | 376.5 ms |
| Japanese word | `全文検索` | 260.8 ms |
| phrase with spaces | `database index` | 166.4 ms |
| path (file name) | `file_000042` | 10.5 ms |
| 2 characters (no trigram) | `検索` | 3.0 ms |
| 1 character (no trigram) | `z` | 1.91 s |
