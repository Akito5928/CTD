# CTD (CLI Text Database) v1.0

TXTファイルをSQLiteに登録・管理・検索する個人向けCLIツール(Windows / `ctd.exe`)。
仕様は「CTD v1.0 正式仕様書」+「Implementation Decisions Addendum」に従います。

## 使い方

```
ctd init                          初期化(設定ファイル・フォルダ・DBを作成)
ctd register <file>               TXTファイルを登録(対話式)
ctd search <keyword>              検索(タイトル・本文・説明 / 日本語対応)
ctd list                          一覧(新しい順、20件ずつ)
ctd show <id>                     詳細表示
ctd edit <id>                     編集(Enterで変更なし)
ctd delete <id>                   削除(確認あり)
ctd category list|add|rename      カテゴリ管理
ctd backup                        DBバックアップ
ctd help / ctd version
```

- `<id>` はUUIDの先頭8文字以上でOK(一意に決まらない場合は候補を表示)。
- editでは、説明・タグは `-` で削除/全解除、カテゴリは `0` で未設定にできます。
- タグの区切りは `,` `，` `、` のどれでも使えます。
- 終了コード: 0=正常(キャンセル含む) / 1=実行時エラー / 2=引数・使用方法エラー。

## フォルダ構成

`ctd.exe` のあるフォルダが基準です。`ctd init` で次が作られます。

```
CTD/
├── ctd.exe
├── config/config.json     (database_path / backup_dir。相対パスは基準フォルダから)
├── database/ctd.db
├── backup/
└── txt/
```

## ビルド・開発

Pythonは「普通にインストール済み(3.11以上)」でも「未インストール」でも使えます。
どちらの場合も、作業用の環境は **このフォルダ内の `.venv` / `.local`** だけに作られ、PCのPython環境や
レジストリは変更しません(フォルダごと削除すれば全て消えます)。

**Pythonを入れたくない場合(推奨)**

1. https://github.com/astral-sh/uv/releases から `uv-x86_64-pc-windows-msvc.zip` をダウンロードして展開し、
   中の `uv.exe` だけを `tools\` フォルダにコピーします。
2. `run_tests.bat`(テスト)→ `build_exe.bat`(`dist\ctd.exe` を作成)の順に実行します。
   初回だけPython 3.12を自動でダウンロードします(インターネットが必要)。

**Pythonがインストール済みの場合**: `tools\uv.exe` を置かずに、そのまま上の2つのバッチを実行します。

```
run_tests.bat                     # テスト実行(pytest。一時DBのみ使用、実データには触れない)
build_exe.bat                     # dist\ctd.exe を作成(PyInstaller)
python ctd_entry.py <command>     # 開発時の実行(基準フォルダ = このプロジェクトのルート)
```

完成した `dist\ctd.exe` は好きな場所にコピーして使えます。

`assets\ctd.ico` があれば、`build_exe.bat` が自動で `ctd.exe` のアイコンとして使います(差し替えるときは同名の `.ico` で置き換えてください)。

- `CTD_DEBUG=1` を設定すると、予期しないエラー時にスタックトレースも表示します。
- 全文検索はFTS5のtrigramを使い、3文字未満の検索(と、trigramが使えない環境)は通常検索(LIKE)になります。

## 構成

```
src/ctd/
├── main.py            コマンドの振り分け・共通エラー処理
├── app.py / config.py 実行コンテキスト・設定
├── commands/          init register search list_notes show edit delete category backup
├── database/          connection schema repository backup
└── utils/             file_reader hash validator timeutil console
tests/                 pytestによる自動テスト
```
