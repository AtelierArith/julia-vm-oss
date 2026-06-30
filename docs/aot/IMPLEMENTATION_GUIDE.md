# AoT 実装ガイド（開発者向け）

**最終更新**: 2026-01-17

このドキュメントは、AoT を “新規に設計する” ための擬似コードではなく、**現行コードを前提にどこを読んで・どこを直すか**のガイドです。

## 入口（まずここ）

- **AoT CLI**: `subset_julia_vm/src/bin/aot.rs`
  - 入力（`.jl` / `-e` / `.sjir`）→ Prelude 合成 → DCE → 推論 → AoT IR → 最適化 → Rust 出力
  - オプション: `--stats`, `--comments`, `--pure-rust`, `--minimal-prelude`
- **AoT モジュール**: `subset_julia_vm/src/aot/`
- **ランタイム**: `subset_julia_vm_runtime/src/`

## コード配置（対応表）

| 目的 | 主なファイル |
|---|---|
| DCE（到達可能関数だけ残す） | `subset_julia_vm/src/aot/call_graph.rs` |
| AoT 型 | `subset_julia_vm/src/aot/types.rs`（`JuliaType`, `StaticType`） |
| 型推論 | `subset_julia_vm/src/aot/inference.rs` |
| Core IR → AoT IR 変換 | `subset_julia_vm/src/aot/analyze.rs`（`program_to_aot_ir`） |
| AoT IR 定義 | `subset_julia_vm/src/aot/ir.rs`（`AotProgram` / `AotExpr` / `AotStmt` など） |
| 最適化（AoT パス） | `subset_julia_vm/src/aot/optimizer.rs` |
| Rust codegen | `subset_julia_vm/src/aot/codegen/rust.rs`（主に `AotCodeGenerator`） |
| Cranelift backend（実験） | `subset_julia_vm/src/aot/codegen/cranelift.rs`（feature=`cranelift`） |
| 動的型/ヘルパ | `subset_julia_vm_runtime/src/value.rs`, `dispatch.rs`, `intrinsics.rs`, `error.rs` |

## 開発時の基本コマンド

### AoT CLI をビルド

```bash
cargo build -p subset_julia_vm --features aot --bin aot
```

### Rust を生成して確認（コメント + 統計）

```bash
cargo run -p subset_julia_vm --bin aot --features aot -- \
  path/to/program.jl \
  -o /tmp/out.rs \
  --comments \
  --stats
```

### “完全静的に落とせるか” を強制（診断用途）

```bash
cargo run -p subset_julia_vm --bin aot --features aot -- \
  path/to/program.jl \
  -o /tmp/out.rs \
  --pure-rust \
  --minimal-prelude
```

`--pure-rust` が失敗する場合、エラーメッセージに “動的操作” の診断が含まれるため、対応すべき箇所の特定に役立ちます。

## よくある作業別の当たりどころ

### 1) 変換エラー（Unsupported / ConversionError / InvalidIR）

- **Core IR → AoT IR** の変換は `aot/analyze.rs` が中心です。
- まず `--comments` を付けて出力を見て、落ちている関数/式の形を特定します。

### 2) 型が Any に落ちる（pure-rust が通らない / 生成が遅い）

- 推論は `aot/inference.rs` と `aot/types.rs` が中心です。
- 手元の入力 Julia 側で **引数/戻り値/ローカル**の型注釈を増やして再現性を確保してから、推論ロジックを拡張するのが安全です。

### 3) 生成 Rust の品質を上げたい（冗長・遅い）

- AoT パスは `aot/optimizer.rs` に集約しています。
- まずは “素直な式/ループ” に落ちる形を優先し、`rustc -O` が効きやすい出力を目指します。

### 4) Cranelift を試す

Cranelift backend は実験的です（対応範囲が限定的）。まずは最小例でテストを回すのがおすすめです。

```bash
timeout 180 cargo test -p subset_julia_vm --features cranelift aot::codegen::cranelift
```

## ベンチマーク

AoT / VM / Julia の比較は `benchmarks/scripts/run_benchmarks.sh` を参照してください（生成時間・`rustc` 時間・サイズも記録します）。

