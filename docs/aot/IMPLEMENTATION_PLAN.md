# AoT 実装ロードマップ（現状ベース）

**最終更新**: 2026-06-19

このファイルは “当初計画” の残骸を整理し、**いまコードに存在する AoT 実装**を前提に「次に何を埋めるか」を短くまとめたロードマップです。

## 現状（実装が存在するもの）

- **AoT CLI**: `subset_julia_vm/src/bin/aot.rs`
  - `.jl` / stdin / `-e` / `.sjir`（Program ロード）から Rust 出力まで一通り繋がっている
  - DCE（CallGraph フィルタ）が入っている
  - `-O` / `--stats` / `--check` / `--time-passes` / `--comments` / `--minimal-prelude` / `--pure-rust` が使える
  - `compile_from_ir_bytes(&[u8])` は serialized Core IR bytes から同じ Core IR -> AoT pipeline を使う
- **AoT 型推論**: `subset_julia_vm/src/aot/inference.rs`
- **AoT IR**: `subset_julia_vm/src/aot/ir.rs`（高レベル `AotProgram` と低レベル `IrModule` 系）
- **最適化パス**: `subset_julia_vm/src/aot/optimizer.rs`
- **Rust codegen**: `subset_julia_vm/src/aot/codegen/rust.rs`
- **AoT runtime crate**: `subset_julia_vm_runtime/`（動的型/ディスパッチ/組み込み関数/エラー）
- **ベンチスイート**: `benchmarks/scripts/run_benchmarks.sh`（AoT生成時間・`rustc`時間・サイズ含む）
- **Cranelift backend（実験）**: `subset_julia_vm/src/aot/codegen/cranelift.rs`（feature=`cranelift`）
- **対応サブセット文書**: `docs/aot/SUPPORT_MATRIX.md`

## 既知のギャップ（ドキュメントとして明示）

- 多重ディスパッチの “動的ディスパッチャ自動生成” は限定的（静的解決中心）
- Cranelift backend は対応範囲が限定的（配列/複雑な呼び出し等はプレースホルダを含む）

## 次にやること（優先度順）

### P0: 生成物の “正しさ” と “診断”

- **変換できない構文/式の明示**: `aot/analyze.rs` のエラーを、入力ソース位置・関数名・式形状と結びつけて出す
- **pure-rust 診断の拡充**: “何が動的になったか” を `AotProgram` 側でより具体的に報告

### P1: 低レベル化/コード生成の穴埋め

- Core IR で現れる主要パターンについて、`AotExpr`/`AotStmt` 変換の対応を広げる
- 生成 Rust の構造を、`rustc -O` が最適化しやすい形に寄せる（不要な一時変数/無駄な式の削減など）

### P2: 最適化パスの強化

- LICM / CSE / Strength reduction などを “安全な範囲” で段階的に拡張
- ベンチで regression を検知できるよう、`benchmarks/results/*/report.md` の出力を基準化する

### P3: Cranelift backend の現実的な位置づけ

- “対応範囲の明確化” と “テストの固定化” を優先し、Rust backend の代替として期待しない

## 参照

- `docs/aot/README.md`: 使い方（CLI/リンク方法）
- `docs/aot/DESIGN.md`: 現行設計メモ
- `docs/aot/IMPLEMENTATION_GUIDE.md`: どこを触るか
