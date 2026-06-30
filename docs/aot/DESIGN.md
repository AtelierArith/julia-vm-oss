# AoT コンパイラ詳細設計（現行実装ベース）

**最終更新**: 2026-06-19
**ステータス**: 実装進行中（CLI/パイプライン/主要IR/最適化/コード生成は存在。範囲と精度は継続改善中）

このドキュメントは `docs/aot/README.md` の “使い方” ではなく、**現行コードに存在する構成要素**と、その接続（責務境界）をまとめる設計メモです。

---

## 全体アーキテクチャ

AoT は SubsetJuliaVM の「サブセット Julia 実行」を、VM インタープリタではなく **Rust ネイティブ実行**に寄せるための追加パスです。

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
                    ┌──────────────────────────────┐
                    │ Codegen: Rust (.rs)          │  ← デフォルト
                    │ Codegen: Cranelift (JIT)     │  ← feature=cranelift（実験的）
                    └──────────────────────────────┘
```

---

## コード配置（現状）

```text
subset_julia_vm/src/
  aot/
    mod.rs              # AotError/AotStats/AotOutput など
    analyze.rs          # Core IR → AoT IR 変換（program_to_aot_ir）
    call_graph.rs       # DCE/到達可能性解析
    inference.rs        # AoT 用型推論（StaticType / JuliaType を利用）
    ir.rs               # AoT IR（AotProgram / AotExpr / AotStmt 等）+ 低レベルIR（IrModule 等）
    optimizer.rs        # AoT 最適化（複数パス）
    types.rs            # AoT の型（JuliaType / StaticType）
    codegen/
      mod.rs            # CodegenConfig と backend トレイト
      rust.rs           # Rust codegen（主に AotCodeGenerator を利用）
      cranelift.rs      # Cranelift backend（feature=cranelift、実験的）

subset_julia_vm/src/bin/
  aot.rs                # AoT CLI（入力・prelude合成・DCE・推論・最適化・出力）

subset_julia_vm_runtime/
  src/                  # AoT ランタイム（Value/RuntimeError 等）
```

> 補足: `subset_julia_vm_runtime` は AoT 出力が動的型（`Value`）やランタイムヘルパを必要とする場合にリンクします。

---

## IR レイヤ

### 1) Core IR（`Program`）

AoT の入力は `lowering` が生成する Core IR（`subset_julia_vm::ir::core::Program`）です。AoT CLI は次の追加処理も行います:

- **Prelude 合成**: `base::get_prelude()` または `base::get_aot_prelude()` を先に lower してユーザープログラムへ merge
- **DCE**: `CallGraph::from_program(&program)` → `filter_program`

### 2) AoT IR（高レベル: `AotProgram`）

AoT 用の高レベル IR は `subset_julia_vm/src/aot/ir.rs` にあり、概ね “Rust に直接落としやすい AST” として扱います（`AotExpr` / `AotStmt` / `AotFunction` / `AotStruct` など）。

### 3) 低レベル IR（`IrModule` / `IrFunction` など）

`codegen/rust.rs` と `codegen/cranelift.rs` は、必要に応じて `IrModule`/`IrFunction` 系（ブロック/命令/終端）も扱います。現行の AoT CLI は主に **高レベル AoT IR → Rust** を利用します。

## Core IR → AoT IR の 2 段変換と feature gate

AoT は Julia source を直接 Rust に落とすのではなく、まず通常 pipeline と同じ Core IR (`Program`) へ lower し、その後 AoT 専用の高レベル IR (`AotProgram`) へ変換します。未対応機能は、できるだけ早い責務境界で gate します。

| Stage | 主な責務 | gate / 診断 |
|---|---|---|
| Parser | Julia source -> CST | 構文エラー。`juliars` は parser span Debug dump ではなく source 抜粋 + caret context を表示 |
| Lowering | CST -> Core IR `Program` | subset lowering 未対応構文。`UnsupportedFeature` Display + span context を表示 |
| Prelude merge / module load | Base/AoT prelude と external module を Core IR へ合成 | prelude/module load 失敗は internal/load error |
| Core IR DCE | call graph で到達関数へ絞る | feature gate ではなく noise reduction。後続の AoT 変換対象を減らす |
| AoT type inference | Core IR 上の式/関数へ `StaticType` を推定 | 型不明は即失敗ではなく `Any` / dynamic fallback へ退避することがある |
| Core IR -> AoT IR | `program_to_aot_ir` で Rust に落としやすい `AotProgram` へ変換 | `ccall` / `llvmcall` / Core intrinsics など native-call boundary は span 付き unsupported error |
| Named pass verifier | `AotPassStage` ごとに malformed AoT IR を検査 | 空関数名、空変数名、不正 index、rooting obligation などは `InvalidIR` |
| Optimizer | AoT IR 最適化 (`-O0..-O3`) | optimizer invariant 違反は `OptimizationError` / verifier error |
| Backend codegen | AoT IR -> Rust source | codegen 未対応 shape は `CodegenError` |
| Pure Rust enforcement | standalone Rust 要求の最終 gate | dynamic operation / residual runtime symbol が残れば `CodegenError` |

この分割により、Julia 互換性に関わる構文・lowering の問題は Core IR 以前、AoT 専用の表現不足は Core IR -> AoT IR 変換、生成物の standalone 性は backend 後の `--pure-rust` gate で報告します。Cranelift backend は現時点では低レベル実験経路であり、`juliars --backend cranelift` は `cranelift` feature build で scalar / straight-line subset のみ高レベル AoT IR から adapter lowering します。

---

## 型の扱い

AoT は `types.rs` の **`StaticType`**（コード生成向け）と **`JuliaType`**（推論・表現用）を中心に動きます。

- **静的に決まる型**: Rust のプリミティブ/`Vec<T>`/タプル/生成 struct 名へ変換される
- **不明/Union/Any**: 既定では `Value` にフォールバック（ランタイム依存の可能性）
- `--pure-rust`（CLI）: AoT IR に動的操作が残っている場合はエラーにする（完全静的のみ）

---

## 多重ディスパッチ（現状）

AoT codegen は関数を「名前 + 引数型」に基づいて **mangle** し、呼び出し側で **静的に解決**できるケースはそれを利用します。

一方で、実行時に `Value` を用いたフル動的ディスパッチャの自動生成は、現状は “コメント生成/将来拡張” の位置づけです（詳細は `codegen/rust.rs` の dispatcher 生成箇所を参照）。

---

## 最適化（AoT passes）

AoT の最適化は `subset_julia_vm/src/aot/optimizer.rs` に実装があり、DCE 後の `AotProgram` に対して複数パスを適用します。最適化の “完全性” より、まずは **生成コードが単純で `rustc -O` が効きやすい形**を目指します。

現行の実行経路は高レベル `AotProgram` optimizer が主経路です。`optimizer/pass.rs` にある `OptimizationPass` trait は低レベル `IrFunction` 用の足場も持ちますが、低レベル strength reduction / inlining は未接続です (Issue #6944)。そのため低レベル backend を使う場合も、現時点では高レベル AoT IR 最適化済みの形を入力境界として扱い、低レベル CFG 上で追加の inlining / strength reduction が走るとは仮定しません。

---

## バックエンド

### Rust backend（主経路）

- **入力**: 主に `AotProgram`
- **出力**: `.rs`（`rustc` でコンパイル）
- **用途**: ベンチ/配布/静的バイナリ（iOS 方針とも整合）

### Cranelift backend（実験的）

- `subset_julia_vm` の `cranelift` feature が必要（`features = ["cranelift"]`）
- 高速コンパイル（JIT）を狙うが、現時点では **対応命令・ランタイム連携が限定的**（配列/関数呼び出し等はプレースホルダを含む）
- “すぐ速い” というより、AoT パイプラインの実験場として位置づけるのが安全

---

## 既知のギャップ（ドキュメントとして明示）

- `aot::compile_from_ir_bytes(&[u8])` は serialized Core IR bytes をロードし、CLI の `.sjir` 入力と同じ Core IR -> AoT pipeline を通す。
- AoT 出力のランタイム依存（`Value`/`RuntimeResult`）は **生成物の内容に依存**する  
  - “完全静的” を保証したい場合は `--pure-rust` を使う

---

## 次に読むもの

- `docs/aot/README.md`: 使い方（CLI/オプション/リンク方法）
- `docs/aot/IMPLEMENTATION_GUIDE.md`: どこを触るか（開発者向け）
