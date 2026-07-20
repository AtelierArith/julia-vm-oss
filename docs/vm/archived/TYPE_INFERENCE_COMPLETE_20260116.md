# SubsetJuliaVM 型推論システム 完全ドキュメント（履歴アーカイブ）

> **Archive note (2026-06-11):** This file preserves the older all-in-one
> type-inference history/planning document. Current implementation guidance is
> maintained in `docs/vm/TYPE_INFERENCE_COMPLETE.md`.

**最終更新**: 2026-06-10
**ステータス**: 実装済み（ただし一部は設計/未統合あり）

---

## 目次

1. [概要](#概要)
2. [ユーザー向けドキュメント](#ユーザー向けドキュメント)
3. [実装状況](#実装状況)
4. [アーキテクチャ](#アーキテクチャ)
5. [実装ガイド](#実装ガイド)
6. [強化計画](#強化計画)
7. [今後の改善点](#今後の改善点)

---

## 概要

SubsetJuliaVM は、格子ベースの抽象解釈エンジン（v2）を使用した型推論システムを実装しています。主に「関数の返り値型推論」「for ループの要素型推論」「一部の条件分岐での型絞り込み」「転送関数による返り値型推論」を行います。

### 主要な特徴

- **抽象解釈**: 抽象型を使用したプログラム実行のシミュレーション
- **型格子**: 階層的な型システム（Bottom → Const → Concrete → Union → Conditional → Top）
- **不動点反復**: 型情報が安定するまで反復
- **転送関数**: 組み込み関数の返り値型を定義
- **型拡大（Widening）**: Union が複雑になりすぎる場合の保守的拡大

---

## ユーザー向けドキュメント

### 型推論の動作原理

型推論は以下の原則に従います：

1. **抽象解釈**: コンパイラは具体的な値の代わりに抽象型を使用してプログラム実行をシミュレート
2. **型格子**: 型は階層構造で組織化（Bottom → Const → Concrete → Union → Conditional → Top）
3. **不動点反復**: 推論エンジンは型情報が安定するまで反復
4. **転送関数**: 組み込み関数は引数に基づいて既知の返り値型を持つ
5. **型拡大**: Union が大きすぎる/複雑すぎる場合は保守的に拡大される（詳細は widening 参照）

### 推論されるもの

#### 変数の型

変数は使用に基づいて型が割り当てられます：

```julia
function example()
    x = 1        # x: Int64
    y = 2.0      # y: Float64
    z = x + y    # z: Float64 (promotion)
    z
end
```

#### ループ変数の型

ループ変数はイテレータの要素型から推論されます：

```julia
function sum_array(arr)
    total = 0
    for x in arr  # x: Int64 (配列の要素型から推論)
        total += x
    end
    total
end
```

#### 条件分岐での型絞り込み（現状の対応範囲）

現状の v2 推論は **環境分割（env splitting）**により、次の条件式で **変数そのもの（`Expr::Var`）**を対象に型を絞り込みます：

- `isa(x, T)`（Call / builtin の両方）
- `x === nothing` / `x !== nothing`（互換として `==` / `!=` も扱う）

注意:
- `&&`/`||`/`!` のような複合条件の解析・伝播が実装されています（Issue [#1286](https://github.com/AtelierArith/ailujsoi/issues/1286) 対応済み）。
- `isa` の else 側で「T 以外」へ厳密に絞れるとは限りません（`Top` からの差集合は保守的に `Top` のままになる場合があります）。

```julia
function process(val)
    if val isa Int
        val + 1  # val: Int64 (Any/Union から絞り込み)
    elseif val isa Float64
        val * 2.0  # val: Float64 (絞り込み)
    else
        0
    end
end
```

#### Union 型

異なる分岐が異なる型を返す場合、Union 型が推論されます：

```julia
function mixed_return(flag)
    if flag
        1        # Int64
    else
        2.0      # Float64
    end
    # 返り値型: Union{Int64, Float64}
end
```

### サポートされる型パターン

#### 配列

```julia
arr = [1, 2, 3]  # Array{Int64}
x = arr[1]       # x: Int64 (要素型が推論される)
```

#### タプル

```julia
tup = (1, 2.0, "hello")  # Tuple{Int64, Float64, String}
for x in tup
    # x: Union{Int64, Float64, String}
end
```

#### 範囲

```julia
for i in 1:10
    # i: Int64 (範囲の要素型)
end
```

#### 辞書

```julia
dict = Dict("a" => 1, "b" => 2)  # Dict{String, Int64}
for (k, v) in dict
    # k: String, v: Int64
end
```

#### 集合

```julia
s = Set([1, 2, 3])  # Set{Int64}
for x in s
    # x: Int64
end
```

### 型注釈が役立つ場合

型推論は強力ですが、明示的な型注釈が役立つ場合があります：

#### 関数パラメータ

```julia
# 注釈なし: パラメータ型は Any
function process(x)
    x + 1  # x: Any (実行時型チェックが必要な場合がある)
end

# 注釈あり: パラメータ型が既知
function process(x::Int64)
    x + 1  # x: Int64 (実行時チェック不要)
end
```

#### 複雑な返り値型

```julia
# 推論では Union{Int64, Float64} になる可能性がある
function maybe_number(flag)
    if flag
        1
    else
        2.0
    end
end

# 明示的な注釈で意図を明確化
function maybe_number(flag)::Union{Int64, Float64}
    if flag
        1
    else
        2.0
    end
end
```

### よくある落とし穴

#### 型拡大

異なる型が多すぎると、推論エンジンは `Any` に拡大します：

```julia
# 型が多すぎると Any に拡大される可能性がある
function many_types(flag)
    if flag == 1
        1
    elseif flag == 2
        2.0
    elseif flag == 3
        "three"
    elseif flag == 4
        true
    else
        :symbol
    end
end
```

#### ループ変数推論の制限

複雑なイテレータ型では、正確な要素型推論ができない場合があります：

```julia
# イテレータ型が不明な場合、Any と推論される可能性がある
for x in some_complex_iterator()
    # x: Any (イテレータ型が決定できない場合)
end
```

### 型推論の警告（コンパイル時診断）

型推論が保守的に `Any` (Top) に拡大される場合、コンパイル時に警告を発することができます。これは `compile/diagnostics.rs` で実装されています（Issue [#1285](https://github.com/AtelierArith/ailujsoi/issues/1285) で対応完了）。

#### 警告が発生する状況

- **未知の関数呼び出し**: 転送関数が登録されていない関数
- **Union 型の拡大**: 要素数が多すぎる（MAX_UNION_LENGTH > 4）または複雑すぎる（MAX_UNION_COMPLEXITY > 3）
- **再帰サイクル検出**: 関数間解析で再帰呼び出しサイクルを検出
- **不動点収束失敗**: IPO 解析が収束しなかった
- **未知の構造体/フィールド**: 構造体テーブルにない構造体へのアクセス
- **配列要素型不明**: 配列の要素型を決定できない
- **条件型結合**: 制御フロー依存の型を保守的に結合

#### 使用方法

警告はデフォルトで無効化されています。有効化するには：

```rust
use subset_julia_vm::compile::diagnostics::DiagnosticsCollector;

// 警告収集を有効化
DiagnosticsCollector::enable();

// コンパイル処理...

// 警告を取得
let warnings = DiagnosticsCollector::take();
for warning in warnings {
    println!("{}", warning);
}

// 警告収集を無効化
DiagnosticsCollector::disable();
```

警告の内容には以下が含まれます：
- 拡大の理由（`DiagnosticReason`）
- ソース位置（利用可能な場合）
- コンテキスト情報（変数名、関数名など）
- 拡大後の型（通常は "Any"）

### パフォーマンスへの影響

型推論はコンパイル時に実行され、実行時パフォーマンスには影響しません。実際、より良い型推論により以下が可能になります：

- より積極的な最適化
- より少ない実行時型チェック
- より良いコード生成

### 型格子階層

型推論システムは以下の階層を使用します：

- **Top** (Any) - 最も一般的な型、任意の値を受け入れる
- **Conditional** - 制御フローに敏感な型（定義されているが、実際には環境分割を使用）
- **Union** - 複数の具体型の Union
- **Concrete** - Int64, Float64, String などの特定の型
- **Const** - コンパイル時に既知の定数値（例: Const(42), Const(true)）
- **Bottom** - 最も具体的な型、到達不能コードを表す

`Const` 型は `Concrete` 型より具体的で、リテラルの具体的な値を追跡します。v2 推論エンジンは `infer_literal()` でリテラルを `Const` 型として生成し、格子の結合演算を通じて伝播します（例：異なる定数値を結合すると `Concrete` 型に昇格）。

---

## 実装状況

### 実装状況サマリー

| 項目 | 実装状況 | 統合状況 | 備考 |
|------|---------|---------|------|
| **型格子（Lattice）** | ✅ 実装済み | ✅ 統合済み | Bottom, Const, Concrete, Union, Conditional, Top |
| **Const 型（定数値）** | ✅ 実装済み | ✅ 統合済み | `infer_literal()` でリテラルを Const 型として生成、格子演算で伝播 |
| **抽象解釈エンジン** | ✅ 実装済み | ✅ 統合済み | `InferenceEngine`を`infer_function_return_type_v2`で使用（`mod.rs:1265`） |
| **転送関数（Transfer Functions）** | ✅ 実装済み | ✅ 統合済み | 算術・配列・文字列・数学関数・フィールド・イテレータ・コレクション実装済み |
| **条件分岐での型絞り込み** | ✅ 実装済み | ✅ 統合済み | `isa(x,T)` と `x ===/!== nothing`（`==/!=`互換）を env splitting で処理 |
| **ループ変数の型推論** | ✅ 実装済み | ✅ 統合済み | v2エンジンとローカル変数追跡の両方で使用 |
| **Range/Dict/Set型** | ✅ 実装済み | ✅ 統合済み | `ConcreteType`と`bridge.rs`に実装済み |
| **定数伝播（const_prop）** | ✅ 実装済み | ✅ 統合済み | `compile/const_prop/` が v2 に統合、binary/unary の定数畳み込みが有効 |
| **テスト** | ✅ 存在 | ✅ 有効 | fixture は主に “実行結果” を検証（推論型の直接検査ではない） |

### アーキテクチャ

型推論は2層構造で動作しています：

#### 1. 関数戻り値型推論（v2エンジン）

**場所**: `compile/mod.rs`（関数のコンパイル時に `infer_function_return_type_v2` を利用）

```rust
infer_function_return_type_v2(func, &shared_ctx.struct_table)
```

`InferenceEngine`を使用した完全な抽象解釈による推論：
- ループ変数の要素型推論
- 条件分岐での型絞り込み（`isa`チェック、`=== nothing`）
- Union型の推論
- 転送関数による組み込み関数の戻り値型

#### 2. ローカル変数型追跡（バイトコード生成用）

**場所**: `compile/inference.rs:238-249`

```rust
Stmt::ForEach { var, iterable, body, .. } => {
    let iterable_type = infer_value_type_with_structs(iterable, locals, struct_table);
    let lattice_iterable = LatticeType::from(&iterable_type);
    let elem_lattice = loop_analysis::element_type(&lattice_iterable);
    let elem_type = bridge::lattice_to_value_type(&elem_lattice);
    locals.insert(var.clone(), elem_type);
}
```

`loop_analysis.rs`の`element_type`関数を使用して、イテラブルから要素型を推論。

### 実装済み機能の詳細

#### 型格子（Lattice）

`compile/lattice/types.rs`に実装：

- `LatticeType::Bottom` - 底型（到達不能コード）
- `LatticeType::Const(ConstValue)` - 定数値型（追加実装）
- `LatticeType::Concrete(ConcreteType)` - 具体型
- `LatticeType::Union(BTreeSet<ConcreteType>)` - Union型
- `LatticeType::Conditional { slot, then_type, else_type }` - 条件依存型（定義のみ、実際は環境分割を使用）
- `LatticeType::Top` - 頂型（Any）

**Const 型の詳細**:
- `ConstValue::Int64(i64)` - 整数定数
- `ConstValue::Float64(f64)` - 浮動小数点定数
- `ConstValue::Bool(bool)` - 真偽値定数
- `ConstValue::String(String)` - 文字列定数
- `ConstValue::Nothing` - Nothing 定数

Const 型は Concrete 型より具体的で、定数伝播に使用されます。`infer_literal()` がリテラル（Int, Float, Bool, String, Nothing）から Const 型を生成し、格子の結合演算（`join`）で異なる定数値が合流すると対応する Concrete 型に昇格します。また、`infer_expr()` は binary/unary 演算で `const_prop` モジュールの `try_eval_binary`/`try_eval_unary` を呼び出し、両オペランドが Const の場合はコンパイル時に演算を評価します。

#### ConcreteType

以下の型が`ConcreteType`に実装済み：

- プリミティブ型: Int8-Int128, UInt8-UInt128, Float32/64, Bool, Char, String
- 特殊型: Nothing, Missing, Symbol
- 複合型: Array, Tuple, NamedTuple
- コレクション型: Range, Dict, Set, Generator, Pairs
- メタ型: DataType, Module, IO
- メタプログラミング型: Expr, QuoteNode, LineNumberNode, GlobalRef
- ユーザー定義型: Struct, Function

#### ループ変数の型推論

`compile/abstract_interp/loop_analysis.rs`の`element_type`関数：

| イテラブル | 要素型 |
|----------|--------|
| `Array{T}` | `T` |
| `Tuple{T1, T2, ...}` | `Union{T1, T2, ...}` または単一型 |
| `Range{T}` | `T` |
| `Dict{K, V}` | `Tuple{K, V}` |
| `Set{T}` | `T` |
| `String` | `Char` |

#### 条件分岐での型絞り込み

`compile/abstract_interp/conditional.rs`の`split_env_by_condition`関数：

- `isa(x, T)` パターン: then 分岐で `x` を `T` に絞り込み（meet）、else 分岐では差集合（subtract）で保守的に除外
- `x === nothing` / `x !== nothing`（互換として `==` / `!=` も扱う）: then/else を Nothing の meet/subtract で分割

機能:
- `&&` / `||` / `!` の条件式構造を解析して絞り込みを伝播します（Issue [#1286](https://github.com/AtelierArith/ailujsoi/issues/1286) で実装済み）。
- 絞り込み対象は基本的に `Expr::Var`（単純な変数）に限定されます。

#### 転送関数

`compile/tfuncs/`に実装済み：

- **算術演算** (`arithmetic.rs`): `+`, `-`, `*`, `/`, `^`, `div`, `mod`, `rem`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `!`
- **配列操作** (`array_ops.rs`): `length`, `size`, `getindex`, `setindex!`, `push!`, `pop!`, `zeros`, `ones`, `fill`, `collect`, `sum`, `prod`, `first`, `last`, `map`, `filter`
- **文字列操作** (`string_ops.rs`): `string`, `split`, `join`, `uppercase`, `lowercase`, `strip`, `lstrip`, `rstrip`, etc.
- **型変換** (`intrinsics.rs`): `Int`, `Int64`, `Int32`, `Float64`, `Float32`, `Bool`, `String`, `Char`, etc.
- **フィールド操作** (`field_ops.rs`): `getfield`, `setfield!`, `fieldtype`
- **イテレータ操作** (`iterator_ops.rs`): `iterate`, `length`, `eachindex`
- **コレクション操作** (`collection_ops.rs`): `keys`, `values`, `pairs`
- **数学関数** (`math_intrinsics.rs`): `sqrt`, `sin`, `cos`, `exp`, `log`, `abs`, `floor`, `ceil`, `round`, `min`, `max`, etc.
- **I/O**: `println`, `print`

全ての転送関数は `register_all()` 関数で一括登録されます。

##### Bottom 型の伝播規則（Issue #1717 対応）

転送関数は `Bottom` 型を正しく伝播する必要があります。`Bottom` は到達不能コード（unreachable code）を表し、例えば空の varargs タプルに対する for ループ内のコードなどに現れます。

**規則**: 二項演算の転送関数で、いずれかのオペランドが `Bottom` の場合、結果も `Bottom` を返す。

```rust
// 例: tfunc_add の実装パターン
pub fn tfunc_add(args: &[LatticeType]) -> LatticeType {
    if args.len() != 2 {
        return LatticeType::Top;
    }

    // Bottom propagation: unreachable code stays unreachable
    if matches!(&args[0], LatticeType::Bottom) || matches!(&args[1], LatticeType::Bottom) {
        return LatticeType::Bottom;
    }

    // ... 通常の型推論ロジック
}
```

**理由**:
1. 空の for ループ（例: `for y in ()` の場合）では、ループ変数 `y` の型が `Bottom` になる
2. `total += y` のような演算で、`y` が `Bottom` なら結果も `Bottom` であるべき
3. これにより格子の join 演算（`Complex{Float64} ⊔ Bottom = Complex{Float64}`）で正しい型が保持される

**影響を受ける転送関数**:
- 算術: `+`, `-`, `*`, `/`, `div`, `rem`, `mod`
- 比較: `==`, `<`, `<=`, `>`, `>=`
- ビット演算: `<<`, `>>`, `&`, `|`, `xor`

単項演算（`!`, `sign`, `floor` 等）は通常 `Bottom` の引数を受け取ることは少ないですが、必要に応じて同様のチェックを追加できます。

#### ValueType ↔ LatticeType ブリッジ

`compile/bridge.rs`に双方向変換を実装：

- `impl From<&ValueType> for LatticeType`
- `impl From<&LatticeType> for ValueType`
- `lattice_to_value_type()` ヘルパー関数

---

## アーキテクチャ

### ディレクトリ構造

```
subset_julia_vm_compile/src/compile/
├── inference.rs              # 型推論エントリポイント
├── bridge.rs                 # ValueType ↔ LatticeType 変換
├── diagnostics.rs            # コンパイル時診断（型拡大警告）
│
├── lattice/                  # 型格子モジュール
│   ├── mod.rs               # モジュールエクスポート
│   ├── types.rs             # LatticeType, ConcreteType 定義
│   ├── ops.rs               # 格子演算（join, meet, is_subtype_of）
│   └── widening.rs          # 型拡大ロジック
│
├── abstract_interp/          # 抽象解釈エンジン
│   ├── mod.rs               # モジュールエクスポート
│   ├── engine.rs            # InferenceEngine（抽象解釈エンジン）
│   ├── env.rs               # 型環境管理
│   ├── conditional.rs      # 条件分岐での型絞り込み
│   ├── loop_analysis.rs     # ループ変数の型推論
│   └── struct_info.rs       # 構造体型情報
│
├── const_prop/               # 定数伝播（v2 推論に統合済み）
│   ├── mod.rs
│   └── eval.rs
│
└── tfuncs/                   # 転送関数
    ├── mod.rs               # モジュールエクスポート
    ├── registry.rs          # 転送関数レジストリ
    ├── arithmetic.rs         # 算術演算の返り値型
    ├── array_ops.rs         # 配列操作の返り値型
    ├── string_ops.rs        # 文字列操作の返り値型
    ├── intrinsics.rs        # Core.Intrinsics の返り値型
    ├── field_ops.rs         # フィールドアクセス
    ├── iterator_ops.rs      # イテレータ操作
    ├── collection_ops.rs    # コレクション操作
    └── math_intrinsics.rs   # 数学関数
```

### モジュール依存関係

```
                    ┌─────────────────┐
                    │   lattice/      │
                    │  types.rs       │
                    │  ops.rs         │
                    │  widening.rs    │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
              ▼              ▼              ▼
     ┌────────────────┐ ┌─────────┐ ┌──────────────┐
     │ abstract_interp│ │ tfuncs/ │ │   bridge.rs  │
     │   engine.rs    │ │registry │ │  (ValueType  │
     │   env.rs       │◀│  .rs    │ │   変換)      │
     │   conditional  │ └─────────┘ └──────────────┘
     │   loop_analysis│                    │
     └───────┬────────┘                    │
             │                             │
             └──────────────┬──────────────┘
                            │
                            ▼
                   ┌────────────────┐
                   │  既存コード     │
                   │  inference.rs  │
                   │  expr/infer.rs │
                   └────────────────┘
```

### 型格子の実装

#### LatticeType（型格子要素）

```rust
// subset_julia_vm_compile/src/compile/lattice/types.rs

pub enum LatticeType {
    /// 底型（空集合、到達不能コード）
    Bottom,

    /// 定数値型（コンパイル時定数）
    /// Concrete 型より具体的で、定数伝播に使用される
    Const(ConstValue),

    /// 具体型（単一の型）
    Concrete(ConcreteType),

    /// Union 型（複数の型の和集合）
    /// MAX_UNION_LENGTH を超えると Top に拡大
    Union(BTreeSet<ConcreteType>),

    /// Conditional 型（制御フロー依存の型）
    /// if x isa T のような条件分岐で生成
    /// 注意: 実際の実装では環境分割アプローチを使用（conditional.rs 参照）
    Conditional {
        /// 条件が適用される変数名
        slot: String,
        /// 条件が真の場合の型
        then_type: Box<LatticeType>,
        /// 条件が偽の場合の型
        else_type: Box<LatticeType>,
    },

    /// 頂型（任意の型、型情報なし）
    Top,
}
```

#### 格子演算

主要な演算：

- **join (⊔)**: 型の結合（Union 型の作成）
- **meet (⊓)**: 型の交差（最大下界）
- **is_subtype_of (⊑)**: 型の包含関係
- **subtract**: 型の差（Conditional 型生成時に使用）

詳細は `subset_julia_vm_compile/src/compile/lattice/ops.rs` を参照。

### 抽象解釈エンジン

**ファイル**: `subset_julia_vm_compile/src/compile/abstract_interp/engine/`

`InferenceEngine` として実装され、以下の機能を提供：

- 不動点反復による型推論
- ループ変数の型推論
- 条件分岐での型絞り込み
- 転送関数による組み込み関数の返り値型推論
- 関数返り値型のキャッシュ

**使用箇所**: `subset_julia_vm_compile/src/compile/mod.rs:1265` で `infer_function_return_type_v2` が呼び出されています。

### Conditional 型の実装アプローチ

**ファイル**: `subset_julia_vm_compile/src/compile/abstract_interp/conditional.rs`

#### 仕様書の提案

仕様書では Conditional 型を格子に追加するアプローチを想定:

```rust
Conditional {
    slot: String,
    then_type: Box<LatticeType>,
    else_type: Box<LatticeType>,
}
```

#### 実際の実装

実装では **環境分割（Environment Splitting）** アプローチを採用:

```rust
pub struct SplitEnv {
    pub then_env: TypeEnv,  // then 分岐の環境
    pub else_env: TypeEnv,  // else 分岐の環境
}

pub fn split_env_by_condition(env: &TypeEnv, condition: &Expr) -> SplitEnv
```

#### トレードオフ

| 観点 | 環境分割（現実装） | Conditional 型（仕様） |
|------|------------------|----------------------|
| 実装の複雑さ | シンプル | 複雑 |
| 保守性 | 高い | 低い |
| 最適化の機会 | 限定的 | 広い |
| 型情報の保持 | 分岐後に失われる | 格子内で保持 |
| 関数シグネチャへの表現 | 不可 | 可能 |

**結論**: 現時点では環境分割アプローチで十分な機能を提供しています。将来的に最適化が重要になった場合、Conditional 型への移行を検討できます。

---

## 実装ガイド

### 型定義

#### LatticeType（型格子要素）

詳細な定義は `subset_julia_vm_compile/src/compile/lattice/types.rs` を参照。

#### ConcreteType（具体型）

詳細な定義は `subset_julia_vm_compile/src/compile/lattice/types.rs` を参照。

### 型格子の実装

#### 格子演算

主要な演算の実装：

- **join**: `subset_julia_vm_compile/src/compile/lattice/ops.rs`
- **meet**: `subset_julia_vm_compile/src/compile/lattice/ops.rs`
- **is_subtype_of**: `subset_julia_vm_compile/src/compile/lattice/ops.rs`
- **subtract**: `subset_julia_vm_compile/src/compile/lattice/ops.rs`

詳細な実装コードは `subset_julia_vm_compile/src/compile/lattice/ops.rs` を参照。

### 抽象解釈エンジン

#### 型環境

**ファイル**: `subset_julia_vm_compile/src/compile/abstract_interp/env.rs`

型環境は変数名から型へのマッピングを管理します。

#### エンジン本体

**ファイル**: `subset_julia_vm_compile/src/compile/abstract_interp/engine/`

`InferenceEngine` の主要なメソッド：

- `infer_function`: 関数の返り値型を推論
- `infer_block_with_fixpoint`: 不動点反復によるブロックの型推論
- `infer_expr`: 式の型推論

詳細な実装コードは `subset_julia_vm_compile/src/compile/abstract_interp/engine/` を参照。

### 転送関数

**実装状況**: ✅ 実装済み

転送関数レジストリは `subset_julia_vm_compile/src/compile/tfuncs/` に実装されています。

`register_all()` 関数で全ての転送関数を一括登録します。

詳細な実装コードは各モジュールファイルを参照：
- `registry.rs` - レジストリ本体
- `arithmetic.rs` - 算術演算と比較演算
- `array_ops.rs` - 配列操作
- `string_ops.rs` - 文字列操作
- `intrinsics.rs` - 組み込み関数と型変換
- `field_ops.rs` - フィールドアクセス
- `iterator_ops.rs` - イテレータ操作
- `collection_ops.rs` - コレクション操作
- `math_intrinsics.rs` - 数学関数

### 既存コードとの統合

#### ValueType ↔ LatticeType ブリッジ

**ファイル**: `subset_julia_vm_compile/src/compile/bridge.rs`

双方向変換を実装：
- `impl From<&ValueType> for LatticeType`
- `impl From<&LatticeType> for ValueType`
- `lattice_to_value_type()` ヘルパー関数

#### 統合アダプタ

**ファイル**: `subset_julia_vm_compile/src/compile/inference.rs`

`infer_function_return_type_v2` 関数が新しい型推論エンジンを使用した関数返り値型推論を提供します。

---

## 強化計画

### 現状の型推論の課題（実装済み機能）

以下の課題は既に実装済みです：

- ✅ **ループ変数の型推論**: `for x in arr` の `x` の型推論が実装済み
- ✅ **条件分岐での型絞り込み**: `if x isa Int` 後の型絞り込みが実装済み（環境分割アプローチ）
- ✅ **Union 型の実装**: Union 型の適切な結合・簡約が実装済み

### 残っている課題

- ✅ **Any への過度なフォールバック**: Issue #1201 で対応済み
- ✅ **関数間型伝播**: Issue #1202 で対応済み

### Julia 本家の型推論アーキテクチャ

#### 抽象解釈 (Abstract Interpretation)

Julia の型推論は **抽象解釈** に基づくデータフロー解析です。

#### 型格子 (Type Lattice)

Julia は階層的な型格子を使用：

```
MustAlias/Conditional (最も精密)
       ↑
    Partials
       ↑
     Consts
       ↑
    JLTypes (通常の型)
       ↑
      Any (最も緩い)
```

**実装状況**: SubsetJuliaVM では以下の階層を実装：
- `Top` (Any) - 最も一般的な型
- `Conditional` - 制御フロー依存型（定義のみ、実際は環境分割を使用）
- `Union` - Union 型
- `Concrete` - 具体型
- `Const` - 定数値型（追加実装）
- `Bottom` - 底型（最も具体的）

#### 型拡大 (Type Widening)

無限ループ防止のための制約：

- `MAX_UNION_LENGTH = 4`: Union 型の最大要素数
- `MAX_UNION_COMPLEXITY = 3`: Union 型の最大ネスト深度

複雑すぎる Union は自動的に拡大されます。

---

## 今後の改善点

> **注意**: 以下の課題は 2026-01-17 に対応完了しました。詳細は各 Issue を参照してください。

### 解決済みの課題

1. ✅ **構造体型のイテレーション対応** (Issue #1200 - CLOSED)
   - `LinRange{Float64}`、`StepRangeLen{Float64}` などのカスタムイテラブル構造体の要素型推論
   - 実装場所: `subset_julia_vm_compile/src/compile/abstract_interp/loop_analysis.rs`

2. ✅ **Any フォールバック削減** (Issue #1201 - CLOSED)
   - 型が決定できない場合の `Any` へのフォールバックを減らす

3. ✅ **関数間型伝播の改善** (Issue #1202 - CLOSED)
   - 関数呼び出し時の本体解析による返り値型推論

4. ✅ **テストの追加** (Issue #1203 - CLOSED)
   - fixture テスト追加
   - `conditional_narrowing.jl` と `union_types.jl` の有効化

5. ✅ **構造体フィールド型ルックアップの改善** (Issue #1204 - CLOSED)
   - ユーザー定義構造体のフィールドアクセス（`obj.field`）の型推論改善

6. ✅ **ValueType::Struct から LatticeType への変換改善** (Issue #1205 - CLOSED)
   - 構造体テーブルを使用した正確な型変換

### 今後の拡張候補

以下は将来の拡張候補として検討中です：

- **高階関数の完全対応** (Issue #1206)
   - `map`/`filter`の引数関数の戻り値型を推論
   - クロージャの型推論

---

## 関連ファイル

### 実装ファイル

- `subset_julia_vm_compile/src/compile/inference.rs` - 型推論エントリポイント（`infer_function_return_type_v2`）
- `subset_julia_vm_compile/src/compile/diagnostics.rs` - コンパイル時診断（型拡大警告）
- `subset_julia_vm_compile/src/compile/abstract_interp/engine/` - 抽象解釈エンジン（`InferenceEngine`）
- `subset_julia_vm_compile/src/compile/abstract_interp/conditional.rs` - 条件分岐型絞り込み（環境分割アプローチ）
- `subset_julia_vm_compile/src/compile/abstract_interp/loop_analysis.rs` - ループ変数型推論
- `subset_julia_vm_compile/src/compile/abstract_interp/env.rs` - 型環境（`TypeEnv`）
- `subset_julia_vm_compile/src/compile/abstract_interp/struct_info.rs` - 構造体型情報
- `subset_julia_vm_compile/src/compile/lattice/types.rs` - 型格子定義（Const 型含む）
- `subset_julia_vm_compile/src/compile/lattice/ops.rs` - 格子演算（join, meet, is_subtype_of, subtract）
- `subset_julia_vm_compile/src/compile/lattice/widening.rs` - 型拡大ロジック
- `subset_julia_vm_compile/src/compile/tfuncs/` - 転送関数レジストリ（全モジュール実装済み）
- `subset_julia_vm_compile/src/compile/bridge.rs` - ValueType ↔ LatticeType 変換
- `subset_julia_vm_compile/src/compile/const_prop/` - 定数伝播モジュール

### テストファイル

- `subset_julia_vm/tests/fixtures/type_inference/` - 型推論の fixture テスト

### ドキュメントファイル（統合前・アーカイブ済み）

以下のファイルは `docs/vm/archived/` に移動されました：

- `docs/vm/archived/TYPE_INFERENCE.md` - ユーザー向けドキュメント（このファイルに統合）
- `docs/vm/archived/TYPE_INFERENCE_STATUS.md` - 実装状況（このファイルに統合）
- `docs/vm/archived/TYPE_INFERENCE_IMPLEMENTATION_STATUS.md` - 実装ステータス（このファイルに統合）
- `docs/vm/archived/TYPE_INFERENCE_IMPLEMENTATION_GUIDE.md` - 実装ガイド（詳細な実装コード例を含む、主要部分をこのファイルに統合）
- `docs/vm/archived/TYPE_INFERENCE_ENHANCEMENT.md` - 型推論強化計画（詳細な設計仕様を含む、主要部分をこのファイルに統合）
- `docs/vm/archived/TYPE_INFERENCE_ENHANCEMENT_20260117.md` - 2026-01-17 時点の強化計画スナップショット

**注意**: 詳細な実装コード例や設計仕様が必要な場合は、アーカイブされたファイルを参照してください。

---

## 更新履歴

- 2026-01-19: コンパイル時型推論警告（診断機能）を追加（Issue #1285 対応完了）
  - `compile/diagnostics.rs` で診断インフラを実装
  - 未知関数、Union拡大、再帰サイクル、不動点収束失敗、未知フィールドアクセスなどで警告を発行可能
  - デフォルトで無効（オプトイン方式）
- 2026-01-19: 複合ブール条件（&&/||/!）の条件分岐型絞り込みを実装（Issue #1286 対応完了）
- 2026-01-18: Issue 参照先を更新（#1103-#1110 → #1200-#1206、アーカイブファイルパス修正）(Issue #1207)
- 2026-01-17: 型推論強化 Issue (#1200-#1205) 対応完了
- 2026-01-16: 完全ドキュメント作成（全 TYPE_INFERENCE 系ドキュメントを統合）
- 2026-01-15: 初版作成（Issue #901 対応）
