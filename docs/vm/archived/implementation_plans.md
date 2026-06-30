# 実装計画アーカイブ

**作成日**: 2026-01-05
**目的**: 完了した実装計画ドキュメントの統合・保存

このドキュメントは、SubsetJuliaVM の開発過程で作成・完了した各種実装計画を統合したアーカイブです。
現在の実装状況は [../DONE.md](../DONE.md) を参照してください。

---

## 目次

1. [Phase 0-5: 基盤整備とパーサ統合](#phase-0-5-基盤整備とパーサ統合)
2. [モジュールシステム](#モジュールシステム)
3. [Union 型サポート](#union-型サポート)
4. [Linear Indexing](#linear-indexing)
5. [@generated 関数](#generated-関数)
6. [ファイル分割リファクタリング](#ファイル分割リファクタリング)
7. [VM アーキテクチャ改善](#vm-アーキテクチャ改善)
8. [Pure Julia 移行計画](#pure-julia-移行計画)
9. [配列実装](#配列実装)

---

## Phase 0-5: 基盤整備とパーサ統合

### Phase 0: 基盤整備 (2025-12-19 完了)

- tree-sitter と tree-sitter-julia の追加・ビルド確認
- thiserror の追加（error module で使用）
- プロジェクト構造の確立

### Phase 1: パーサ層 (2025-12-19 完了)

- `Span` 型の実装（start, end, line, column）
- tree-sitter-julia の言語ファイル統合
- `JuliaParser` / `ParsedSource` の実装
- `NodeKind` enum の定義（全Julia構文要素）
- `CstWalker` の実装

### Phase 2: Lowering層 (2025-12-20 完了)

- `UnsupportedFeatureKind` の完全な enum 定義
- `Program`, `Function`, `Block` の実装
- `Stmt` / `Expr` enum の完全な定義
- すべてのノードに `Span` を保持
- `serde` によるシリアライズ実装

### Phase 5: 構文拡張 (2025-12-20 完了)

- If/Elseif/Else
- While/Break/Continue
- 短絡評価 (&&, ||)
- ユーザ定義関数呼び出し

### Pure Rust パーサー統一 (2025-12-30 完了)

- tree-sitter-julia 依存を完全に削除
- WASM/Native 両対応の Pure Rust パーサー
- C 依存なし、完全にポータブル

---

## モジュールシステム

**完了日**: 2025-12-28

### 実装済み機能

| 機能 | 説明 |
|------|------|
| `using Module` | モジュールの全 export をスコープに取り込む |
| `import Module` | モジュール名のみをスコープに取り込む |
| `export func` | モジュールから関数を公開 |
| `Module.func()` | モジュール修飾呼び出し |
| `import Module: func` | 特定の関数のみインポート |
| `using Module: func` | 特定の関数のみ using |
| ネストモジュール | `module A; module B; end; end` |
| `Base.func()` | 組み込み関数への修飾アクセス |
| シャドウイング | ユーザー定義関数が組み込みを上書き可能 |
| Module 型 | モジュールをファーストクラスの値として扱う |
| モジュール別名 | `S = Statistics; S.mean(...)` |

### 関数解決順序

1. 現在のスコープ（ローカル関数）
2. 現在のモジュール内の関数
3. `using` されたモジュールの export 関数
4. グローバルビルトイン関数

### stdlib モジュール

- Base, Core, Main
- Statistics (mean, var, std, median, cov, cor, quantile)
- Random (rand, randn)
- Test

---

## Union 型サポート

**完了日**: 2025-12-28

### 実装内容

- `JuliaType::Union(Vec<JuliaType>)` バリアント
- `is_subtype_of()` での Union 型サブタイプチェック
- `from_name()` での `Union{...}` パース
- `Union{}` (Bottom) のサポート

### サブタイプルール

- `T <: Union{T1, T2}` ⟺ `T <: T1 || T <: T2`
- `Union{T1, T2} <: U` ⟺ `T1 <: U && T2 <: U`
- `Union{} <: T` for all T (Bottom は全ての型のサブタイプ)

---

## Linear Indexing

**完了日**: 2026-01-04

### 概要

多次元配列に対する単一インデックスアクセス（Column-Major Order）のサポート。

### 実装内容

- `linear_index()` 関数の拡張
- 単一インデックスで多次元配列にアクセス可能
- `vec()` 関数が多次元配列で動作

### 変換式 (1-indexed)

```
Multi-Index → Linear Index:
linear = i + (j-1)*m + (k-1)*m*n + ...

Linear Index → Multi-Index (2D):
i = ((linear - 1) % m) + 1
j = ((linear - 1) ÷ m) + 1
```

---

## @generated 関数

**完了日**: 2026-01-05

### Phase 1: フォールバック実行

- `if @generated ... else fallback end` パターンをサポート
- `@generated` を `false` に置換し、else ブランチを実行
- iOS App Store 制限（JIT 禁止）に準拠

### Phase 2: Val{N} 静的特殊化

- Val{N} 型パラメータから整数値 N を抽出
- コンパイラ: `val_type_params: HashSet<String>` で Val{N} パターンを追跡
- ランタイム: 型名から整数を抽出し `frame.locals_i64` に格納
- `for i in 1:N` のようなループで N を直接使用可能

### Phase 3: Quote アンクォート

- シンプルな quoted expression を「アンクォート」して直接実行
- `try_unquote_generated_block` 関数で quote 式を検出・変換
- サポートパターン: `:(expr)` 単独、または `var = :(expr)` 代入形式
- **注意**: Phase 3 の動作は標準 Julia と異なる

---

## ファイル分割リファクタリング

**完了日**: 2024-12-30

### 1500行ルール

ファイルが1500行を超えた段階で、リファクタリング（ファイル分割）を検討する。

### 完了した分割

| ファイル | 分割前 | 分割後 |
|---------|--------|--------|
| compile/expr.rs | 4,795行 | 8ファイル (すべて1,500行以下) |
| vm/mod.rs | 4,955行 | 1,359行 + exec.rs |
| lowering/expr.rs | 2,336行 | 6ファイル (すべて800行以下) |

### compile/expr/ 構造

```
compile/expr/
├── mod.rs       # 764行
├── builtin.rs   # 1,409行
├── call.rs      # 912行
├── binary.rs    # 738行
├── infer.rs     # 731行
├── struct_.rs   # 214行
├── collection.rs # 129行
└── unary.rs     # 105行
```

### lowering/expr/ 構造

```
lowering/expr/
├── mod.rs        # 325行 - メインディスパッチ + 共通関数
├── call.rs       # 796行
├── collection.rs # 484行
├── misc.rs       # 324行
├── binary.rs     # 260行
└── literal.rs    # 240行
```

---

## VM アーキテクチャ改善

### Core Intrinsics 実装 (2025-12-25 完了)

`1 + 2` のような演算を内部で `Base.add_int` 関数呼び出しとして処理する設計に変更。

### 実装された Intrinsics (約50種)

- 整数算術: NegInt, AddInt, SubInt, MulInt, SdivInt, SremInt
- 浮動小数点: NegFloat, AddFloat, SubFloat, MulFloat, DivFloat, PowFloat
- 比較演算: EqInt, NeInt, SltInt, SleInt, SgtInt, SgeInt, ...
- ビット演算: AndInt, OrInt, XorInt, NotInt, ShlInt, LshrInt, AshrInt
- 型変換: Sitofp, Fptosi
- 低レベル数学: SqrtLlvm, FloorLlvm, CeilLlvm, TruncLlvm, ...
- 複素数: NegComplex, AddComplex, SubComplex, MulComplex, DivComplex, ...

### VM 最適化 (2026-01-01 完了)

- VM 命令プロファイラ (`src/vm/profiler.rs`)
- 12 個の融合命令（Load-Op, Store-Op, Compare-Jump fusion）
- Peephole オプティマイザ (`src/compile/peephole.rs`)
- Parse/Lower: 28-45% 改善、Compile: 14-52% 改善

### 3層アーキテクチャ目標

```
Layer 3: SubsetJulia Code     ← Julia コードで実装可能な関数
Layer 2: Builtin Functions    ← Rust 実装の組み込み関数
Layer 1: VM Intrinsics        ← 最小限の固定命令セット（約 50 命令）
```

---

## Pure Julia 移行計画

### 目標

- VM コア変更なしで新機能追加可能に
- Julia 標準ライブラリとの互換性向上
- コードの可読性・保守性向上

### 現状サマリー

| 項目 | 数値 |
|------|------|
| Rust Builtin 関数 | 166 個 |
| Pure Julia 関数 | 214 個 |
| 移行済み | 49 個 |
| 移行候補 | 約 50 個 |
| 移行困難（HOF等）| 約 70 個 |

### Phase 1: 統計関数 (完了)

- `var`, `varm`, `std`, `stdm`, `median`, `middle`, `quantile`, `cov`, `cor`

### Phase 2: 文字列操作 (一部完了)

- `strip`, `lstrip`, `rstrip`, `chomp`, `chop`

### Phase 3: 配列ユーティリティ (一部完了)

- `repeat`, `circshift`, `rot180`, `rotl90`, `rotr90`

### Phase 4: 浮動小数点プロパティ (完了)

- `eps`, `floatmin`, `floatmax`, `typemin`, `typemax`

### 移行困難な関数

高階関数（`map`, `filter`, `reduce` 等）は VM アーキテクチャ制限により Rust Builtin を維持。

---

## 配列実装

### SubsetJuliaVM の配列構造

```rust
pub enum ArrayData {
    F32(Vec<f32>), F64(Vec<f64>),
    I8(Vec<i8>), I16(Vec<i16>), I32(Vec<i32>), I64(Vec<i64>),
    U8(Vec<u8>), U16(Vec<u16>), U32(Vec<u32>), U64(Vec<u64>),
    Bool(Vec<bool>), String(Vec<String>), Char(Vec<char>),
    StructRefs(Vec<usize>), Any(Vec<Value>),
}

pub struct ArrayValue {
    pub data: ArrayData,
    pub shape: Vec<usize>,
    pub struct_type_id: Option<usize>,
}
```

### Julia との比較

| Feature | Julia | SubsetJuliaVM |
|---------|-------|---------------|
| Memory model | GenericMemory + offset | Rust Vec (no offset) |
| GC | Tracing GC | Rust ownership (Rc<RefCell>) |
| Growth strategy | Explicit overallocation | Rust Vec automatic |
| Offset support | Yes | No |
| Type-segregated | Yes | Yes |
| Column-major | Yes | Yes |

### 実装済み操作

- Pure Julia: sum, prod, minimum, maximum, vcat, hcat, vec, axes, reverse, circshift, rot90系, sort系
- Rust Builtin: zeros, ones, push!, pop!, reshape, map, filter, reduce

---

## 参考資料

- [DONE.md](../DONE.md) - 実装済み機能一覧
- [STATUS.md](../STATUS.md) - 現状分析
- [DESIGN.md](../DESIGN.md) - 設計思想
