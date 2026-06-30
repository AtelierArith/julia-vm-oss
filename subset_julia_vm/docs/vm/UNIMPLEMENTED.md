# SubsetJuliaVM 未実装機能一覧

**最終更新:** 2026-02-11

このドキュメントはSubsetJuliaVMで未実装の機能を整理したものです。Julia本家では動作するがSubsetJuliaVMでは動作しない構文・機能を記載しています。

---

## 構文 (Syntax)

### 未実装の構文

| 構文 | 説明 | エラータイプ | 優先度 |
|------|------|--------------|--------|
| `baremodule` (意味論) | baremodule で Base を自動インポートしない動作 | 構文は対応済み、意味論が未完全 | 低 |
| `A \ b` | 左除算演算子 | 部分実装 | 中 |
| `try ... catch ... else ... end` | try-else 構文 | `UnsupportedFeature` | 中 |
| `try ... catch ... finally ... end` | try-finally 構文 | `UnsupportedFeature` | 中 |

### 実装済みの構文 (以前は未実装だったもの)

以下は実装が完了し、テスト済みです：

| 構文 | 実装時期 | テスト |
|------|----------|--------|
| `[x^2 for x in arr]` 配列内包表記 | 実装済み | `array/comprehension.jl` |
| `[i*j for i in 1:3, j in 1:4]` 多変数内包表記 | Issue #2143 | `array/multi_comprehension.jl` |
| `arr[1:5]` 配列スライス | 実装済み | `array/slice_*.jl` |
| `arr[end]`, `arr[2:end]` end キーワード | Issue #2310 | `array/begin_indexing.jl` |
| `arr[begin]`, `m[begin, end]` begin キーワード | Issue #2349 | `array/begin_end_multidim.jl` |
| `module MyModule ... end` モジュール定義 | 実装済み | `module/*.jl` |
| `baremodule` (構文) | 実装済み | `module/baremodule_basic.jl` |
| `import Base: map` import 文 | 実装済み | `module/*.jl` |
| `using Module: f as g` エイリアス付きインポート | 実装済み | テスト済み |
| `r"pattern"` 正規表現リテラル | 実装済み | `regex/*.jl` |
| `export foo, bar` エクスポート宣言 | 実装済み | `module/*.jl` |

### 部分実装の構文

| 構文 | 現状 | 制限事項 |
|------|------|----------|
| `where T<:Real` | 型変数サポート | 複雑な制約は限定的 |
| 多次元配列リテラル | 1D/2D/3D 対応 | 4D以上は未テスト |

---

## 型 (Types)

### 未実装の型

| 型 | 説明 | 代替手段 |
|----|------|----------|
| `Task` | 並行処理タスク | なし |
| `Channel` | 通信チャネル | なし |
| `SubString` | 部分文字列参照 | `String` を使用 |
| `SubArray` | 配列ビュー | コピーを使用 |
| `BitVector` | ビット配列 | `Vector{Bool}` を使用 |
| `SparseArray` | 疎行列 | 密行列を使用 |

### 実装済みの型 (以前は未実装だったもの)

| 型 | 実装時期 | テスト |
|----|----------|--------|
| `Regex` | 実装済み | `regex/*.jl` |
| `RegexMatch` | 実装済み | `regex/match_field_access.jl` |
| `IOBuffer` | 実装済み | `io/*.jl` |
| `IOStream` (ファイルハンドル) | 実装済み | `filesystem/*.jl` |

### 型の実装状況

| 型 | 現状 | 備考 |
|----|------|------|
| `Array{T,N}` | N=1,2,3 完全サポート | N≥4 は未テスト |
| `Union{T1,T2}` | 完全サポート | 型推論・ディスパッチ対応 |
| `NamedTuple` | 完全サポート | 構築、フィールドアクセス、typeof、keys/values/pairs 対応 |
| `Complex{T}` | 完全サポート | Phase 1-4 完了。Pure Julia 実装 |
| `Rational{T}` | 完全サポート | Pure Julia 実装、75+ テスト |
| `Set` | 完全サポート | Phase 4-3, 4-4 で Pure Julia 化 |
| `Dict` | 完全サポート | Pure Julia + Rust intrinsics |

---

## 組み込み関数 (Built-in Functions)

### 未実装の関数

#### 配列操作
| 関数 | 説明 | Issue |
|------|------|-------|
| `view(arr, ...)` | 配列ビュー作成 | - |
| `selectdim(arr, dim, i)` | 次元選択 | - |
| `permutedims(A)` | 次元入れ替え | - |

#### システム関数
| 関数 | 説明 | Issue |
|------|------|-------|
| `@async` / `@spawn` | 非同期実行 | Task未実装 |
| `fetch(task)` | Task結果取得 | Task未実装 |
| `wait(task)` | Task完了待機 | Task未実装 |
| `Threads.nthreads()` | スレッド数 | スレッド未実装 |

### 実装済みの関数 (以前は未実装だったもの)

#### 配列操作
| 関数 | テスト |
|------|--------|
| `hcat(a, b)` / `vcat(a, b)` | `array/*.jl` |
| `eachrow(A)` / `eachcol(A)` | `iterators/*.jl` |

#### 文字列操作
| 関数 | テスト |
|------|--------|
| `lpad(str, n)` / `rpad(str, n)` | `strings/lpad_basic.jl`, `strings/rpad_basic.jl` |
| `match(r"...", str)` | `regex/match.jl` |
| `eachmatch(r"...", str)` | `regex/eachmatch.jl` |
| `occursin(needle, haystack)` | `strings/string_occursin_basic.jl` |
| `replace(str, regex => sub)` | `strings/strings_regex_replace.jl` |

#### ファイルI/O
| 関数 | テスト |
|------|--------|
| `open(filename, [mode])` | `filesystem/*.jl` |
| `read(filename, String)` | `filesystem/file_io.jl` |
| `readline(filename)` / `readlines(filename)` | `filesystem/countlines_readline.jl` |
| `readdir(path)` | `filesystem/pwd_readdir.jl` |
| `mkdir(path)` / `mkpath(path)` | `filesystem/mkdir_rm.jl` |
| `rm(path)` / `touch(path)` | `filesystem/mkdir_rm.jl` |
| `cp(src, dst)` / `mv(src, dst)` | `filesystem/cp_mv_mtime.jl` |
| `isfile(path)` / `isdir(path)` / `ispath(path)` | `filesystem/file_io.jl` |
| `pwd()` / `cd(path)` | `filesystem/pwd_readdir.jl` |
| `tempdir()` / `tempname()` | `filesystem/mkdir_rm.jl` |
| `filesize(path)` / `mtime(path)` | `filesystem/cp_mv_mtime.jl` |

#### 未実装のファイルI/O
| 関数 | 理由 |
|------|------|
| `chmod`, `chown`, `filemode` | iOS サンドボックス制限 |
| `download` | ネットワーク操作 |
| `symlink`, `hardlink` | iOS 制限 |
| `walkdir` | 未実装 |
| `stat`, `lstat` | 未実装 |
| `mktemp`, `mktempdir` | 未実装 |

---

## 標準ライブラリ (Standard Library)

### 未実装モジュール

| モジュール | 説明 | 優先度 |
|------------|------|--------|
| `Pkg` | パッケージマネージャ | 対象外 |
| `Distributed` | 分散処理 | 対象外 |
| `SharedArrays` | 共有配列 | 対象外 |
| `REPL` | REPLユーティリティ | 低 |
| `Sockets` | ネットワーク | 対象外 |
| `Logging` | ログ機能 | 低 |
| `UUIDs` | UUID生成 | 低 |
| `DelimitedFiles` | CSV等の読み書き | 低 |
| `TOML` | TOML パース | 低 |
| `JSON` | JSON パース | 低（外部パッケージ） |
| `SparseArrays` | 疎行列 | 低 |

### 実装済みモジュール

| モジュール | 実装状況 | 備考 |
|------------|----------|------|
| `Base` | 90%以上 | 主要機能は実装済み。Pure Julia 移行進行中 |
| `Statistics` | 完全 | var, std, mean, median, cor, cov, quantile |
| `LinearAlgebra` | 80% | lu, qr, svd, eigvals, cholesky, rank, cond, det, inv |
| `Random` | 基本 | rand, randn |
| `Test` | 完全 | @test, @testset, @test_throws |
| `Dates` | 基本 | Date, DateTime |
| `InteractiveUtils` | 部分 | @code_lowered など |
| `Printf` | 基本 | @printf, @sprintf |
| `Iterators` | ほぼ完全 | enumerate, zip, take, drop, countfrom, flatten, cycle, product 等 |
| `Broadcast` | 完全 | Pure Julia 実装。BroadcastStyle, materialize, @. マクロ |
| `Base64` | 基本 | base64encode, base64decode |

---

## コンパイラ/ランタイム機能

### 未実装の機能

| 機能 | 説明 | 理由 |
|------|------|------|
| JIT コンパイル | 実行時コード生成 | iOS App Store 禁止 |
| ガベージコレクション | 自動メモリ管理 | Rust メモリ管理を使用 |
| スレッド/並行処理 | マルチスレッド | 未実装 |
| コルーチン/Task | 協調マルチタスク | 未実装 |
| プリコンパイル | .jso キャッシュ | 不要 |
| パッケージシステム | Pkg 管理 | 静的埋め込みのみ |
| 末尾呼び出し最適化 | TCO | 未実装 |
| デッドコード削除 | DCE | 未実装 |

### 実装済みの機能

| 機能 | 実装状況 | 備考 |
|------|----------|------|
| 多重ディスパッチ | 完全 | 型ベースメソッド選択、Vararg{T,N} 対応 |
| 型推論 | 完全 | コンパイル時最適化、Effects System (Phase 1) |
| ブロードキャスト | 完全 | Pure Julia 実装 (Phase 1-7)。@. マクロ、.&&/.|| 対応 |
| マクロ展開 | ほぼ完全 | 一部制限あり（下記参照） |
| 例外処理 | 基本 | try-catch（finally/else は未実装） |
| スタックトレース | 完全 | ソースレベルのエラー報告 |
| 高階関数 | 完全 | map, filter, reduce, foldl/foldr, mapreduce, any, all, count 等 |
| イテレータ | 完全 | enumerate, zip, take, drop, countfrom, flatten, cycle 等 |
| Callable struct | 完全 | `(::Type)(args) = body` 構文 (Issue #2671) |
| AoT コンパイル | 基本 | Cranelift JIT バックエンド、enum 対応 |

---

## 既知の制限事項

### マクロパラメータの評価タイミング問題

**Issue:** マクロパラメータが quote ブロック外で評価されてしまう

```julia
# Julia本家では動作するが、SubsetJuliaVMでは失敗
macro test_throws(T, ex)
    expr_str = string(ex)  # ← ここで ex が評価されてしまう
    quote
        # ...
    end
end
```

**回避策:** パラメータ操作はすべて quote 内部で行う

**詳細:** `docs/vm/STATUS.md` の「マクロパラメータが quote 外で評価される問題」を参照

### I/O の制限

- ファイルI/O は iOS サンドボックスにより一部制限あり（権限操作、シンボリックリンク等）
- `print`/`println` は内部バッファへの出力のみ
- ネットワーク操作は未実装

### try-catch の制限

- `try ... catch ... finally ... end` は未サポート (Issue #317)
- `try ... catch ... else ... end` は未サポート (Issue #317)

---

## 優先度の定義

| 優先度 | 説明 |
|--------|------|
| 高 | ユーザーコードで頻繁に使用される。早期実装が必要 |
| 中 | 便利だが回避策あり。計画的に実装 |
| 低 | 特殊用途。必要に応じて実装 |
| 対象外 | iOS制限等により実装不可または不要 |

---

## 関連ドキュメント

- `docs/vm/STATUS.md` - 最近の実装と技術的知見
- `docs/vm/TESTING_CHECKLIST.md` - テスト手順
- `CLAUDE.md` - 開発ガイドライン
