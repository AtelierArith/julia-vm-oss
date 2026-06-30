# 技術リファレンス

> **Archive note (2026-06-11):** This 2026-01-05 reference snapshot is
> preserved for historical context. It describes retired runtime carriers such
> as `Value::Array(ArrayRef)` and older VM instruction names. Use
> `docs/vm/ARCHITECTURE_OVERVIEW.md`, `docs/vm/TYPE_SYSTEM.md`,
> `docs/vm/COLLECTIONS.md`, and `docs/vm/CALL_INSTRUCTIONS.md` for current
> architecture and runtime references.

**作成日**: 2026-01-05
**目的**: SubsetJuliaVM の技術的な詳細情報をまとめたリファレンスドキュメント

---

## 目次

1. [アーキテクチャ概要](#アーキテクチャ概要)
2. [型システム](#型システム)
3. [VM 命令セット](#vm-命令セット)
4. [Builtin 関数](#builtin-関数)
5. [配列実装](#配列実装)
6. [Intrinsics](#intrinsics)

---

## アーキテクチャ概要

### 3層アーキテクチャ

```
┌─────────────────────────────────────────────────────────┐
│  Layer 3: SubsetJulia Code                              │
│  sin, cos, map, filter, push!, pop!, ...               │
├─────────────────────────────────────────────────────────┤
│  Layer 2: Builtin Functions (Rust 実装)                 │
│  typeof, isa, getfield, setfield!, throw, ...          │
├─────────────────────────────────────────────────────────┤
│  Layer 1: VM Intrinsics (CPU 命令相当)                  │
│  add_int, mul_float, eq_int, sqrt_llvm, ...            │
└─────────────────────────────────────────────────────────┘
```

### パイプライン

```
Julia source → Parser (Pure Rust) → CST
            → Lowering → Core IR (or UnsupportedFeature error)
            → Compiler → Bytecode
            → VM → Results (or RuntimeError)
            → Swift/iOS via C ABI
```

---

## 型システム

### 型階層

```
Any
├── Number
│   ├── Real
│   │   ├── Integer
│   │   │   └── Int64
│   │   └── AbstractFloat
│   │       └── Float64
│   └── Complex{T}
├── AbstractString
│   └── String
├── AbstractChar
│   └── Char
├── AbstractArray
│   └── Array
├── Tuple
├── NamedTuple
├── Dict
├── Nothing
└── DataType
```

### JuliaType enum (Rust)

```rust
pub enum JuliaType {
    Int64, Float64, String, Char,
    Array, Tuple, NamedTuple, Dict,
    UnitRange, StepRange,
    DataType, Nothing,
    Bottom,                          // Union{}
    Union(Vec<JuliaType>),          // Union{T1, T2, ...}
    Struct(String),                  // Complex{Float64} など
    AbstractUser(String, Option<String>),  // 抽象型
}
```

### Value enum (Rust)

| Julia 型 | Rust Value | 説明 |
|----------|-----------|------|
| `Int64` | `Value::I64(i64)` | 64ビット整数 |
| `Float64` | `Value::F64(f64)` | 64ビット浮動小数点 |
| `Bool` | `Value::I64(0/1)` | 整数として表現 |
| `String` | `Value::Str(String)` | UTF-8 文字列 |
| `Char` | `Value::Char(char)` | Unicode 文字 |
| `Nothing` | `Value::Nothing` | 単位型 |
| `Complex{T}` | `Value::Struct(...)` | 複素数 |
| `Array` | `Value::Array(ArrayRef)` | 多次元配列 |
| `Tuple` | `Value::Tuple(Vec<Value>)` | タプル |
| `Dict` | `Value::Dict(DictValue)` | 辞書 |
| `Range` | `Value::Range(RangeValue)` | 範囲 |
| `DataType` | `Value::DataType(JuliaType)` | 型情報 |
| `Module` | `Value::Module(ModuleValue)` | モジュール |

### サブタイプルール

- `T <: Union{T1, T2}` ⟺ `T <: T1 || T <: T2`
- `Union{T1, T2} <: U` ⟺ `T1 <: U && T2 <: U`
- `Union{} <: T` for all T

---

## VM 命令セット

### スタック操作

| 命令 | 説明 |
|------|------|
| `PushI64(i64)` | 64bit 整数をプッシュ |
| `PushF64(f64)` | 64bit 浮動小数点をプッシュ |
| `PushStr(String)` | 文字列をプッシュ |
| `PushNil` | Nothing をプッシュ |
| `Pop` | スタックトップを破棄 |
| `Dup` | スタックトップを複製 |

### ローカル変数

| 命令 | 説明 |
|------|------|
| `Load(slot)` | ローカル変数ロード |
| `Store(slot)` | ローカル変数ストア |
| `LoadI64(slot)` | 整数変数ロード（最適化用）|

### 算術演算 (Intrinsics)

| 命令 | 説明 |
|------|------|
| `CallIntrinsic(AddInt)` | 整数加算 |
| `CallIntrinsic(SubInt)` | 整数減算 |
| `CallIntrinsic(MulInt)` | 整数乗算 |
| `CallIntrinsic(AddFloat)` | 浮動小数点加算 |
| `CallIntrinsic(DivFloat)` | 浮動小数点除算 |
| ... | 約50種のIntrinsics |

### 融合命令 (Peephole最適化)

| 命令 | 説明 |
|------|------|
| `LoadAddI64(slot, imm)` | Load + Add の融合 |
| `LoadSubI64(slot, imm)` | Load + Sub の融合 |
| `IncVarI64(slot)` | Load + Add + Store の融合 |
| `JumpIfNeI64(addr)` | Ne + JumpIfZero の融合 |
| ... | 計12個の融合命令 |

### 制御フロー

| 命令 | 説明 |
|------|------|
| `Jump(addr)` | 無条件ジャンプ |
| `JumpIfZero(addr)` | 条件ジャンプ |
| `Call(func_id, argc)` | ユーザー関数呼び出し |
| `Return` | 関数から戻る |
| `PushHandler` | 例外ハンドラ登録 |
| `PopHandler` | 例外ハンドラ解除 |

### 配列操作

| 命令 | 説明 |
|------|------|
| `ArrayNew(capacity)` | 配列作成 |
| `ArrayGet` | 要素取得 |
| `ArraySet` | 要素設定 |
| `ArrayLen` | 長さ取得 |
| `ArrayPush` | 末尾追加 |
| `ArrayPop` | 末尾削除 |
| `BroadcastBinOp` | ブロードキャスト演算 |

### 構造体操作

| 命令 | 説明 |
|------|------|
| `NewStruct(type_id)` | 構造体作成 |
| `GetField(field_idx)` | フィールド取得 |
| `SetField(field_idx)` | フィールド設定 |

---

## Builtin 関数

### 数学関数 (Pure Julia)

| 関数 | 説明 | 実装場所 |
|------|------|---------|
| `sin`, `cos`, `tan` | 三角関数 | base/special/trig.jl |
| `asin`, `acos`, `atan` | 逆三角関数 | base/special/trig.jl |
| `sinh`, `cosh`, `tanh` | 双曲線関数 | base/special/hyperbolic.jl |
| `exp`, `log`, `sqrt` | 指数・対数・平方根 | base/math.jl |
| `floor`, `ceil`, `round` | 丸め | base/floatfuncs.jl |
| `abs`, `sign` | 符号関連 | base/intfuncs.jl |
| `gcd`, `lcm` | 最大公約数・最小公倍数 | base/intfuncs.jl |

### 配列操作 (Rust Builtin)

| 関数 | 説明 | 実装 |
|------|------|------|
| `zeros`, `ones` | 配列作成 | Rust |
| `push!`, `pop!` | 末尾操作 | Rust |
| `length`, `size` | サイズ取得 | Rust |
| `reshape` | 形状変更 | Rust |

### 配列操作 (Pure Julia)

| 関数 | 説明 | 実装場所 |
|------|------|---------|
| `sum`, `prod` | 集約 | base/array.jl |
| `minimum`, `maximum` | 最小・最大 | base/array.jl |
| `reverse`, `sort` | 並べ替え | base/array.jl |
| `vcat`, `hcat` | 連結 | base/array.jl |
| `vec`, `axes` | 形状 | base/array.jl |
| `findfirst`, `findall` | 検索 | base/array.jl |

### 高階関数 (Rust Builtin)

| 関数 | 説明 |
|------|------|
| `map(f, arr)` | 関数適用 |
| `filter(f, arr)` | フィルタリング |
| `reduce(f, arr)` | リダクション |
| `foreach(f, arr)` | 反復実行 |
| `any(f, arr)`, `all(f, arr)` | 述語判定 |

### 統計関数 (stdlib/Statistics)

| 関数 | 説明 |
|------|------|
| `mean` | 平均 |
| `var`, `std` | 分散・標準偏差 |
| `median` | 中央値 |
| `cov`, `cor` | 共分散・相関 |
| `quantile` | 分位数 |

---

## 配列実装

### ArrayData enum

```rust
pub enum ArrayData {
    F32(Vec<f32>), F64(Vec<f64>),
    I8(Vec<i8>), I16(Vec<i16>), I32(Vec<i32>), I64(Vec<i64>),
    U8(Vec<u8>), U16(Vec<u16>), U32(Vec<u32>), U64(Vec<u64>),
    Bool(Vec<bool>),
    String(Vec<String>),
    Char(Vec<char>),
    StructRefs(Vec<usize>),
    Any(Vec<Value>),
}
```

### ArrayValue

```rust
pub struct ArrayValue {
    pub data: ArrayData,           // 型別ストレージ
    pub shape: Vec<usize>,         // 次元 [dim1, dim2, ...]
    pub struct_type_id: Option<usize>,
}

pub type ArrayRef = Rc<RefCell<ArrayValue>>;
```

### Column-Major Order

Julia と同じく列優先順序を使用：

```
2D行列 A[m, n] に対して:
Linear Index:  1    2    3    4    5    6
Array Index:  [1,1][2,1][1,2][2,2][1,3][2,3]

変換式 (1-indexed):
linear = i + (j-1)*m + (k-1)*m*n + ...
```

### Julia との比較

| Feature | Julia | SubsetJuliaVM |
|---------|-------|---------------|
| Memory model | GenericMemory + offset | Rust Vec |
| GC | Tracing GC | Rust ownership (Rc<RefCell>) |
| Growth strategy | Explicit overallocation | Rust Vec automatic |
| Offset support | Yes | No |
| Type-segregated | Yes | Yes |
| Column-major | Yes | Yes |

---

## Intrinsics

### 整数算術

| Intrinsic | 説明 |
|-----------|------|
| `NegInt` | 符号反転 |
| `AddInt` | 加算 |
| `SubInt` | 減算 |
| `MulInt` | 乗算 |
| `SdivInt` | 符号付き除算 |
| `SremInt` | 符号付き剰余 |

### 浮動小数点算術

| Intrinsic | 説明 |
|-----------|------|
| `NegFloat` | 符号反転 |
| `AddFloat` | 加算 |
| `SubFloat` | 減算 |
| `MulFloat` | 乗算 |
| `DivFloat` | 除算 |
| `PowFloat` | べき乗 |

### 比較

| Intrinsic | 説明 |
|-----------|------|
| `EqInt`, `NeInt` | 整数等価・非等価 |
| `SltInt`, `SleInt` | 符号付き less than/equal |
| `EqFloat`, `NeFloat` | 浮動小数点等価・非等価 |
| `LtFloat`, `LeFloat` | 浮動小数点 less than/equal |

### ビット演算

| Intrinsic | 説明 |
|-----------|------|
| `AndInt`, `OrInt`, `XorInt` | AND, OR, XOR |
| `NotInt` | NOT |
| `ShlInt`, `LshrInt`, `AshrInt` | シフト |

### 型変換

| Intrinsic | 説明 |
|-----------|------|
| `Sitofp` | 符号付き整数→浮動小数点 |
| `Fptosi` | 浮動小数点→符号付き整数 |

### 低レベル数学

| Intrinsic | 説明 |
|-----------|------|
| `SqrtLlvm` | 平方根 |
| `FloorLlvm` | 切り捨て |
| `CeilLlvm` | 切り上げ |
| `TruncLlvm` | ゼロ方向丸め |
| `AbsFloat` | 絶対値 |
| `CopysignFloat` | 符号コピー |

### 複素数

| Intrinsic | 説明 |
|-----------|------|
| `NegComplex` | 複素数符号反転 |
| `AddComplex`, `SubComplex` | 複素数加減算 |
| `MulComplex`, `DivComplex` | 複素数乗除算 |
| `EqComplex`, `NeComplex` | 複素数比較 |

---

## 参考資料

- [DONE.md](./DONE.md) - 実装済み機能一覧
- [STATUS.md](./STATUS.md) - 現状分析
- [DESIGN.md](./DESIGN.md) - 設計思想
- [archived/implementation_plans.md](./archived/implementation_plans.md) - 実装計画アーカイブ
