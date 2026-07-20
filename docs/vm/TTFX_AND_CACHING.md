# sjulia の起動レイテンシとキャッシュ戦略

この文書は、sjulia における Julia の TTFX（Time To First X）問題の扱い方と、それを回避・軽減するためのキャッシュ機構についてまとめたものです。

## 1. Julia の TTFX とは何か

公式 Julia では、関数が初めて呼ばれる時に以下が発生します：

- 型推論（inference）
- メソッドの特殊化（specialization）
- LLVM を使ったネイティブコード生成（codegen）

これが **TTFX**（特に「初回実行までの遅延」）の主な原因です。`using Plots` 後の初回 `plot(sin)` などで顕著になります。

## 2. sjulia はどう回避しているか

sjulia は **JIT コンパイラを持たない** 静的パイプラインです：

```text
Julia source → Parser → Lowering → Compiler → VM bytecode → VM execution
```

すべてのコード生成は実行前に完了します。実行時に LLVM codegen が走ることはありません。

### 2.1 sjulia のコンパイラは AOT bytecode compiler

sjulia のコンパイラは Julia ソース（正確には Core IR）を VM 用の bytecode（`Instr` の列）に変換します。これは **AOT（Ahead-Of-Time）コンパイラ** です。

- 実行時に新規 bytecode を生成することは基本的にありません
- ただし、後述の **runtime specialization** によって、一部のホットパスで実行時に最適化された bytecode を生成することがあります
- これは Julia スタイルの LLVM JIT とは異なります

### 2.2 静的パイプラインのメリット

| 観点 | 公式 Julia | sjulia |
|---|---|---|
| 実行時 codegen | あり（LLVM） | なし |
| 初回呼び出しコスト | 型推論 + LLVM codegen | 既存 bytecode の実行 |
| 起動レイテンシの主因 | JIT コンパイル | Base 読み込み・キャッシュデコード |

## 3. sjulia のキャッシュ機構

sjulia は複数の階層でキャッシュを使い、起動時の再コンパイルを回避します。

### 3.1 Base bytecode cache

- `build.sh` 時に `--precompile-base` で生成
- xcframework / バイナリに `SJULIA_BASE_CACHE` 経由で埋め込み
- 実行時に Base 関数の bytecode を読み込むだけ
- iOS では `build.sh` により自動的に埋め込まれる

### 3.2 Prelude Program cache

- prelude（前処理済みプログラム）を解析・ロワリング済みの状態で保存
- `SJULIA_PRELUDE_PROGRAM_CACHE` 経由で埋め込み
- Base cache と併せて iOS バイナリに含まれる

### 3.3 Package loader cache（`.ji.json`）

`subset_julia_vm/src/loader.rs` が管理する永続キャッシュです：

```text
$TMPDIR/subset_julia_vm_cache/<name>.<source-hash>.ji.json
```

- パッケージの lowered `Module` を保存
- 2 回目以降の `using X` で parse/lower をスキップ
- `CACHE_VERSION` と `schema_fingerprint` で stale 検出

### 3.4 Preloaded-package bytecode cache

`subset_julia_vm_compile/src/compile/preload_cache.rs` で管理。対象パッケージは
`SJULIA_PRELOAD_PACKAGES` から読み込まれ、`build.sh` は iOS サンプルの
`using` / `import` 行から自動検出します。

- ビルド時に検出されたパッケージと推移的依存を bytecode までコンパイル
- 実行時、プログラムの non-Base 関数レイアウトが cache と一致すれば bytecode を splice（接合）
- `using Plots; using LinearAlgebra` のコストを約 307ms → 53ms に削減
- iOS / WASM では `SJULIA_PRELOAD_CACHE` 経由でバイナリに埋め込み
- `SJULIA_PRELOAD_PACKAGES="PkgA,PkgB" ./build.sh` で検出結果を上書き可能

#### 制約

- プログラムの関数レイアウトが生成時と一致しない場合、gate が deactivate して通常コンパイルに fallback
- `surface(x, y, (x,y) -> ...)` のような「main lifted lambda」を含むプログラムは gate が外れ、約 266ms のコンパイルが発生（Issue #9189/#9254）

### 3.5 `.sjvmbc` ユーザースクリプトキャッシュ

- `sjulia --compile-vm script.jl -o script.sjvmbc` で生成
- `sjulia script.sjvmbc` で parse/lower/compile をスキップして実行
- 現状は手動運用
- 自動化は Issue #6349 で提案されている（未実装）

#### 計測例

| スクリプト | `.jl` 実行 | `.sjvmbc` 実行 | 速度向上 |
|---|---|---|---|
| 計算処理中心（約 170 行） | ~145–162ms | ~59–75ms | 約 2.4 倍 |
| 関数 1000 個 | ~119–140ms | ~28–43ms | 約 3.9 倍 |

`.sjvmbc` ファイルサイズは 15–17 MB 程度（`CompiledProgram` + 元の Core IR `Program` を含む）。

## 4. パッケージごとの違い

| パッケージ | PRELOAD_PACKAGES | `using` 時の挙動 |
|---|---|---|
| iOS サンプルで検出されたパッケージ | ✅ | レイアウトが一致すれば bytecode splice（速い） |
| 未検出・未指定のパッケージ | ❌ | その場で bytecode コンパイル |

Preload cache は関数レイアウト同一性に依存するため、検出された superset が
常に全サンプルへ効くわけではありません。一致しない場合は通常コンパイルへ
fail-safe に戻ります。特定サンプルの TTFX を優先する場合は
`SJULIA_PRELOAD_PACKAGES` でそのサンプルの `using` 順を指定します。

## 5. iOS アプリにおける考慮事項

iOS アプリ（`SubsetJuliaVMApp`）では：

- `VMBridge.execute()` → `compile_and_run_detailed()` が毎回ソースをコンパイル
- Base cache と preload cache は xcframework に埋め込まれている
- ユーザーコード（Hello world 含む）は毎回コンパイルされるため、100ms 程度かかる

### 改善策

1. **バンドルサンプルの `.sjvmbc` 化**（Issue #9945）
   - `build.sh` 時に `Samples/**/*.jl` を `.sjvmbc` にコンパイル
   - アプリバンドルに同梱
   - 実行時は `.sjvmbc` を直接読み込む

2. **ユーザー入力コードのアプリ内キャッシュ**
   - iOS の `Caches/` ディレクトリに `.sjvmbc` を保存
   - 2 回目以降の実行を高速化

## 6. Runtime Specialization と JIT の違い

sjulia には `CallSpecialize` などの **runtime specialization** があります。これは実行中に特定の型パターンに対して専用の bytecode を生成する仕組みです。

| | 一般的な JIT | sjulia の runtime specialization |
|---|---|---|
| 対象 | 任意のメソッド | 特定のホットパス・決まったパターン |
| 出力 | マシンコード or 一般 bytecode | 限定的な専用 bytecode |
| トリガー | メソッド初回呼び出し | 型が確定したループなど |
| スコープ | 言語全体 | 型特殊化の決まったケース |

結論：

- **runtime specialization は広義には「実行時コンパイル」の一種**
- しかし、sjulia が主張する「no JIT」とは「Julia スタイルの LLVM ベース JIT を持たない」という意味
- 「sjulia は JIT bytecode compiler を持っている」と言うのは誤解を招く

## 7. 他の言語との比較

| 言語/システム | 類似点 | 違い |
|---|---|---|
| **CPython** | source → bytecode → interpreter + cache | 言語が単純、多重ディスパッチなし |
| **Lua** | 軽量 bytecode VM、JIT なし | 型システムが単純 |
| **CRuby** | bytecode VM | 近年 JIT 化が進む |
| **GraalVM Native Image** | AOT、実行時 codegen なし | マシンコード出力 |
| **PackageCompiler.jl** | Julia の TTFX 対策 | ネイティブコード AOT |
| **MicroPython** | サブセット + bytecode VM | Python、マイコン向け |

sjulia は Julia の多重ディスパッチ・型システムを扱いつつ、JIT なしの bytecode VM で実行するという点で、他に直接的な先例が少ない独特な位置づけです。

## 8. FAQ

### Q: sjulia にも TTFX はあるか？

**A**: 狭義の JIT 起因の TTFX はありません。ただし、広義の「初回実行までのレイテンシ」は存在します。主な要因は：

- Base / prelude キャッシュのデコード
- ユーザースクリプトの parse / lower / compile
- 非 PRELOAD_PACKAGES のパッケージの bytecode 生成

### Q: `.sjvmbc` は Julia の `.ji` ファイルと同じか？

**A**: 似ていますが異なります。`.sjvmbc` は VM bytecode まで含み、`.ji` は主に型推論結果や lowered IR を保存します。sjulia でも package loader cache（`.ji.json`）は lowered `Module` を保存します。

### Q: `plot(sin)` 実行時に bytecode は生成されるか？

**A**: PRELOAD_PACKAGES のレイアウト gate が一致した場合は基本的に生成されません。`plot` の bytecode はビルド時に生成されており、実行時は VM がそれを解釈実行するだけです。

### Q: なぜ `using Symbolics` は `using Plots` より遅いか？

**A**: `Symbolics` が PRELOAD_PACKAGES に含まれていない、または含まれていてもレイアウト gate が一致しない場合、`using` 時にその場で bytecode を生成する必要があるからです。

## 関連ファイル

- `subset_julia_vm_compile/src/compile/preload_cache.rs`
- `subset_julia_vm/src/loader.rs`
- `subset_julia_vm_compile/src/compile/cache.rs`
- `subset_julia_vm_compile/src/compile/precompile.rs`
- `subset_julia_vm/src/vm_bytecode_file.rs`
- `subset_julia_vm/src/bin/sjulia.rs`
- `SubsetJuliaVMApp/SubsetJuliaVMApp/Services/FFI/VMBridge.swift`
- `build.sh`

## 関連 Issue

- #6349 — Transparent per-script `.sjvmbc` bytecode cache
- #9189 / #9245 / #9254 — Preloaded-package bytecode cache
- #7921 — Package loader cache invalidation
- #2929 — Precompiled Base cache for iOS
- #9945 — iOS: build `.sjvmbc` for bundled samples during `./build.sh`
