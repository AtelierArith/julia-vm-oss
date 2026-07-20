# 型付き配列システムの整理計画

**作成日**: 2026-01-05
**最終更新**: 2026-01-06
**ステータス**: Phase 1, 2, 3, 4 完了

## 概要

`Value::Array(ArrayRef)` の型システムを整理し、効率性と型安全性を向上させる。

---

## Julia 本家の配列実装

### 配列構造 (`julia/src/julia.h`)

```c
// Julia 1.11+ の配列構造
typedef struct {
    jl_genericmemoryref_t ref;  // メモリ参照
    size_t dimsize[];           // 次元サイズ
} jl_array_t;

typedef struct {
    size_t length;
    void *ptr;
    // インラインデータまたはオーナーポインタ
} jl_genericmemory_t;
```

### 要素格納方式 (`julia/base/genericmemory.jl`)

Julia は要素型に応じて3つの格納方式を使用:

```julia
isbits = 0      # isbits 型: インライン格納（連続メモリ）
isboxed = 1     # 非 isbits 型: ポインタ格納
isunion = 2     # Union 型: タグ付き格納
```

#### isbits 型の例

```julia
# Complex{Float64} は isbits（re, im が連続）
struct Complex{T<:Real} <: Number
    re::T
    im::T
end
# Vector{Complex{Float64}} → [re1, im1, re2, im2, ...] (AoS)

# Tuple{Int64, Float64} も isbits
# Vector{Tuple{Int64, Float64}} → [a1, b1, a2, b2, ...] (AoS)
```

**重要**: Julia は **AoS (Array of Structs)** 形式を使用。SoA ではない。

---

## 現状分析

### 現在のアーキテクチャ

```
subset_julia_vm_vm/src/vm/value.rs

┌─────────────────────────────────────────────────────────────┐
│ Value enum                                                  │
│   └── Array(ArrayRef)                                       │
│         │                                                   │
│         └── Rc<RefCell<ArrayValue>>                         │
│               ├── data: ArrayData                           │
│               ├── shape: Vec<usize>                         │
│               └── struct_type_id: Option<usize>             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ ArrayData enum (型別ストレージ)                             │
│   ├── F32(Vec<f32>)                                         │
│   ├── F64(Vec<f64>)                                         │
│   ├── I8(Vec<i8>) ... I64(Vec<i64>)                         │
│   ├── U8(Vec<u8>) ... U64(Vec<u64>)                         │
│   ├── Bool(Vec<bool>)                                       │
│   ├── String(Vec<String>)                                   │
│   ├── Char(Vec<char>)                                       │
│   ├── StructRefs(Vec<usize>)  ← ヒープインデックス          │
│   └── Any(Vec<Value>)         ← ボックス化（非効率）        │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ ArrayElementType enum (要素型識別)                          │
│   F32, F64, I8..I64, U8..U64, Bool, String, Char,           │
│   Struct, StructOf(usize), Any                              │
└─────────────────────────────────────────────────────────────┘
```

### 使用状況

| ファイル | `Value::Array` 使用数 | `ArrayData::` 使用数 |
|----------|----------------------|---------------------|
| `vm/exec.rs` | 62 | 28 |
| `vm/builtins_exec.rs` | 32 | 18 |
| `vm/mod.rs` | 13 | 21 |
| `vm/hof_exec.rs` | 9 | 7 |
| その他 | 31 | 268 |
| **合計** | **147** | **342** |

---

## 問題点

### 1. `ArrayData::Any` の非効率性

```rust
ArrayData::Any(Vec<Value>)  // 各要素が Value enum (56+ bytes)
```

- 各要素が `Value` enum 全体のサイズを消費
- キャッシュ効率が悪い
- 数値演算時に毎回パターンマッチが必要

**影響**: HOF (`map`, `filter`) で異種型配列を扱う場合のパフォーマンス低下

### 2. `StructRefs` と VM コンテキストの分離

```rust
ArrayData::StructRefs(Vec<usize>)  // ヒープインデックスのみ
```

- 実際の構造体データへのアクセスに VM の `struct_heap` が必要
- `ArrayValue` 単体では構造体の内容を参照できない
- `struct_type_id` で型情報は保持しているが、名前解決に `struct_defs` が必要

### 3. タプル配列の未サポート

```rust
// 現在動作しない
[(1, "a"), (2, "b"), (3, "c")]  // タプル配列
```

- `ArrayData` に `Tuple` バリアントがない
- `ArrayData::Any` にフォールバックすると非効率

### 4. Complex 配列の特殊処理

```rust
// value.rs:778
pub fn set_complex(&mut self, ...) -> Result<(), VmError> {
    // VM-level struct construction が必要
    Err(VmError::TypeError(...))
}
```

- Complex は Pure Julia struct として実装されている
- 配列内の Complex 要素の効率的な格納方法が未整備

### 5. 型情報の冗長性

```rust
ArrayElementType  // コンパイル時の型識別
ValueType         // ランタイム時の型識別
JuliaType         // 型オブジェクト表現
```

3つの型表現が存在し、変換が頻繁に必要

---

## 改善計画

### Phase 1: Complex 配列の効率化（優先度：高）

#### 目標
Complex 配列を `ArrayData::F64` の interleaved 形式で格納

#### 設計

```rust
pub struct ArrayValue {
    pub data: ArrayData,
    pub shape: Vec<usize>,
    pub struct_type_id: Option<usize>,
    pub is_complex: bool,  // 新規追加
}
```

```
Complex 配列 [1+2im, 3+4im] の格納:
data: ArrayData::F64([1.0, 2.0, 3.0, 4.0])  // [re1, im1, re2, im2, ...]
shape: [2]
is_complex: true
```

#### タスク

- [ ] `ArrayValue` に `is_complex: bool` フラグを追加
- [ ] Complex 配列作成時の interleaved 格納を実装
- [ ] `get_value` / `set_value` で Complex の pack/unpack を実装
- [ ] `zeros(Complex{Float64}, n)` の対応
- [ ] テスト追加: `complex_array_interleaved`

### Phase 2: Tuple 配列のサポート ✅ 完了（2026-01-06）

#### 目標
同型タプルの配列を効率的に格納（Julia 互換の AoS 形式）

#### Julia との互換性

Julia では Tuple は **AoS (Array of Structs)** として格納:
```julia
# Vector{Tuple{Int64, Float64}}
# メモリレイアウト: [a1, b1, a2, b2, a3, b3, ...]
```

#### 実装: `element_type_override` + `ArrayData::Any`

既存の `element_type_override` パターン（Complex 配列で使用）を拡張:

```rust
// ArrayElementType に TupleOf バリアントを追加
#[derive(Debug, Clone, PartialEq, Eq, Hash)]  // Copy を削除
pub enum ArrayElementType {
    // ... 既存のバリアント ...
    TupleOf(Vec<ArrayElementType>),  // 新規追加
}

// ヘルパーメソッド
impl ArrayElementType {
    pub fn is_tuple(&self) -> bool;
    pub fn tuple_field_types(&self) -> Option<&Vec<ArrayElementType>>;
    pub fn tuple_arity(&self) -> Option<usize>;
}
```

**重要な変更**: `ArrayElementType` と `ValueType` から `Copy` トレイトを削除
（`TupleOf(Vec<...>)` を含むため）

#### 実装詳細

```rust
// タプル配列コンストラクタ
impl ArrayValue {
    pub fn tuple_array(data: Vec<Value>, shape: Vec<usize>,
                       field_types: Vec<ArrayElementType>) -> Self;
    pub fn with_tuple_capacity(field_types: Vec<ArrayElementType>,
                               capacity: usize) -> Self;
}

// get/set/push/pop で TupleOf を処理
// 現在は ArrayData::Any に格納し、element_type_override で型情報を保持
```

#### コンパイラ・VM 修正

- `push!`, `pushfirst!`: 値を F64 に強制変換せず、元の型を保持
- `setindex!` (stmt.rs): 同様に元の型を保持
- `IndexStore` 命令: タプル値を検出して直接格納

#### タスク

- [x] `ArrayElementType::TupleOf` バリアントを追加
- [x] `Copy` トレイトを削除し `.clone()` に移行（23ファイル修正）
- [x] ヘルパーメソッド追加: `is_tuple()`, `tuple_field_types()`, `tuple_arity()`
- [x] `ArrayValue` コンストラクタ: `tuple_array()`, `with_tuple_capacity()`
- [x] `get()`, `set()`, `push()`, `pop()` で `TupleOf` を処理
- [x] コンパイラ修正: 非数値型のサポート
- [x] VM 修正: `IndexStore` でタプル値を処理
- [x] テスト追加: `tuple_array_basic`, `tuple_array_pushpop`, `tuple_array_setindex`

#### 今後の最適化オプション

AoS interleaved 格納（Complex 配列と同様）は将来的に実装可能:
```
[(1, 2.0), (3, 4.0)] の格納:
element_type_override: TupleOf([I64, F64])
data: ArrayData::Any([I64(1), F64(2.0), I64(3), F64(4.0)])  // interleaved
shape: [2]
```

現在は `ArrayData::Any` でタプル全体を格納（ボックス化）。
パフォーマンス要件に応じて interleaved 形式に移行可能。

### Phase 3: 型情報の統合 ✅ 完了（2026-01-06）

#### 目標
`ArrayElementType` と `ValueType` の関係を明確化

#### 実装内容

`ArrayElementType` に型変換メソッドを追加:

```rust
impl ArrayElementType {
    /// ValueType への変換
    pub fn to_value_type(&self) -> ValueType;

    /// ValueType からの変換
    pub fn from_value_type(vt: &ValueType) -> Self;
}
```

#### タスク

- [x] `ArrayElementType` ↔ `ValueType` 変換メソッドを追加
- [x] 重複した型変換コードを統合（`builtin.rs` の `value_type_to_array_element_type` を置換）
- [x] `JuliaType` との関係を明確化（主に `runtime_type()` で使用）

### Phase 4: isbits 構造体のインライン格納 ✅ 完了（2026-01-06）

#### 目標
構造体配列のアクセスを Julia 互換で効率化

#### Julia の構造体格納方式

```julia
# isbits 構造体（全フィールドが primitive）
struct Point
    x::Float64
    y::Float64
end
# Vector{Point} → インライン格納 [x1, y1, x2, y2, ...]

# 非 isbits 構造体（ポインタを含む）
mutable struct Node
    value::Int
    next::Union{Node, Nothing}
end
# Vector{Node} → ポインタ配列（現在の StructRefs に相当）
```

#### 実装: `StructInlineOf` バリアント + `element_type_override` パターン

既存の `element_type_override` パターン（Complex/Tuple 配列で使用）を拡張:

```rust
// ArrayElementType に StructInlineOf バリアントを追加
pub enum ArrayElementType {
    // ... 既存のバリアント ...
    StructInlineOf(usize, usize),  // (type_id, field_count)
}

// ヘルパーメソッド
impl ArrayElementType {
    pub fn is_isbits(&self) -> bool;
    pub fn is_struct_inline(&self) -> bool;
    pub fn struct_inline_info(&self) -> Option<(usize, usize)>;
}

// StructDefInfo に isbits 判定
impl StructDefInfo {
    pub fn is_isbits(&self) -> bool;
}
```

#### ストレージ形式

```
[Point(1.0, 2.0), Point(3.0, 4.0)] の格納:
element_type_override: StructInlineOf(type_id, 2)
data: ArrayData::Any([F64(1.0), F64(2.0), F64(3.0), F64(4.0)])  // AoS
shape: [2]
struct_type_id: Some(type_id)
```

#### 非 isbits 構造体

現状の `StructRefs` を維持:
```rust
ArrayData::StructRefs(Vec<usize>)  // ヒープインデックス
```

#### タスク

- [x] `is_isbits()` 判定関数を `ArrayElementType` と `StructDefInfo` に追加
- [x] `StructInlineOf(type_id, field_count)` バリアントを追加
- [x] `isbits_struct_array()`, `with_isbits_struct_capacity()` コンストラクタを追加
- [x] `get()`, `set()`, `push()`, `pop()` で `StructInlineOf` を処理
- [x] VM `exec.rs` で `NewArrayTyped` を更新
- [x] match 式の網羅性を修正（`infer.rs`, `builtins_exec.rs`, `type_ops.rs`）

---

## マイルストーン

| Phase | 内容 | ステータス |
|-------|------|-----------|
| Phase 1 | Complex 配列の効率化 | ✅ 完了 |
| Phase 2 | Tuple 配列のサポート | ✅ 完了（2026-01-06） |
| Phase 3 | 型情報の統合 | ✅ 完了（2026-01-06） |
| Phase 4 | isbits 構造体のインライン格納 | ✅ 完了（2026-01-06） |

---

## Julia 互換性チェックリスト

| 項目 | Julia の方式 | 本計画 | 互換性 |
|------|-------------|--------|--------|
| Complex 配列 | AoS interleaved `[re1,im1,re2,im2]` | Phase 1: 同方式 | ✅ |
| Tuple 配列 | AoS `[a1,b1,a2,b2]` | Phase 2: 同方式 | ✅ |
| isbits 構造体配列 | インライン AoS | Phase 4: 同方式 | ✅ |
| 非 isbits 構造体配列 | ポインタ配列 | 現状維持 (StructRefs) | ✅ |
| Union 型配列 | タグ付き格納 | 未対応 | ⚠️ |

---

## 互換性への影響

### 変更なし
- `Value::Array(ArrayRef)` の外部インターフェース
- `ArrayValue::get()` / `set()` の API
- 既存テストの動作
- Julia との互換性（SubsetJuliaVM で動くコードは Julia でも動く）

### 内部変更
- `ArrayData` enum のバリアント追加
- `ArrayValue` のフィールド追加
- Complex/Tuple/isbits構造体配列の内部表現（AoS 形式）

---

## テスト計画

### 新規テスト

```
tests/fixtures/arrays/
├── complex_array_interleaved.jl   # Phase 1 ✅
├── complex_array_ops.jl           # Phase 1
├── tuple_array_basic.jl           # Phase 2 ✅
├── tuple_array_pushpop.jl         # Phase 2 ✅
├── tuple_array_setindex.jl        # Phase 2 ✅
└── struct_array_soa.jl            # Phase 4
```

### 回帰テスト

既存の配列関連テスト（50+件）が引き続きパスすることを確認
✅ 206 件の Fixture テストが全てパス（2026-01-06 時点）

---

## 参考資料

### SubsetJuliaVM
- `subset_julia_vm_vm/src/vm/value.rs` - 現在の実装
- `docs/vm/STATUS.md` - プロジェクト状況

### Julia 本家（`julia/` ディレクトリ）
- `julia/src/julia.h:185-195` - `jl_array_t`, `jl_genericmemory_t` 定義
- `julia/base/genericmemory.jl:101-102` - isbits/isboxed/isunion の定義
- `julia/base/complex.jl:13-16` - Complex 構造体の定義
- `julia/base/tuple.jl` - Tuple の操作

### 外部ドキュメント
- Julia 公式ドキュメント: [Multi-dimensional Arrays](https://docs.julialang.org/en/v1/manual/arrays/)
- Julia Internals: [Memory Layout](https://docs.julialang.org/en/v1/devdocs/object/)
