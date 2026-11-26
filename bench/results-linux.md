## CTD benchmark

Hardware: CPU: Intel(R) Xeon(R) Processor @ 2.30GHz; logical CPUs: 1; RAM:        4088088 kB; OS: linux

Binary: /home/claude/ctd12/target/release/ctd


### 1000 files

Dataset: 1000 files, 3.6 MB of text; index (cache directory): 3.7 MB

| Measurement | Time |
|---|---|
| First search on a new root (builds the full index) | 42.0 ms |
| `rebuild` | 41.4 ms |
| First search on an up-to-date index (OS cache dropped) | 12.1 ms |
| Search on an up-to-date index, rare word (median of 5) | 6.2 ms |
| `status` (no scan, median of 5) | 1.1 ms |
| Incremental: 10 new files (search incl. update) | 8.1 ms |
| Incremental: 10 modified files | 8.5 ms |
| Incremental: 10 deleted files | 8.9 ms |

Search time on an up-to-date index (process start and change check included, median of 5):

| Query type | Query | Time |
|---|---|---|
| rare English word (~0.1% of files) | `rarequery7731` | 6.2 ms |
| common English word | `database` | 6.6 ms |
| Japanese word | `全文検索` | 6.6 ms |
| two terms (AND), both common | `database index` | 8.0 ms |
| three terms (AND) | `database index 全文検索` | 6.6 ms |
| path (file name) | `file_000042` | 6.4 ms |
| 1 character | `z` | 6.0 ms |
| unlimited depth (-s), rare word | `-s rarequery7731` | 6.3 ms |

### 10000 files

Dataset: 10000 files, 35.7 MB of text; index (cache directory): 36.7 MB

| Measurement | Time |
|---|---|
| First search on a new root (builds the full index) | 350.8 ms |
| `rebuild` | 359.2 ms |
| First search on an up-to-date index (OS cache dropped) | 52.9 ms |
| Search on an up-to-date index, rare word (median of 5) | 34.5 ms |
| `status` (no scan, median of 5) | 2.3 ms |
| Incremental: 100 new files (search incl. update) | 43.3 ms |
| Incremental: 100 modified files | 43.8 ms |
| Incremental: 100 deleted files | 39.7 ms |

Search time on an up-to-date index (process start and change check included, median of 5):

| Query type | Query | Time |
|---|---|---|
| rare English word (~0.1% of files) | `rarequery7731` | 37.2 ms |
| common English word | `database` | 35.6 ms |
| Japanese word | `全文検索` | 35.3 ms |
| two terms (AND), both common | `database index` | 35.3 ms |
| three terms (AND) | `database index 全文検索` | 36.0 ms |
| path (file name) | `file_000042` | 37.0 ms |
| 1 character | `z` | 36.1 ms |
| unlimited depth (-s), rare word | `-s rarequery7731` | 35.4 ms |

### 100000 files

Dataset: 100000 files, 357.0 MB of text; index (cache directory): 367.2 MB

| Measurement | Time |
|---|---|
| First search on a new root (builds the full index) | 3.70 s |
| `rebuild` | 3.43 s |
| First search on an up-to-date index (OS cache dropped) | 734.1 ms |
| Search on an up-to-date index, rare word (median of 5) | 313.4 ms |
| `status` (no scan, median of 5) | 17.0 ms |
| Incremental: 1000 new files (search incl. update) | 381.7 ms |
| Incremental: 1000 modified files | 380.6 ms |
| Incremental: 1000 deleted files | 345.6 ms |

Search time on an up-to-date index (process start and change check included, median of 5):

| Query type | Query | Time |
|---|---|---|
| rare English word (~0.1% of files) | `rarequery7731` | 319.6 ms |
| common English word | `database` | 326.7 ms |
| Japanese word | `全文検索` | 417.8 ms |
| two terms (AND), both common | `database index` | 405.9 ms |
| three terms (AND) | `database index 全文検索` | 380.9 ms |
| path (file name) | `file_000042` | 375.2 ms |
| 1 character | `z` | 366.8 ms |
| unlimited depth (-s), rare word | `-s rarequery7731` | 384.9 ms |
