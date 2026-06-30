# AoT (Ahead-of-Time) コンパイル

**最終更新**: 2026-06-24

SubsetJuliaVM の AoT (Ahead-of-Time) コンパイラは、Julia（サブセット）を **Rust ソースコードにトランスパイル**し、`rustc` でネイティブ実行するための仕組みです。

AoT は `subset_julia_vm` クレートの **`aot` feature** 配下で提供され、ランタイム依存が必要なケース向けに `subset_julia_vm_runtime` クレートが同梱されています。

## 概要（現状のパイプライン）

現行の AoT CLI（`subset_julia_vm/src/bin/aot.rs`）は、主に **Core IR を入力として**以下の流れで Rust を生成します（`.sjir` も最終的には Program へロードして同様に処理します）。

```text
Julia source (.jl) ─┐
                    ├─→ Parser → Lowering → Core IR (Program)
Core IR (.sjir) ───┘               ↓
                         Dead Code Elimination (CallGraph)
                                   ↓
                           Type Inference (AoT)
                                   ↓
                              AoT IR (AotProgram)
                                   ↓
                           Optimizer (AoT passes)
                                   ↓
                          Rust codegen (AotCodeGenerator)
```

## 使い方（CLI）

### ビルド

```bash
cargo build -p subset_julia_vm --release --features aot --bin juliars
```

### Julia ソースから Rust を生成

```bash
cargo run -p subset_julia_vm --bin juliars --features aot -- \
  path/to/program.jl \
  -o /tmp/program.rs \
  --stats
```

### 標準入力から読み、標準出力へ書く

```bash
cat path/to/program.jl | \
  cargo run -p subset_julia_vm --bin juliars --features aot -- \
    - -o -
```

### 1行コードから Rust を生成

```bash
cargo run -p subset_julia_vm --bin juliars --features aot -- \
  -e "1 + 2" \
  -o /tmp/eval.rs
```

### Core IR (`.sjir`) から Rust を生成

```bash
cargo run -p subset_julia_vm --bin juliars --features aot -- \
  --ir path/to/program.sjir \
  -o /tmp/program.rs
```

### 生成された Rust をビルドして実行

AoT の出力は `subset_julia_vm_runtime` に依存することがあるため、まずは helper script 経由の **Cargo project link**
を標準手順として使ってください:

```bash
scripts/juliars_build_generated.sh /tmp/program.rs /tmp/program
/tmp/program
```

この helper は一時 Cargo project を作り、`subset_julia_vm_runtime` を workspace path dependency として link します。
`rustc --extern ... -L ...` を手で同期する必要はありません。

低レベルの手動確認が必要な場合だけ、runtime rlib を明示して直接 `rustc` を呼びます:

```bash
cargo build --release -p subset_julia_vm_runtime
rustc -O /tmp/program.rs -o /tmp/program \
  --extern subset_julia_vm_runtime="target/release/libsubset_julia_vm_runtime.rlib" \
  -L "target/release/deps"
/tmp/program
```

生成された Rust が **本当にスタンドアロン** で、`subset_julia_vm_runtime` を参照していない場合だけ、次の簡易形でもビルドできます:

```bash
rustc -O /tmp/program.rs -o /tmp/program
/tmp/program
```

### よく使うオプション

- `-O0` / `-O1` / `-O2` / `-O3` or `--opt-level N`: AoT IR 最適化レベルを指定（デフォルト `-O2`）
- `--stats`: コンパイル統計、生成 Rust LOC、推定出力サイズ、残存動的ディスパッチ箇所を表示
- `--check`: Rust を書き出さず、AoT 非対応/動的ディスパッチ箇所を検査
- `--time-passes`: DCE・推論・IR 変換・最適化・codegen の所要時間を表示
- `--emit-binary <path>`: 生成 Rust を一時 Cargo project で build し、native binary を `<path>` に出力（`-o` 併用時は Rust source も保存）
- `--target <triple>`: `--emit-binary` の Cargo build に target triple を渡す（target は事前に Rust toolchain へ追加してください）
- `--export-c-abi <symbol>` / `--export-c-abi <symbol=function>` / `--export-c-abi <symbol=function(Int64,Float64)>`: `#[no_mangle] extern "C"` entry を生成。top-level comma 区切りで複数 entry を一括指定可能。現状は `Int8/16/32/64`, `UInt8/16/32/64`, `Float32/64`, `Bool`, `Nothing` return の C-stable scalar signature のみ
- `--diagnostic-format human|json`: エラー診断を通常表示または JSON で出力
- `--color auto|always|never`: human 診断の色付けを制御
- `--comments`: 生成コードにデバッグコメントを付与
- `--minimal-prelude`: AoT 向けの最小 Prelude を使用（出力を小さくしやすい）
- `--pure-rust`: **完全にスタンドアロンな Rust を要求**します。動的ディスパッチやランタイム依存が残る場合はエラーにする想定のオプションです
- `--backend rust|cranelift`: codegen backend 指定。`rust` が現行の実装済み経路で、`cranelift` は `cranelift` feature build で有効になる実験的 backend です。Cranelift backend は scalar / native stack aggregate subset の `--check`、opt-in `--jit-run`、`--emit-object`、`--emit-binary`、`--emit-library` に対応します。runtime `Value` / GC / rooting が必要な型や呼び出し形は contract 実装が接続されるまで diagnostic gate します。

`--target` は `--emit-binary` の helper build 用です。Rust source だけを生成する場合は `-o output.rs` を使い、任意の手元 toolchain
でその source を build してください。

`--export-c-abi` は Swift/iOS など native host から直接呼ぶための entry symbol を出力します。AoT IR 上で distinct overload method が
存在する Julia 関数は `sjulia_add_i64=add(Int64,Int64)` のように Julia 関数名 + 引数型で自動解決できます。generated method 名（例: `add_i64_i64`）を
指定する既存形式や、`sjulia_add=add_i64_i64` の alias も引き続き利用できます。`String`/配列/struct
など Rust-native だが C ABI として安定でない型、または `Any` / multi-variant `Union` など runtime `Value` boundary が必要な型は codegen
error として拒否します。これらの将来 ABI は [ABI_AND_NUMERIC_CONTRACTS.md](./ABI_AND_NUMERIC_CONTRACTS.md) の borrowed view / out-param / opaque handle contract に従います。

### 終了コード

- `0`: 成功
- `2`: CLI 使用法エラー（入力の同時指定など）
- `3`: I/O エラー
- `4`: parse/lowering エラー
- `5`: AoT 非対応機能
- `6`: codegen エラー

## バックエンド

- **Rust codegen**: `subset_julia_vm/src/aot/codegen/aot_codegen/`（Rustソースを生成 → `rustc`）
- **Cranelift**: `subset_julia_vm/src/aot/codegen/cranelift/`（`cranelift` feature、実験的）

## 対応サブセット / 既知の制限

詳細な機能別マトリクスは [SUPPORT_MATRIX.md](./SUPPORT_MATRIX.md) を参照してください。Cranelift backend 固有の
対応/制限/ロードマップは [CRANELIFT_SUPPORT_MATRIX.md](./CRANELIFT_SUPPORT_MATRIX.md) に分離しています。要約すると、現行の安定経路は
`juliars --backend rust` で、スカラー演算、単純な関数、分岐/ループ、1D 配列、タプル、基本的な struct/Complex
の一部、文字列 literal / `*` concat / `string(...)` concat を Rust source へ落とします。

`Any` / `Union` / 動的 dispatch / runtime `Value` が必要な箇所は生成 Rust が `subset_julia_vm_runtime` に依存します。
完全に standalone な Rust が必要な場合は `--pure-rust` を使い、失敗時の dynamic site と residual runtime symbol を
確認してください。

Varargs / splatting、broadcast fusion、first-class function、do-block、closure、`try` /
`catch` / `finally` は、[CALL_CONTROL_FLOW_CONTRACTS.md](./CALL_CONTROL_FLOW_CONTRACTS.md)
の contract に従って static native path と runtime helper boundary を分けます。
静的に Julia semantics を保てない箇所は、helper 接続まで span 付き diagnostic gate として扱います。

Cranelift backend は現時点では設計・実験用です。`juliars --backend cranelift` は
`cranelift` feature build で既存 Cranelift generator へ到達しますが、対応範囲は
scalar / straight-line subset に限定されます。

## ドキュメント構成（このディレクトリ）

| ファイル | 役割 |
|---|---|
| `README.md` | 概要・使い方（このファイル） |
| `DESIGN.md` | 設計メモ（現行実装に合わせて更新） |
| `SUPPORT_MATRIX.md` | AoT 対応サブセット / 制限の機能別マトリクス |
| `CALL_CONTROL_FLOW_CONTRACTS.md` | varargs / broadcast / first-class functions / closures / exceptions の AoT contract |
| `CRANELIFT_SUPPORT_MATRIX.md` | Cranelift backend 固有の対応範囲 / gate / milestone roadmap |
| `IMPLEMENTATION_GUIDE.md` | 実装/デバッグ用の開発者ガイド |
| `IMPLEMENTATION_PLAN.md` | ロードマップ（現状に合わせて更新） |
| `PHASE1_FOUNDATION.md` | 初期計画（必要に応じて “歴史的資料” として整理） |

## 関連

- `benchmarks/scripts/run_benchmarks.sh`: AoT / VM / Julia の比較（AoT 生成・`rustc` 時間・サイズ計測も含む）
- `subset_julia_vm/src/bin/aot.rs`: `juliars` CLI 実装（オプション仕様の一次情報）
