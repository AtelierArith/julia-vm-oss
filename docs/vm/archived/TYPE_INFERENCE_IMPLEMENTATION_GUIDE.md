# 型推論強化 実装ガイド

**作成日**: 2026-01-13
**ステータス**: 設計段階

> **Archive note (2026-06-11):** この設計ガイドは履歴として保持します。
> 現行ガイドは [TYPE_INFERENCE_COMPLETE.md](../TYPE_INFERENCE_COMPLETE.md)、
> 旧統合ドキュメントは [TYPE_INFERENCE_COMPLETE_20260116.md](TYPE_INFERENCE_COMPLETE_20260116.md)
> を参照してください。

## 概要

SubsetJuliaVM の型推論を Julia 本家のアプローチに基づいて強化するための詳細な実装ガイド。
既存のコードベースを最小限の変更で拡張し、段階的に導入可能な設計を提案する。

## 目次

1. [ディレクトリ構造](#ディレクトリ構造)
2. [モジュール依存関係](#モジュール依存関係)
3. [型定義](#型定義)
4. [型格子の実装](#型格子の実装)
5. [抽象解釈エンジン](#抽象解釈エンジン)
6. [転送関数](#転送関数)
7. [既存コードとの統合](#既存コードとの統合)
8. [テスト戦略](#テスト戦略)

---

## ディレクトリ構造

### 新規ファイル追加

```
subset_julia_vm_compile/src/compile/
├── mod.rs                    # 既存（修正）
├── types.rs                  # 既存
├── inference.rs              # 既存（統合用に修正）
├── context.rs                # 既存
├── stmt.rs                   # 既存
├── expr/
│   ├── mod.rs               # 既存
│   ├── infer.rs             # 既存（統合用に修正）
│   └── ...
│
├── lattice/                  # 【新規】型格子モジュール
│   ├── mod.rs               # モジュールエクスポート
│   ├── types.rs             # LatticeType, ConcreteType 定義
│   ├── ops.rs               # 格子演算（join, meet, is_subtype_of）
│   └── widening.rs          # 型拡大ロジック
│
├── abstract_interp/          # 【新規】抽象解釈エンジン
│   ├── mod.rs               # モジュールエクスポート
│   ├── engine.rs            # InferenceEngine（抽象解釈エンジン）
│   ├── env.rs               # 型環境管理
│   ├── conditional.rs       # 条件分岐での型絞り込み
│   └── loop_analysis.rs     # ループ変数の型推論
│
├── tfuncs/                   # 【新規】転送関数
│   ├── mod.rs               # モジュールエクスポート
│   ├── registry.rs          # 転送関数レジストリ
│   ├── arithmetic.rs        # 算術演算の返り値型
│   ├── array_ops.rs         # 配列操作の返り値型
│   ├── string_ops.rs        # 文字列操作の返り値型
│   └── intrinsics.rs        # Core.Intrinsics の返り値型
│
└── bridge.rs                 # 【新規】ValueType ↔ LatticeType 変換
```

### モジュール宣言の追加

```rust
// subset_julia_vm_compile/src/compile/mod.rs に追加

// 新規モジュール
pub mod lattice;
pub mod abstract_interp;
pub mod tfuncs;
mod bridge;

// 既存モジュール
mod types;
mod inference;
mod context;
mod stmt;
mod expr;
pub mod cache;
mod peephole;
mod type_helpers;
mod method_table;
```

---

## モジュール依存関係

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

---

## 型定義

### LatticeType（型格子要素）

```rust
// subset_julia_vm_compile/src/compile/lattice/types.rs

use std::collections::BTreeSet;

/// 型格子の要素
/// Julia の型システムを抽象化した表現
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LatticeType {
    /// 底型（空集合、到達不能コード）
    /// 例: throw() 後、無限ループ内など
    Bottom,

    /// 定数値型（コンパイル時定数）
    /// Concrete 型より具体的で、定数伝播に使用される
    /// 例: Const(42) は Concrete(Int64) より具体的
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

/// 定数値
/// コンパイル時に既知の値
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConstValue {
    /// 整数定数（64-bit signed）
    Int64(i64),
    /// 浮動小数点定数（64-bit）
    Float64(f64),
    /// 真偽値定数
    Bool(bool),
    /// 文字列定数
    String(String),
    /// Nothing 定数
    Nothing,
}

/// 具体型
/// ValueType との対応を保ちつつ、より詳細な型情報を表現
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConcreteType {
    // === プリミティブ型 ===
    /// 符号付き整数
    I8,
    I16,
    I32,
    I64,
    I128,
    BigInt,

    /// 符号なし整数
    U8,
    U16,
    U32,
    U64,
    U128,

    /// 浮動小数点数
    F32,
    F64,
    BigFloat,

    /// 真偽値
    Bool,

    /// 文字・文字列
    Char,
    String,

    /// 特殊型
    Nothing,
    Missing,
    Symbol,

    // === 複合型 ===
    /// 配列（要素型付き）
    Array {
        element: Box<ConcreteType>,
    },

    /// タプル（要素型リスト付き）
    Tuple {
        elements: Vec<ConcreteType>,
    },

    /// 名前付きタプル
    NamedTuple {
        fields: Vec<(String, ConcreteType)>,
    },

    /// 辞書
    Dict {
        key: Box<ConcreteType>,
        value: Box<ConcreteType>,
    },

    /// 範囲
    Range {
        element: Box<ConcreteType>,
    },

    /// ユーザー定義構造体
    Struct {
        /// 型ID（struct_table のキー）
        type_id: usize,
        /// 構造体名（デバッグ用）
        name: String,
    },

    /// DataType（型の型）
    DataType,

    /// Module
    Module,

    /// 関数型
    Function {
        /// 関数名（オプション）
        name: Option<String>,
    },
}

impl ConcreteType {
    /// ValueType から変換
    pub fn from_value_type(vt: &crate::vm::ValueType, struct_name: Option<&str>) -> Self {
        use crate::vm::ValueType;
        match vt {
            ValueType::I8 => ConcreteType::I8,
            ValueType::I16 => ConcreteType::I16,
            ValueType::I32 => ConcreteType::I32,
            ValueType::I64 => ConcreteType::I64,
            ValueType::I128 => ConcreteType::I128,
            ValueType::BigInt => ConcreteType::BigInt,
            ValueType::U8 => ConcreteType::U8,
            ValueType::U16 => ConcreteType::U16,
            ValueType::U32 => ConcreteType::U32,
            ValueType::U64 => ConcreteType::U64,
            ValueType::U128 => ConcreteType::U128,
            ValueType::F32 => ConcreteType::F32,
            ValueType::F64 => ConcreteType::F64,
            ValueType::BigFloat => ConcreteType::BigFloat,
            ValueType::Bool => ConcreteType::Bool,
            ValueType::Char => ConcreteType::Char,
            ValueType::Str => ConcreteType::String,
            ValueType::Nothing => ConcreteType::Nothing,
            ValueType::Missing => ConcreteType::Missing,
            ValueType::Symbol => ConcreteType::Symbol,
            ValueType::Struct(id) => ConcreteType::Struct {
                type_id: *id,
                name: struct_name.unwrap_or("?").to_string(),
            },
            ValueType::Array => ConcreteType::Array {
                element: Box::new(ConcreteType::F64), // デフォルト
            },
            ValueType::ArrayOf(elem) => ConcreteType::Array {
                element: Box::new(Self::from_array_element(elem)),
            },
            ValueType::Tuple => ConcreteType::Tuple { elements: vec![] },
            ValueType::Range => ConcreteType::Range {
                element: Box::new(ConcreteType::I64),
            },
            ValueType::Dict => ConcreteType::Dict {
                key: Box::new(ConcreteType::String),
                value: Box::new(ConcreteType::F64), // デフォルト
            },
            ValueType::DataType => ConcreteType::DataType,
            ValueType::Module => ConcreteType::Module,
            _ => ConcreteType::F64, // フォールバック
        }
    }

    fn from_array_element(elem: &crate::vm::ArrayElementType) -> ConcreteType {
        use crate::vm::ArrayElementType;
        match elem {
            ArrayElementType::I64 => ConcreteType::I64,
            ArrayElementType::F64 => ConcreteType::F64,
            ArrayElementType::Bool => ConcreteType::Bool,
            ArrayElementType::String => ConcreteType::String,
            ArrayElementType::Char => ConcreteType::Char,
            ArrayElementType::StructOf(id) => ConcreteType::Struct {
                type_id: *id,
                name: "?".to_string(),
            },
            _ => ConcreteType::F64,
        }
    }

    /// 数値型かどうか
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            ConcreteType::I8 | ConcreteType::I16 | ConcreteType::I32 |
            ConcreteType::I64 | ConcreteType::I128 | ConcreteType::BigInt |
            ConcreteType::U8 | ConcreteType::U16 | ConcreteType::U32 |
            ConcreteType::U64 | ConcreteType::U128 |
            ConcreteType::F32 | ConcreteType::F64 | ConcreteType::BigFloat
        )
    }

    /// 整数型かどうか
    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            ConcreteType::I8 | ConcreteType::I16 | ConcreteType::I32 |
            ConcreteType::I64 | ConcreteType::I128 | ConcreteType::BigInt |
            ConcreteType::U8 | ConcreteType::U16 | ConcreteType::U32 |
            ConcreteType::U64 | ConcreteType::U128
        )
    }

    /// 浮動小数点型かどうか
    pub fn is_float(&self) -> bool {
        matches!(
            self,
            ConcreteType::F32 | ConcreteType::F64 | ConcreteType::BigFloat
        )
    }
}

impl Default for LatticeType {
    fn default() -> Self {
        LatticeType::Top
    }
}
```

### 型格子の定数

```rust
// subset_julia_vm_compile/src/compile/lattice/widening.rs

/// Union 型の最大要素数
/// これを超えると Top に拡大
pub const MAX_UNION_LENGTH: usize = 4;

/// Union 型の最大ネスト深度
/// Array{Union{Int, Float}} のようなネストをカウント
pub const MAX_UNION_COMPLEXITY: usize = 3;

/// 不動点計算の最大反復回数
pub const MAX_ITERATIONS: usize = 100;
```

---

## 型格子の実装

### 格子演算

```rust
// subset_julia_vm_compile/src/compile/lattice/ops.rs

use super::types::{ConcreteType, LatticeType};
use super::widening::{MAX_UNION_LENGTH, MAX_UNION_COMPLEXITY};
use std::collections::BTreeSet;

impl LatticeType {
    /// 型の結合（join, ⊔）
    /// 2つの型の最小上界を計算
    ///
    /// # Examples
    /// ```
    /// Int64.join(Float64) = Union{Int64, Float64}
    /// Int64.join(Int64) = Int64
    /// Bottom.join(T) = T
    /// T.join(Top) = Top
    /// ```
    pub fn join(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Bottom は単位元
            (LatticeType::Bottom, t) | (t, LatticeType::Bottom) => t.clone(),

            // Top は吸収元
            (LatticeType::Top, _) | (_, LatticeType::Top) => LatticeType::Top,

            // 同一の具体型
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
                LatticeType::Concrete(a.clone())
            }

            // 異なる具体型 → Union
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                let mut set = BTreeSet::new();
                set.insert(a.clone());
                set.insert(b.clone());
                Self::simplify_union(set)
            }

            // Union + Concrete
            (LatticeType::Union(us), LatticeType::Concrete(c)) |
            (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                let mut new_set = us.clone();
                new_set.insert(c.clone());
                Self::simplify_union(new_set)
            }

            // Union + Union
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let combined: BTreeSet<_> = a.union(b).cloned().collect();
                Self::simplify_union(combined)
            }

            // Conditional は保守的に Top
            (LatticeType::Conditional { .. }, _) |
            (_, LatticeType::Conditional { .. }) => LatticeType::Top,
        }
    }

    /// 型の交差（meet, ⊓）
    /// 2つの型の最大下界を計算
    ///
    /// # Examples
    /// ```
    /// Int64.meet(Float64) = Bottom
    /// Int64.meet(Int64) = Int64
    /// Union{Int, Float}.meet(Int) = Int
    /// ```
    pub fn meet(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            // Top は単位元
            (LatticeType::Top, t) | (t, LatticeType::Top) => t.clone(),

            // Bottom は吸収元
            (LatticeType::Bottom, _) | (_, LatticeType::Bottom) => LatticeType::Bottom,

            // 同一の具体型
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) if a == b => {
                LatticeType::Concrete(a.clone())
            }

            // 異なる具体型 → Bottom
            (LatticeType::Concrete(_), LatticeType::Concrete(_)) => LatticeType::Bottom,

            // Union と Concrete の交差
            (LatticeType::Union(us), LatticeType::Concrete(c)) |
            (LatticeType::Concrete(c), LatticeType::Union(us)) => {
                if us.contains(c) {
                    LatticeType::Concrete(c.clone())
                } else {
                    LatticeType::Bottom
                }
            }

            // Union 同士の交差
            (LatticeType::Union(a), LatticeType::Union(b)) => {
                let intersection: BTreeSet<_> = a.intersection(b).cloned().collect();
                if intersection.is_empty() {
                    LatticeType::Bottom
                } else if intersection.len() == 1 {
                    LatticeType::Concrete(intersection.into_iter().next().unwrap())
                } else {
                    LatticeType::Union(intersection)
                }
            }

            // Conditional は保守的に処理
            _ => LatticeType::Bottom,
        }
    }

    /// 型の包含関係（⊑）
    /// self が other の部分型かどうか
    ///
    /// # Examples
    /// ```
    /// Bottom ⊑ T (for all T)
    /// T ⊑ Top (for all T)
    /// Int64 ⊑ Union{Int64, Float64}
    /// Int64 ⊑ Int64
    /// ```
    pub fn is_subtype_of(&self, other: &LatticeType) -> bool {
        match (self, other) {
            // Bottom は全ての型の部分型
            (LatticeType::Bottom, _) => true,

            // 全ての型は Top の部分型
            (_, LatticeType::Top) => true,

            // Top は Bottom 以外の部分型ではない
            (LatticeType::Top, _) => false,

            // Concrete 同士
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => a == b,

            // Concrete は Union の部分型（要素に含まれる場合）
            (LatticeType::Concrete(c), LatticeType::Union(us)) => us.contains(c),

            // Union は Union の部分型（全要素が含まれる場合）
            (LatticeType::Union(a), LatticeType::Union(b)) => a.is_subset(b),

            // Union は Concrete の部分型ではない（単一要素でも）
            (LatticeType::Union(_), LatticeType::Concrete(_)) => false,

            // Conditional は保守的
            _ => false,
        }
    }

    /// Union の簡約と拡大
    fn simplify_union(types: BTreeSet<ConcreteType>) -> LatticeType {
        if types.is_empty() {
            return LatticeType::Bottom;
        }

        if types.len() == 1 {
            return LatticeType::Concrete(types.into_iter().next().unwrap());
        }

        // 最大要素数を超えたら拡大
        if types.len() > MAX_UNION_LENGTH {
            return Self::widen_union(&types);
        }

        // 複雑度チェック（ネストした型の深さ）
        let complexity = Self::compute_complexity(&types);
        if complexity > MAX_UNION_COMPLEXITY {
            return Self::widen_union(&types);
        }

        LatticeType::Union(types)
    }

    /// Union の拡大（widening）
    fn widen_union(types: &BTreeSet<ConcreteType>) -> LatticeType {
        // 全て数値型なら Number へ
        if types.iter().all(|t| t.is_numeric()) {
            // 整数と浮動小数点の混合
            let has_int = types.iter().any(|t| t.is_integer());
            let has_float = types.iter().any(|t| t.is_float());

            if has_int && has_float {
                // Int + Float → Union{Int64, Float64} に正規化
                let mut normalized = BTreeSet::new();
                normalized.insert(ConcreteType::I64);
                normalized.insert(ConcreteType::F64);
                return LatticeType::Union(normalized);
            }
        }

        // それ以外は Top
        LatticeType::Top
    }

    /// Union の複雑度を計算
    fn compute_complexity(types: &BTreeSet<ConcreteType>) -> usize {
        types.iter().map(|t| Self::type_depth(t)).max().unwrap_or(0)
    }

    /// 型の深さを計算
    fn type_depth(ty: &ConcreteType) -> usize {
        match ty {
            ConcreteType::Array { element } => 1 + Self::type_depth(element),
            ConcreteType::Tuple { elements } => {
                1 + elements.iter().map(Self::type_depth).max().unwrap_or(0)
            }
            ConcreteType::Dict { key, value } => {
                1 + Self::type_depth(key).max(Self::type_depth(value))
            }
            _ => 1,
        }
    }

    /// 型の差（差集合）
    /// Conditional 型生成時に使用
    pub fn subtract(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            (LatticeType::Union(us), LatticeType::Concrete(c)) => {
                let remaining: BTreeSet<_> = us.iter()
                    .filter(|t| *t != c)
                    .cloned()
                    .collect();
                Self::simplify_union(remaining)
            }
            (t, LatticeType::Concrete(_)) if t == other => LatticeType::Bottom,
            _ => self.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_concrete() {
        let int = LatticeType::Concrete(ConcreteType::I64);
        let float = LatticeType::Concrete(ConcreteType::F64);

        let joined = int.join(&float);
        assert!(matches!(joined, LatticeType::Union(_)));
    }

    #[test]
    fn test_join_with_bottom() {
        let int = LatticeType::Concrete(ConcreteType::I64);
        let bottom = LatticeType::Bottom;

        assert_eq!(int.join(&bottom), int);
        assert_eq!(bottom.join(&int), int);
    }

    #[test]
    fn test_is_subtype_of() {
        let int = LatticeType::Concrete(ConcreteType::I64);
        let top = LatticeType::Top;
        let bottom = LatticeType::Bottom;

        assert!(bottom.is_subtype_of(&int));
        assert!(int.is_subtype_of(&top));
        assert!(!top.is_subtype_of(&int));
    }
}
```

---

## 抽象解釈エンジン

**実装状況**: ✅ 実装済み

抽象解釈エンジンは `subset_julia_vm_compile/src/compile/abstract_interp/engine.rs` に `InferenceEngine` として実装されています。

**主な機能**:
- 不動点反復による型推論
- ループ変数の型推論
- 条件分岐での型絞り込み（環境分割アプローチ）
- 転送関数による組み込み関数の返り値型推論
- 関数返り値型のキャッシュ

**使用箇所**: `subset_julia_vm_compile/src/compile/mod.rs:1265` で `infer_function_return_type_v2` が呼び出されています。

### 型環境

```rust
// subset_julia_vm_compile/src/compile/abstract_interp/env.rs

use std::collections::HashMap;
use crate::compile::lattice::types::LatticeType;

/// 型環境
/// 変数名から型へのマッピング
#[derive(Clone, Debug, Default)]
pub struct TypeEnv {
    /// ローカル変数の型
    locals: HashMap<String, LatticeType>,

    /// グローバル変数の型（読み取り専用）
    globals: HashMap<String, LatticeType>,

    /// 保護された変数（関数パラメータなど、型が変更されない）
    protected: std::collections::HashSet<String>,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self::default()
    }

    /// グローバル変数を設定
    pub fn set_globals(&mut self, globals: HashMap<String, LatticeType>) {
        self.globals = globals;
    }

    /// 保護変数を追加
    pub fn protect(&mut self, name: &str) {
        self.protected.insert(name.to_string());
    }

    /// 変数の型を取得
    pub fn get(&self, name: &str) -> Option<&LatticeType> {
        self.locals.get(name).or_else(|| self.globals.get(name))
    }

    /// 変数の型を設定（保護変数は無視）
    pub fn set(&mut self, name: &str, ty: LatticeType) -> bool {
        if self.protected.contains(name) {
            return false;
        }

        match self.locals.get(name) {
            Some(old) => {
                let joined = old.join(&ty);
                if &joined != old {
                    self.locals.insert(name.to_string(), joined);
                    true
                } else {
                    false
                }
            }
            None => {
                self.locals.insert(name.to_string(), ty);
                true
            }
        }
    }

    /// 環境を複製
    pub fn clone_env(&self) -> Self {
        Self {
            locals: self.locals.clone(),
            globals: self.globals.clone(),
            protected: self.protected.clone(),
        }
    }

    /// 2つの環境をマージ（join）
    pub fn merge(&self, other: &TypeEnv) -> TypeEnv {
        let mut result = self.clone_env();

        for (name, ty) in &other.locals {
            match result.locals.get(name) {
                Some(existing) => {
                    result.locals.insert(name.clone(), existing.join(ty));
                }
                None => {
                    result.locals.insert(name.clone(), ty.clone());
                }
            }
        }

        result
    }

    /// 環境が等しいか（不動点チェック用）
    pub fn equals(&self, other: &TypeEnv) -> bool {
        if self.locals.len() != other.locals.len() {
            return false;
        }

        for (name, ty) in &self.locals {
            match other.locals.get(name) {
                Some(other_ty) if ty == other_ty => continue,
                _ => return false,
            }
        }

        true
    }

    /// ローカル変数のイテレータ
    pub fn iter_locals(&self) -> impl Iterator<Item = (&String, &LatticeType)> {
        self.locals.iter()
    }
}
```

### 抽象解釈エンジン本体

```rust
// subset_julia_vm_compile/src/compile/abstract_interp/engine.rs

use std::collections::HashMap;
use crate::ir::core::{Expr, Stmt, Block, BinaryOp, UnaryOp, Literal};
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::lattice::widening::MAX_ITERATIONS;
use crate::compile::tfuncs::registry::TransferFunctions;
use super::env::TypeEnv;
use super::conditional::narrow_by_condition;
use super::loop_analysis::infer_iterator_element_type;

/// 抽象解釈による型推論エンジン
/// 実装では `InferenceEngine` という名前で実装されています
pub struct InferenceEngine {
    /// 転送関数テーブル
    tfuncs: TransferFunctions,

    /// 関数の返り値型キャッシュ
    return_type_cache: HashMap<String, LatticeType>,

    /// 構造体情報
    struct_table: HashMap<String, StructTypeInfo>,
}

/// 構造体の型情報
#[derive(Clone, Debug)]
pub struct StructTypeInfo {
    pub type_id: usize,
    pub name: String,
    pub fields: Vec<(String, LatticeType)>,
}

impl InferenceEngine {
    pub fn new() -> Self {
        Self::with_struct_table(HashMap::new())
    }

    pub fn with_struct_table(struct_table: HashMap<String, StructTypeInfo>) -> Self {
        let mut tfuncs = TransferFunctions::new();
        crate::compile::tfuncs::register_all(&mut tfuncs);

        Self {
            tfuncs,
            return_type_cache: HashMap::new(),
            struct_table,
        }
    }

    /// 構造体情報を設定
    pub fn set_struct_table(&mut self, table: HashMap<String, StructTypeInfo>) {
        self.struct_table = table;
    }

    /// 関数本体の型推論
    pub fn infer_function(
        &mut self,
        params: &[(String, Option<LatticeType>)],
        body: &Block,
    ) -> LatticeType {
        let mut env = TypeEnv::new();

        // パラメータの型を環境に追加
        for (name, ty) in params {
            let param_type = ty.clone().unwrap_or(LatticeType::Top);
            env.set(name, param_type);
            env.protect(name);
        }

        // 不動点計算
        let mut iterations = 0;
        loop {
            let old_env = env.clone_env();
            self.analyze_block(&mut env, body);
            iterations += 1;

            if env.equals(&old_env) || iterations >= MAX_ITERATIONS {
                break;
            }
        }

        // 返り値型を収集
        self.collect_return_type(&env, body)
    }

    /// ブロックの解析
    fn analyze_block(&mut self, env: &mut TypeEnv, block: &Block) {
        for stmt in &block.stmts {
            self.analyze_stmt(env, stmt);
        }
    }

    /// 文の解析
    fn analyze_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) {
        match stmt {
            Stmt::Assign { var, value, .. } => {
                let value_type = self.infer_expr_type(env, value);
                env.set(var, value_type);
            }

            Stmt::If { cond, then_branch, else_branch, .. } => {
                // 条件による型絞り込み
                let (then_env, else_env) = narrow_by_condition(env, cond);

                // then 分岐の解析
                let mut then_env = then_env;
                self.analyze_block(&mut then_env, then_branch);

                // else 分岐の解析
                let mut else_env = else_env;
                if let Some(else_block) = else_branch {
                    self.analyze_block(&mut else_env, else_block);
                }

                // 環境のマージ
                *env = then_env.merge(&else_env);
            }

            Stmt::For { var, start, end, body, .. } => {
                // ループ変数は整数
                env.set(var, LatticeType::Concrete(ConcreteType::I64));

                // ループ本体の解析（複数回）
                for _ in 0..3 {
                    self.analyze_block(env, body);
                }
            }

            Stmt::ForEach { var, iter, body, .. } => {
                // イテレータの要素型を推論
                let iter_type = self.infer_expr_type(env, iter);
                let elem_type = infer_iterator_element_type(&iter_type, &self.struct_table);
                env.set(var, elem_type);

                // ループ本体の解析
                for _ in 0..3 {
                    self.analyze_block(env, body);
                }
            }

            Stmt::While { cond, body, .. } => {
                // while ループの解析
                for _ in 0..3 {
                    // 条件による型絞り込み（オプション）
                    let (loop_env, _) = narrow_by_condition(env, cond);
                    *env = loop_env;
                    self.analyze_block(env, body);
                }
            }

            Stmt::Return { value, .. } => {
                // 返り値の型は別途収集
                if let Some(expr) = value {
                    let _ = self.infer_expr_type(env, expr);
                }
            }

            Stmt::Expr { expr, .. } => {
                let _ = self.infer_expr_type(env, expr);
            }

            Stmt::Try { try_block, catch_block, finally_block, .. } => {
                // try ブロックの解析
                let mut try_env = env.clone_env();
                self.analyze_block(&mut try_env, try_block);

                // catch ブロックの解析
                if let Some(catch) = catch_block {
                    let mut catch_env = env.clone_env();
                    self.analyze_block(&mut catch_env, catch);
                    try_env = try_env.merge(&catch_env);
                }

                // finally ブロックの解析
                if let Some(finally) = finally_block {
                    self.analyze_block(&mut try_env, finally);
                }

                *env = try_env;
            }

            _ => {}
        }
    }

    /// 式の型推論
    pub fn infer_expr_type(&self, env: &TypeEnv, expr: &Expr) -> LatticeType {
        match expr {
            // リテラル
            Expr::Literal(lit, _) => self.infer_literal_type(lit),

            // 変数
            Expr::Var(name, _) => {
                env.get(name).cloned().unwrap_or(LatticeType::Top)
            }

            // 二項演算
            Expr::BinaryOp { op, left, right, .. } => {
                let left_ty = self.infer_expr_type(env, left);
                let right_ty = self.infer_expr_type(env, right);
                self.infer_binary_op_type(op, &left_ty, &right_ty)
            }

            // 単項演算
            Expr::UnaryOp { op, operand, .. } => {
                let operand_ty = self.infer_expr_type(env, operand);
                self.infer_unary_op_type(op, &operand_ty)
            }

            // 関数呼び出し
            Expr::Call { function, args, .. } => {
                let arg_types: Vec<_> = args.iter()
                    .map(|a| self.infer_expr_type(env, a))
                    .collect();

                // 構造体コンストラクタチェック
                if let Some(struct_info) = self.struct_table.get(function) {
                    return LatticeType::Concrete(ConcreteType::Struct {
                        type_id: struct_info.type_id,
                        name: struct_info.name.clone(),
                    });
                }

                // 転送関数で返り値型を取得
                self.tfuncs.infer_return_type(function, &arg_types)
            }

            // 配列リテラル
            Expr::ArrayLiteral { elements, .. } => {
                if elements.is_empty() {
                    return LatticeType::Concrete(ConcreteType::Array {
                        element: Box::new(ConcreteType::F64),
                    });
                }

                // 要素型を推論
                let elem_types: Vec<_> = elements.iter()
                    .map(|e| self.infer_expr_type(env, e))
                    .collect();

                // 全要素の型を結合
                let unified = elem_types.iter()
                    .fold(LatticeType::Bottom, |acc, t| acc.join(t));

                match unified {
                    LatticeType::Concrete(elem) => {
                        LatticeType::Concrete(ConcreteType::Array {
                            element: Box::new(elem),
                        })
                    }
                    _ => LatticeType::Concrete(ConcreteType::Array {
                        element: Box::new(ConcreteType::F64),
                    }),
                }
            }

            // タプルリテラル
            Expr::TupleLiteral { elements, .. } => {
                let elem_types: Vec<_> = elements.iter()
                    .map(|e| {
                        match self.infer_expr_type(env, e) {
                            LatticeType::Concrete(c) => c,
                            _ => ConcreteType::F64,
                        }
                    })
                    .collect();

                LatticeType::Concrete(ConcreteType::Tuple {
                    elements: elem_types,
                })
            }

            // インデックスアクセス
            Expr::Index { array, indices, .. } => {
                let array_ty = self.infer_expr_type(env, array);

                // スライスかどうかチェック
                let is_slice = indices.iter().any(|idx| {
                    matches!(idx, Expr::Range { .. } | Expr::SliceAll { .. })
                });

                if is_slice {
                    // スライスは配列型を保持
                    array_ty
                } else {
                    // 要素アクセス
                    match array_ty {
                        LatticeType::Concrete(ConcreteType::Array { element }) => {
                            LatticeType::Concrete(*element)
                        }
                        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
                            // インデックスが定数なら対応する要素型
                            if let Some(Expr::Literal(Literal::Int(idx), _)) = indices.first() {
                                let idx = (*idx as usize).saturating_sub(1); // 1-indexed
                                if idx < elements.len() {
                                    return LatticeType::Concrete(elements[idx].clone());
                                }
                            }
                            // 不明なインデックスは要素型の Union
                            if elements.is_empty() {
                                LatticeType::Top
                            } else {
                                let mut result = LatticeType::Concrete(elements[0].clone());
                                for elem in &elements[1..] {
                                    result = result.join(&LatticeType::Concrete(elem.clone()));
                                }
                                result
                            }
                        }
                        LatticeType::Concrete(ConcreteType::String) => {
                            LatticeType::Concrete(ConcreteType::Char)
                        }
                        _ => LatticeType::Top,
                    }
                }
            }

            // フィールドアクセス
            Expr::FieldAccess { object, field, .. } => {
                let obj_ty = self.infer_expr_type(env, object);

                match obj_ty {
                    LatticeType::Concrete(ConcreteType::Struct { type_id, .. }) => {
                        // 構造体テーブルからフィールド型を取得
                        for (_, info) in &self.struct_table {
                            if info.type_id == type_id {
                                for (fname, ftype) in &info.fields {
                                    if fname == field {
                                        return ftype.clone();
                                    }
                                }
                            }
                        }
                        LatticeType::Top
                    }
                    _ => LatticeType::Top,
                }
            }

            // 三項演算子
            Expr::Ternary { then_expr, else_expr, .. } => {
                let then_ty = self.infer_expr_type(env, then_expr);
                let else_ty = self.infer_expr_type(env, else_expr);
                then_ty.join(&else_ty)
            }

            // Range
            Expr::Range { start, .. } => {
                let start_ty = start.as_ref()
                    .map(|s| self.infer_expr_type(env, s))
                    .unwrap_or(LatticeType::Concrete(ConcreteType::I64));

                match start_ty {
                    LatticeType::Concrete(elem) => {
                        LatticeType::Concrete(ConcreteType::Range {
                            element: Box::new(elem),
                        })
                    }
                    _ => LatticeType::Concrete(ConcreteType::Range {
                        element: Box::new(ConcreteType::I64),
                    }),
                }
            }

            // その他
            _ => LatticeType::Top,
        }
    }

    /// リテラルの型推論
    fn infer_literal_type(&self, lit: &Literal) -> LatticeType {
        match lit {
            Literal::Int(_) => LatticeType::Concrete(ConcreteType::I64),
            Literal::Int128(_) => LatticeType::Concrete(ConcreteType::I128),
            Literal::BigInt(_) => LatticeType::Concrete(ConcreteType::BigInt),
            Literal::Float(_) => LatticeType::Concrete(ConcreteType::F64),
            Literal::Float32(_) => LatticeType::Concrete(ConcreteType::F32),
            Literal::BigFloat(_) => LatticeType::Concrete(ConcreteType::BigFloat),
            Literal::Bool(_) => LatticeType::Concrete(ConcreteType::Bool),
            Literal::Str(_) => LatticeType::Concrete(ConcreteType::String),
            Literal::Char(_) => LatticeType::Concrete(ConcreteType::Char),
            Literal::Nothing => LatticeType::Concrete(ConcreteType::Nothing),
            Literal::Missing => LatticeType::Concrete(ConcreteType::Missing),
            Literal::Symbol(_) => LatticeType::Concrete(ConcreteType::Symbol),
            Literal::Struct(name, _) => {
                if let Some(info) = self.struct_table.get(name) {
                    LatticeType::Concrete(ConcreteType::Struct {
                        type_id: info.type_id,
                        name: info.name.clone(),
                    })
                } else {
                    LatticeType::Top
                }
            }
            _ => LatticeType::Top,
        }
    }

    /// 二項演算の型推論
    fn infer_binary_op_type(
        &self,
        op: &BinaryOp,
        left: &LatticeType,
        right: &LatticeType,
    ) -> LatticeType {
        match op {
            // 比較演算子は Bool
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le |
            BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne => {
                LatticeType::Concrete(ConcreteType::Bool)
            }

            // 論理演算子は Bool
            BinaryOp::And | BinaryOp::Or => {
                LatticeType::Concrete(ConcreteType::Bool)
            }

            // 除算は Float64
            BinaryOp::Div => {
                LatticeType::Concrete(ConcreteType::F64)
            }

            // べき乗
            BinaryOp::Pow => {
                match (left, right) {
                    (
                        LatticeType::Concrete(l),
                        LatticeType::Concrete(r),
                    ) if l.is_integer() && r.is_integer() => {
                        LatticeType::Concrete(ConcreteType::I64)
                    }
                    _ => LatticeType::Concrete(ConcreteType::F64),
                }
            }

            // その他の算術演算
            _ => {
                match (left, right) {
                    // 両方 Int → Int
                    (
                        LatticeType::Concrete(l),
                        LatticeType::Concrete(r),
                    ) if l.is_integer() && r.is_integer() => {
                        LatticeType::Concrete(ConcreteType::I64)
                    }
                    // 少なくとも一方が Float → Float
                    (
                        LatticeType::Concrete(l),
                        LatticeType::Concrete(r),
                    ) if l.is_float() || r.is_float() => {
                        LatticeType::Concrete(ConcreteType::F64)
                    }
                    // 構造体が含まれる場合は保守的
                    (
                        LatticeType::Concrete(ConcreteType::Struct { .. }),
                        _,
                    ) |
                    (
                        _,
                        LatticeType::Concrete(ConcreteType::Struct { .. }),
                    ) => {
                        // 構造体の演算結果は構造体型を保持することが多い
                        left.clone()
                    }
                    _ => LatticeType::Top,
                }
            }
        }
    }

    /// 単項演算の型推論
    fn infer_unary_op_type(&self, op: &UnaryOp, operand: &LatticeType) -> LatticeType {
        match op {
            UnaryOp::Not => LatticeType::Concrete(ConcreteType::Bool),
            UnaryOp::Neg | UnaryOp::Pos => operand.clone(),
        }
    }

    /// 返り値型の収集
    fn collect_return_type(&self, env: &TypeEnv, block: &Block) -> LatticeType {
        let mut return_types = vec![];
        self.collect_return_types_recursive(&block.stmts, env, &mut return_types);

        if return_types.is_empty() {
            // 暗黙の返り値（最後の式）
            if let Some(Stmt::Expr { expr, .. }) = block.stmts.last() {
                return self.infer_expr_type(env, expr);
            }
            return LatticeType::Concrete(ConcreteType::Nothing);
        }

        // 全返り値型の結合
        return_types.iter()
            .fold(LatticeType::Bottom, |acc, t| acc.join(t))
    }

    fn collect_return_types_recursive(
        &self,
        stmts: &[Stmt],
        env: &TypeEnv,
        result: &mut Vec<LatticeType>,
    ) {
        for stmt in stmts {
            match stmt {
                Stmt::Return { value: Some(expr), .. } => {
                    result.push(self.infer_expr_type(env, expr));
                }
                Stmt::Return { value: None, .. } => {
                    result.push(LatticeType::Concrete(ConcreteType::Nothing));
                }
                Stmt::If { then_branch, else_branch, .. } => {
                    self.collect_return_types_recursive(&then_branch.stmts, env, result);
                    if let Some(else_block) = else_branch {
                        self.collect_return_types_recursive(&else_block.stmts, env, result);
                    }
                }
                Stmt::For { body, .. } |
                Stmt::ForEach { body, .. } |
                Stmt::While { body, .. } => {
                    self.collect_return_types_recursive(&body.stmts, env, result);
                }
                Stmt::Try { try_block, catch_block, .. } => {
                    self.collect_return_types_recursive(&try_block.stmts, env, result);
                    if let Some(catch) = catch_block {
                        self.collect_return_types_recursive(&catch.stmts, env, result);
                    }
                }
                _ => {}
            }
        }
    }
}
```

### 条件分岐での型絞り込み

```rust
// subset_julia_vm_compile/src/compile/abstract_interp/conditional.rs

use crate::ir::core::{Expr, Literal};
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use super::env::TypeEnv;

/// 条件式による環境の分割
/// if x isa T のような条件で then/else 分岐の型環境を絞り込む
pub fn narrow_by_condition(
    env: &TypeEnv,
    cond: &Expr,
) -> (TypeEnv, TypeEnv) {
    let mut then_env = env.clone_env();
    let mut else_env = env.clone_env();

    match cond {
        // x isa T パターン
        Expr::Call { function, args, .. } if function == "isa" && args.len() == 2 => {
            if let Expr::Var(var_name, _) = &args[0] {
                if let Some(type_const) = extract_type_from_expr(&args[1]) {
                    // then: x は T 型
                    then_env.set(var_name, type_const.clone());

                    // else: x は T 以外
                    if let Some(current) = env.get(var_name) {
                        let narrowed = current.subtract(&type_const);
                        let _ = else_env.set(var_name, narrowed);
                    }
                }
            }
        }

        // x === nothing パターン
        Expr::BinaryOp { op: crate::ir::core::BinaryOp::Eq, left, right, .. } => {
            match (left.as_ref(), right.as_ref()) {
                (Expr::Var(name, _), Expr::Literal(Literal::Nothing, _)) |
                (Expr::Literal(Literal::Nothing, _), Expr::Var(name, _)) => {
                    // then: x は Nothing
                    then_env.set(name, LatticeType::Concrete(ConcreteType::Nothing));

                    // else: x は Nothing 以外
                    if let Some(current) = env.get(name) {
                        let narrowed = current.subtract(
                            &LatticeType::Concrete(ConcreteType::Nothing)
                        );
                        let _ = else_env.set(name, narrowed);
                    }
                }
                _ => {}
            }
        }

        // x !== nothing パターン
        Expr::BinaryOp { op: crate::ir::core::BinaryOp::Ne, left, right, .. } => {
            match (left.as_ref(), right.as_ref()) {
                (Expr::Var(name, _), Expr::Literal(Literal::Nothing, _)) |
                (Expr::Literal(Literal::Nothing, _), Expr::Var(name, _)) => {
                    // then: x は Nothing 以外
                    if let Some(current) = env.get(name) {
                        let narrowed = current.subtract(
                            &LatticeType::Concrete(ConcreteType::Nothing)
                        );
                        then_env.set(name, narrowed);
                    }

                    // else: x は Nothing
                    else_env.set(name, LatticeType::Concrete(ConcreteType::Nothing));
                }
                _ => {}
            }
        }

        // && 条件の処理
        Expr::BinaryOp { op: crate::ir::core::BinaryOp::And, left, right, .. } => {
            // 左条件で絞り込んでから右条件
            let (left_then, left_else) = narrow_by_condition(env, left);
            let (combined_then, _) = narrow_by_condition(&left_then, right);

            then_env = combined_then;
            else_env = left_else;
        }

        // || 条件の処理
        Expr::BinaryOp { op: crate::ir::core::BinaryOp::Or, left, right, .. } => {
            let (left_then, left_else) = narrow_by_condition(env, left);
            let (right_then, right_else) = narrow_by_condition(&left_else, right);

            // then: 左が真 OR (左が偽 AND 右が真)
            then_env = left_then.merge(&right_then);
            // else: 両方偽
            else_env = right_else;
        }

        // ! 条件の処理
        Expr::UnaryOp { op: crate::ir::core::UnaryOp::Not, operand, .. } => {
            let (inner_then, inner_else) = narrow_by_condition(env, operand);
            // 反転
            then_env = inner_else;
            else_env = inner_then;
        }

        _ => {
            // その他の条件は絞り込みなし
        }
    }

    (then_env, else_env)
}

/// 式から型を抽出
fn extract_type_from_expr(expr: &Expr) -> Option<LatticeType> {
    match expr {
        Expr::Var(name, _) => {
            // 型名の解釈
            match name.as_str() {
                "Int" | "Int64" => Some(LatticeType::Concrete(ConcreteType::I64)),
                "Int32" => Some(LatticeType::Concrete(ConcreteType::I32)),
                "Int128" => Some(LatticeType::Concrete(ConcreteType::I128)),
                "Float64" => Some(LatticeType::Concrete(ConcreteType::F64)),
                "Float32" => Some(LatticeType::Concrete(ConcreteType::F32)),
                "Bool" => Some(LatticeType::Concrete(ConcreteType::Bool)),
                "String" => Some(LatticeType::Concrete(ConcreteType::String)),
                "Char" => Some(LatticeType::Concrete(ConcreteType::Char)),
                "Nothing" => Some(LatticeType::Concrete(ConcreteType::Nothing)),
                "Symbol" => Some(LatticeType::Concrete(ConcreteType::Symbol)),
                _ => None, // ユーザー定義型は struct_table を参照する必要あり
            }
        }
        _ => None,
    }
}
```

### ループ変数の型推論

```rust
// subset_julia_vm_compile/src/compile/abstract_interp/loop_analysis.rs

use std::collections::HashMap;
use crate::compile::lattice::types::{ConcreteType, LatticeType};
use crate::compile::abstract_interp::engine::StructTypeInfo;

/// イテレータの要素型を推論
pub fn infer_iterator_element_type(
    iter_type: &LatticeType,
    struct_table: &HashMap<String, StructTypeInfo>,
) -> LatticeType {
    match iter_type {
        // Array{T} → T
        LatticeType::Concrete(ConcreteType::Array { element }) => {
            LatticeType::Concrete((**element).clone())
        }

        // Tuple{T1, T2, ...} → Union{T1, T2, ...}（全要素同じなら T）
        LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
            if elements.is_empty() {
                return LatticeType::Bottom;
            }

            // 全要素が同じ型かチェック
            if elements.iter().all(|e| e == &elements[0]) {
                return LatticeType::Concrete(elements[0].clone());
            }

            // 異なる型は Union
            elements.iter()
                .map(|e| LatticeType::Concrete(e.clone()))
                .fold(LatticeType::Bottom, |acc, t| acc.join(&t))
        }

        // Range{T} → T
        LatticeType::Concrete(ConcreteType::Range { element }) => {
            LatticeType::Concrete((**element).clone())
        }

        // String → Char
        LatticeType::Concrete(ConcreteType::String) => {
            LatticeType::Concrete(ConcreteType::Char)
        }

        // Dict{K, V} → Tuple{K, V}（pairs の場合）
        LatticeType::Concrete(ConcreteType::Dict { key, value }) => {
            LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![(**key).clone(), (**value).clone()],
            })
        }

        // 構造体の場合は iterate メソッドを参照
        LatticeType::Concrete(ConcreteType::Struct { name, .. }) => {
            // UnitRange, StepRange などの Range 系
            if name.starts_with("UnitRange") || name.starts_with("StepRange") {
                return LatticeType::Concrete(ConcreteType::I64);
            }

            // その他の構造体は iterate メソッドの返り値型から推論
            // 簡易実装では Top
            LatticeType::Top
        }

        // Union 型の場合は各要素型の Union
        LatticeType::Union(types) => {
            types.iter()
                .map(|t| infer_iterator_element_type(
                    &LatticeType::Concrete(t.clone()),
                    struct_table,
                ))
                .fold(LatticeType::Bottom, |acc, t| acc.join(&t))
        }

        _ => LatticeType::Top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_element_type() {
        let array_type = LatticeType::Concrete(ConcreteType::Array {
            element: Box::new(ConcreteType::I64),
        });

        let elem_type = infer_iterator_element_type(&array_type, &HashMap::new());
        assert_eq!(elem_type, LatticeType::Concrete(ConcreteType::I64));
    }

    #[test]
    fn test_tuple_element_type() {
        let tuple_type = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::I64, ConcreteType::I64],
        });

        let elem_type = infer_iterator_element_type(&tuple_type, &HashMap::new());
        assert_eq!(elem_type, LatticeType::Concrete(ConcreteType::I64));
    }

    #[test]
    fn test_mixed_tuple_element_type() {
        let tuple_type = LatticeType::Concrete(ConcreteType::Tuple {
            elements: vec![ConcreteType::I64, ConcreteType::F64],
        });

        let elem_type = infer_iterator_element_type(&tuple_type, &HashMap::new());
        assert!(matches!(elem_type, LatticeType::Union(_)));
    }
}
```

---

## 転送関数

**実装状況**: ✅ 実装済み

転送関数レジストリは `subset_julia_vm_compile/src/compile/tfuncs/` に実装されています。
以下のモジュールで転送関数が定義されています：

- `registry.rs` - レジストリ本体
- `arithmetic.rs` - 算術演算と比較演算
- `array_ops.rs` - 配列操作
- `string_ops.rs` - 文字列操作
- `intrinsics.rs` - 組み込み関数と型変換
- `field_ops.rs` - フィールドアクセス
- `iterator_ops.rs` - イテレータ操作
- `collection_ops.rs` - コレクション操作
- `math_intrinsics.rs` - 数学関数

`register_all()` 関数で全ての転送関数を一括登録します。

### レジストリ

```rust
// subset_julia_vm_compile/src/compile/tfuncs/registry.rs

use std::collections::HashMap;
use crate::compile::lattice::types::{ConcreteType, LatticeType};

/// 組み込み関数の返り値型を定義する転送関数レジストリ
pub struct TransferFunctions {
    /// 関数名 → 転送関数のマップ
    funcs: HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
}

impl TransferFunctions {
    pub fn new() -> Self {
        let mut funcs: HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>> = HashMap::new();

        // 算術演算
        Self::register_arithmetic(&mut funcs);

        // 比較演算
        Self::register_comparison(&mut funcs);

        // 配列操作
        Self::register_array_ops(&mut funcs);

        // 文字列操作
        Self::register_string_ops(&mut funcs);

        // 型変換
        Self::register_conversions(&mut funcs);

        // 数学関数
        Self::register_math(&mut funcs);

        // I/O 関数
        Self::register_io(&mut funcs);

        Self { funcs }
    }

    fn register_arithmetic(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        // 加算
        funcs.insert("+", Box::new(|args| {
            Self::numeric_binary_op(args)
        }));

        // 減算
        funcs.insert("-", Box::new(|args| {
            if args.len() == 1 {
                // 単項マイナス
                args[0].clone()
            } else {
                Self::numeric_binary_op(args)
            }
        }));

        // 乗算
        funcs.insert("*", Box::new(|args| {
            // 文字列の繰り返し
            if args.len() == 2 {
                if let (
                    LatticeType::Concrete(ConcreteType::String),
                    LatticeType::Concrete(ConcreteType::I64),
                ) | (
                    LatticeType::Concrete(ConcreteType::I64),
                    LatticeType::Concrete(ConcreteType::String),
                ) = (&args[0], &args[1]) {
                    return LatticeType::Concrete(ConcreteType::String);
                }
            }
            Self::numeric_binary_op(args)
        }));

        // 除算
        funcs.insert("/", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::F64)
        }));

        // 整数除算
        funcs.insert("div", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::I64)
        }));

        funcs.insert("÷", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::I64)
        }));

        // 剰余
        funcs.insert("mod", Box::new(|args| {
            if args.iter().all(|a| matches!(a, LatticeType::Concrete(c) if c.is_integer())) {
                LatticeType::Concrete(ConcreteType::I64)
            } else {
                LatticeType::Concrete(ConcreteType::F64)
            }
        }));

        funcs.insert("rem", Box::new(|args| {
            if args.iter().all(|a| matches!(a, LatticeType::Concrete(c) if c.is_integer())) {
                LatticeType::Concrete(ConcreteType::I64)
            } else {
                LatticeType::Concrete(ConcreteType::F64)
            }
        }));

        // べき乗
        funcs.insert("^", Box::new(|args| {
            if args.len() == 2 {
                if let (
                    LatticeType::Concrete(l),
                    LatticeType::Concrete(r),
                ) = (&args[0], &args[1]) {
                    if l.is_integer() && r.is_integer() {
                        return LatticeType::Concrete(ConcreteType::I64);
                    }
                }
            }
            LatticeType::Concrete(ConcreteType::F64)
        }));
    }

    fn register_comparison(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        for op in ["==", "!=", "<", "<=", ">", ">=", "===", "!==", "isequal", "isless"] {
            funcs.insert(op, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::Bool)
            }));
        }

        funcs.insert("isa", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));
    }

    fn register_array_ops(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        // length, size, ndims
        funcs.insert("length", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::I64)
        }));

        funcs.insert("size", Box::new(|args| {
            if args.len() == 2 {
                // size(arr, dim) → Int
                LatticeType::Concrete(ConcreteType::I64)
            } else {
                // size(arr) → Tuple
                LatticeType::Concrete(ConcreteType::Tuple { elements: vec![] })
            }
        }));

        funcs.insert("ndims", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::I64)
        }));

        // getindex
        funcs.insert("getindex", Box::new(|args| {
            if args.is_empty() {
                return LatticeType::Top;
            }
            match &args[0] {
                LatticeType::Concrete(ConcreteType::Array { element }) => {
                    LatticeType::Concrete((**element).clone())
                }
                LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
                    if elements.is_empty() {
                        LatticeType::Top
                    } else if elements.iter().all(|e| e == &elements[0]) {
                        LatticeType::Concrete(elements[0].clone())
                    } else {
                        elements.iter()
                            .map(|e| LatticeType::Concrete(e.clone()))
                            .fold(LatticeType::Bottom, |a, b| a.join(&b))
                    }
                }
                LatticeType::Concrete(ConcreteType::String) => {
                    LatticeType::Concrete(ConcreteType::Char)
                }
                _ => LatticeType::Top,
            }
        }));

        // push!, pop!
        funcs.insert("push!", Box::new(|args| {
            if !args.is_empty() {
                args[0].clone()
            } else {
                LatticeType::Top
            }
        }));

        funcs.insert("pop!", Box::new(|args| {
            if let Some(LatticeType::Concrete(ConcreteType::Array { element })) = args.first() {
                LatticeType::Concrete((**element).clone())
            } else {
                LatticeType::Top
            }
        }));

        // zeros, ones, fill
        funcs.insert("zeros", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::F64),
            })
        }));

        funcs.insert("ones", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::F64),
            })
        }));

        funcs.insert("fill", Box::new(|args| {
            if let Some(first) = args.first() {
                if let LatticeType::Concrete(elem) = first {
                    return LatticeType::Concrete(ConcreteType::Array {
                        element: Box::new(elem.clone()),
                    });
                }
            }
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::F64),
            })
        }));

        // collect
        funcs.insert("collect", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::F64),
            })
        }));

        // sum, prod
        funcs.insert("sum", Box::new(|args| {
            if let Some(LatticeType::Concrete(ConcreteType::Array { element })) = args.first() {
                if element.is_integer() {
                    return LatticeType::Concrete(ConcreteType::I64);
                }
            }
            LatticeType::Concrete(ConcreteType::F64)
        }));

        funcs.insert("prod", Box::new(|args| {
            if let Some(LatticeType::Concrete(ConcreteType::Array { element })) = args.first() {
                if element.is_integer() {
                    return LatticeType::Concrete(ConcreteType::I64);
                }
            }
            LatticeType::Concrete(ConcreteType::F64)
        }));

        // first, last
        funcs.insert("first", Box::new(|args| {
            if let Some(LatticeType::Concrete(ConcreteType::Array { element })) = args.first() {
                LatticeType::Concrete((**element).clone())
            } else {
                LatticeType::Top
            }
        }));

        funcs.insert("last", Box::new(|args| {
            if let Some(LatticeType::Concrete(ConcreteType::Array { element })) = args.first() {
                LatticeType::Concrete((**element).clone())
            } else {
                LatticeType::Top
            }
        }));
    }

    fn register_string_ops(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        for f in ["string", "repr", "uppercase", "lowercase", "strip",
                  "lstrip", "rstrip", "titlecase", "lowercasefirst",
                  "uppercasefirst", "reverse"] {
            funcs.insert(f, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::String)
            }));
        }

        funcs.insert("split", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::String),
            })
        }));

        funcs.insert("join", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));

        funcs.insert("startswith", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));

        funcs.insert("endswith", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));

        funcs.insert("contains", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));

        funcs.insert("occursin", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));
    }

    fn register_conversions(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        for name in ["Int", "Int64", "Int32", "Int16", "Int8"] {
            funcs.insert(name, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::I64)
            }));
        }

        for name in ["UInt", "UInt64", "UInt32", "UInt16", "UInt8"] {
            funcs.insert(name, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::U64)
            }));
        }

        for name in ["Float64", "Float32"] {
            funcs.insert(name, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::F64)
            }));
        }

        funcs.insert("Bool", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));

        funcs.insert("Char", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Char)
        }));

        funcs.insert("String", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));
    }

    fn register_math(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        // 常に Float64 を返す数学関数
        for f in ["sqrt", "sin", "cos", "tan", "asin", "acos", "atan",
                  "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
                  "exp", "exp2", "exp10", "log", "log2", "log10"] {
            funcs.insert(f, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::F64)
            }));
        }

        // 引数の型を保持する関数
        for f in ["abs", "abs2", "sign"] {
            funcs.insert(f, Box::new(|args| {
                if let Some(arg) = args.first() {
                    arg.clone()
                } else {
                    LatticeType::Concrete(ConcreteType::F64)
                }
            }));
        }

        // floor, ceil, round, trunc
        for f in ["floor", "ceil", "round", "trunc"] {
            funcs.insert(f, Box::new(|args| {
                // floor(T, x) 形式の場合は T を返す
                if args.len() == 2 {
                    return args[0].clone();
                }
                LatticeType::Concrete(ConcreteType::F64)
            }));
        }

        // min, max
        for f in ["min", "max"] {
            funcs.insert(f, Box::new(|args| {
                Self::numeric_binary_op(args)
            }));
        }

        // clamp
        funcs.insert("clamp", Box::new(|args| {
            if let Some(arg) = args.first() {
                arg.clone()
            } else {
                LatticeType::Concrete(ConcreteType::F64)
            }
        }));

        // rand, randn
        funcs.insert("rand", Box::new(|args| {
            if args.is_empty() {
                LatticeType::Concrete(ConcreteType::F64)
            } else {
                LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(ConcreteType::F64),
                })
            }
        }));

        funcs.insert("randn", Box::new(|args| {
            if args.is_empty() {
                LatticeType::Concrete(ConcreteType::F64)
            } else {
                LatticeType::Concrete(ConcreteType::Array {
                    element: Box::new(ConcreteType::F64),
                })
            }
        }));
    }

    fn register_io(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>>,
    ) {
        for f in ["println", "print", "printstyled", "error", "throw", "sleep"] {
            funcs.insert(f, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::Nothing)
            }));
        }

        funcs.insert("readline", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));
    }

    /// 数値二項演算の型推論
    fn numeric_binary_op(args: &[LatticeType]) -> LatticeType {
        if args.len() != 2 {
            return LatticeType::Top;
        }

        match (&args[0], &args[1]) {
            (
                LatticeType::Concrete(l),
                LatticeType::Concrete(r),
            ) => {
                // 両方整数 → 整数
                if l.is_integer() && r.is_integer() {
                    return LatticeType::Concrete(ConcreteType::I64);
                }
                // 少なくとも一方が浮動小数点 → 浮動小数点
                if l.is_float() || r.is_float() {
                    return LatticeType::Concrete(ConcreteType::F64);
                }
                // 両方数値 → Float
                if l.is_numeric() && r.is_numeric() {
                    return LatticeType::Concrete(ConcreteType::F64);
                }
                LatticeType::Top
            }
            _ => LatticeType::Top,
        }
    }

    /// 関数の返り値型を取得
    pub fn infer_return_type(&self, func_name: &str, arg_types: &[LatticeType]) -> LatticeType {
        if let Some(tfunc) = self.funcs.get(func_name) {
            tfunc(arg_types)
        } else {
            LatticeType::Top
        }
    }

    /// 転送関数の登録（拡張用）
    pub fn register(
        &mut self,
        name: &'static str,
        func: Box<dyn Fn(&[LatticeType]) -> LatticeType + Send + Sync>,
    ) {
        self.funcs.insert(name, func);
    }
}

impl Default for TransferFunctions {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## 既存コードとの統合

### ValueType ↔ LatticeType ブリッジ

```rust
// subset_julia_vm_compile/src/compile/bridge.rs

use crate::vm::{ValueType, ArrayElementType};
use crate::compile::lattice::types::{ConcreteType, LatticeType};

/// ValueType から LatticeType への変換
impl From<&ValueType> for LatticeType {
    fn from(vt: &ValueType) -> Self {
        match vt {
            ValueType::I8 => LatticeType::Concrete(ConcreteType::I8),
            ValueType::I16 => LatticeType::Concrete(ConcreteType::I16),
            ValueType::I32 => LatticeType::Concrete(ConcreteType::I32),
            ValueType::I64 => LatticeType::Concrete(ConcreteType::I64),
            ValueType::I128 => LatticeType::Concrete(ConcreteType::I128),
            ValueType::BigInt => LatticeType::Concrete(ConcreteType::BigInt),
            ValueType::U8 => LatticeType::Concrete(ConcreteType::U8),
            ValueType::U16 => LatticeType::Concrete(ConcreteType::U16),
            ValueType::U32 => LatticeType::Concrete(ConcreteType::U32),
            ValueType::U64 => LatticeType::Concrete(ConcreteType::U64),
            ValueType::U128 => LatticeType::Concrete(ConcreteType::U128),
            ValueType::F32 => LatticeType::Concrete(ConcreteType::F32),
            ValueType::F64 => LatticeType::Concrete(ConcreteType::F64),
            ValueType::BigFloat => LatticeType::Concrete(ConcreteType::BigFloat),
            ValueType::Bool => LatticeType::Concrete(ConcreteType::Bool),
            ValueType::Char => LatticeType::Concrete(ConcreteType::Char),
            ValueType::Str => LatticeType::Concrete(ConcreteType::String),
            ValueType::Nothing => LatticeType::Concrete(ConcreteType::Nothing),
            ValueType::Missing => LatticeType::Concrete(ConcreteType::Missing),
            ValueType::Symbol => LatticeType::Concrete(ConcreteType::Symbol),
            ValueType::Struct(id) => LatticeType::Concrete(ConcreteType::Struct {
                type_id: *id,
                name: String::new(),
            }),
            ValueType::Array => LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::F64),
            }),
            ValueType::ArrayOf(elem) => LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(array_element_to_concrete(elem)),
            }),
            ValueType::Tuple => LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![],
            }),
            ValueType::Range => LatticeType::Concrete(ConcreteType::Range {
                element: Box::new(ConcreteType::I64),
            }),
            ValueType::Dict => LatticeType::Concrete(ConcreteType::Dict {
                key: Box::new(ConcreteType::String),
                value: Box::new(ConcreteType::F64),
            }),
            ValueType::DataType => LatticeType::Concrete(ConcreteType::DataType),
            ValueType::Module => LatticeType::Concrete(ConcreteType::Module),
            ValueType::Any => LatticeType::Top,
            _ => LatticeType::Top,
        }
    }
}

/// LatticeType から ValueType への変換
impl From<&LatticeType> for ValueType {
    fn from(lt: &LatticeType) -> Self {
        match lt {
            LatticeType::Bottom => ValueType::Nothing,
            LatticeType::Concrete(ct) => concrete_to_value_type(ct),
            LatticeType::Union(_) => ValueType::Any,
            LatticeType::Conditional { .. } => ValueType::Any,
            LatticeType::Top => ValueType::Any,
        }
    }
}

fn concrete_to_value_type(ct: &ConcreteType) -> ValueType {
    match ct {
        ConcreteType::I8 => ValueType::I8,
        ConcreteType::I16 => ValueType::I16,
        ConcreteType::I32 => ValueType::I32,
        ConcreteType::I64 => ValueType::I64,
        ConcreteType::I128 => ValueType::I128,
        ConcreteType::BigInt => ValueType::BigInt,
        ConcreteType::U8 => ValueType::U8,
        ConcreteType::U16 => ValueType::U16,
        ConcreteType::U32 => ValueType::U32,
        ConcreteType::U64 => ValueType::U64,
        ConcreteType::U128 => ValueType::U128,
        ConcreteType::F32 => ValueType::F32,
        ConcreteType::F64 => ValueType::F64,
        ConcreteType::BigFloat => ValueType::BigFloat,
        ConcreteType::Bool => ValueType::Bool,
        ConcreteType::Char => ValueType::Char,
        ConcreteType::String => ValueType::Str,
        ConcreteType::Nothing => ValueType::Nothing,
        ConcreteType::Missing => ValueType::Missing,
        ConcreteType::Symbol => ValueType::Symbol,
        ConcreteType::Array { element } => ValueType::ArrayOf(concrete_to_array_element(element)),
        ConcreteType::Tuple { .. } => ValueType::Tuple,
        ConcreteType::NamedTuple { .. } => ValueType::NamedTuple,
        ConcreteType::Dict { .. } => ValueType::Dict,
        ConcreteType::Range { .. } => ValueType::Range,
        ConcreteType::Struct { type_id, .. } => ValueType::Struct(*type_id),
        ConcreteType::DataType => ValueType::DataType,
        ConcreteType::Module => ValueType::Module,
        ConcreteType::Function { .. } => ValueType::Any,
    }
}

fn array_element_to_concrete(elem: &ArrayElementType) -> ConcreteType {
    match elem {
        ArrayElementType::I64 => ConcreteType::I64,
        ArrayElementType::F64 => ConcreteType::F64,
        ArrayElementType::Bool => ConcreteType::Bool,
        ArrayElementType::String => ConcreteType::String,
        ArrayElementType::Char => ConcreteType::Char,
        ArrayElementType::StructOf(id) => ConcreteType::Struct {
            type_id: *id,
            name: String::new(),
        },
        _ => ConcreteType::F64,
    }
}

fn concrete_to_array_element(ct: &ConcreteType) -> ArrayElementType {
    match ct {
        ConcreteType::I64 => ArrayElementType::I64,
        ConcreteType::F64 => ArrayElementType::F64,
        ConcreteType::Bool => ArrayElementType::Bool,
        ConcreteType::String => ArrayElementType::String,
        ConcreteType::Char => ArrayElementType::Char,
        ConcreteType::Struct { type_id, .. } => ArrayElementType::StructOf(*type_id),
        _ => ArrayElementType::Any,
    }
}
```

### 統合アダプタ

```rust
// subset_julia_vm_compile/src/compile/inference.rs に追加

use super::lattice::types::LatticeType;
use super::abstract_interp::engine::{InferenceEngine, StructTypeInfo};
use super::bridge;

/// 新しい型推論エンジンを使用した関数返り値型推論
pub fn infer_function_return_type_v2(
    func: &Function,
    struct_table: &HashMap<String, StructInfo>,
) -> ValueType {
    // StructInfo を StructTypeInfo に変換
    let lattice_struct_table: HashMap<String, StructTypeInfo> = struct_table.iter()
        .map(|(name, info)| {
            (name.clone(), StructTypeInfo {
                type_id: info.type_id,
                name: name.clone(),
                fields: info.fields.iter()
                    .map(|(fname, ftype)| {
                        (fname.clone(), LatticeType::from(ftype))
                    })
                    .collect(),
            })
        })
        .collect();

    let mut engine = InferenceEngine::with_struct_table(lattice_struct_table);
    engine.set_struct_table(lattice_struct_table);

    // パラメータの型を変換
    let params: Vec<_> = func.params.iter()
        .map(|p| {
            let ty = match &p.type_annotation {
                Some(jt) => {
                    let vt = julia_type_to_value_type(jt);
                    Some(LatticeType::from(&vt))
                }
                None => None,
            };
            (p.name.clone(), ty)
        })
        .collect();

    // 推論実行
    let return_type = engine.infer_function(&params, &func.body);

    // ValueType に変換
    ValueType::from(&return_type)
}
```

---

## テスト戦略

### ユニットテスト

```rust
// subset_julia_vm_compile/src/compile/lattice/tests.rs

#[cfg(test)]
mod tests {
    use super::super::types::*;
    use super::super::ops::*;

    #[test]
    fn test_lattice_join() {
        let int = LatticeType::Concrete(ConcreteType::I64);
        let float = LatticeType::Concrete(ConcreteType::F64);
        let bottom = LatticeType::Bottom;
        let top = LatticeType::Top;

        // Bottom + T = T
        assert_eq!(bottom.join(&int), int);
        assert_eq!(int.join(&bottom), int);

        // T + Top = Top
        assert_eq!(int.join(&top), top);
        assert_eq!(top.join(&int), top);

        // T + T = T
        assert_eq!(int.join(&int), int);

        // Int + Float = Union{Int, Float}
        let joined = int.join(&float);
        assert!(matches!(joined, LatticeType::Union(_)));
    }

    #[test]
    fn test_lattice_subtype() {
        let int = LatticeType::Concrete(ConcreteType::I64);
        let float = LatticeType::Concrete(ConcreteType::F64);
        let bottom = LatticeType::Bottom;
        let top = LatticeType::Top;

        // Bottom <: T for all T
        assert!(bottom.is_subtype_of(&int));
        assert!(bottom.is_subtype_of(&float));
        assert!(bottom.is_subtype_of(&top));

        // T <: Top for all T
        assert!(int.is_subtype_of(&top));
        assert!(float.is_subtype_of(&top));

        // T <: T
        assert!(int.is_subtype_of(&int));

        // !(Top <: T) for T != Top
        assert!(!top.is_subtype_of(&int));
    }

    #[test]
    fn test_union_widening() {
        use std::collections::BTreeSet;

        // 5要素の Union は Top に拡大
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::I64);
        types.insert(ConcreteType::F64);
        types.insert(ConcreteType::Bool);
        types.insert(ConcreteType::String);
        types.insert(ConcreteType::Char);

        let result = LatticeType::simplify_union(types);
        assert_eq!(result, LatticeType::Top);
    }
}
```

### 統合テスト

```julia
# tests/fixtures/type_inference/loop_inference.jl

# テスト: ループ変数の型推論
function sum_array(arr)
    total = 0
    for x in arr  # x: Int64 （配列要素型から推論）
        total += x
    end
    total
end

# 期待: sum_array([1,2,3]) の x は Int64 と推論される
result = sum_array([1, 2, 3])
@assert result == 6
```

```julia
# tests/fixtures/type_inference/conditional_narrowing.jl

# テスト: 条件分岐での型絞り込み
function process(val)
    if val isa Int
        val + 1  # val: Int64
    elseif val isa Float64
        val * 2.0  # val: Float64
    else
        0
    end
end

# 期待: 各分岐で val の型が絞り込まれる
@assert process(5) == 6
@assert process(2.5) == 5.0
```

```julia
# tests/fixtures/type_inference/union_types.jl

# テスト: Union 型の推論
function mixed_return(flag)
    if flag
        1
    else
        2.0
    end
end

# 期待: 返り値型は Union{Int64, Float64}
result1 = mixed_return(true)
result2 = mixed_return(false)
@assert result1 isa Int
@assert result2 isa Float64
```

---

## 実装優先順位

**実装状況**: ✅ 全てのフェーズが実装済み

### Phase 1: 基盤（必須）✅ 完了
1. ✅ `lattice/types.rs` - LatticeType, ConcreteType, Const 型定義
2. ✅ `lattice/ops.rs` - join, meet, is_subtype_of, subtract
3. ✅ `bridge.rs` - ValueType ↔ LatticeType 変換

### Phase 2: 推論エンジン ✅ 完了
1. ✅ `abstract_interp/env.rs` - 型環境（TypeEnv）
2. ✅ `abstract_interp/engine.rs` - 基本推論（InferenceEngine）
3. ✅ `tfuncs/registry.rs` - 組み込み関数（全モジュール実装済み）

### Phase 3: 高度な機能 ✅ 完了
1. ✅ `abstract_interp/conditional.rs` - 条件分岐での型絞り込み（環境分割アプローチ）
2. ✅ `abstract_interp/loop_analysis.rs` - ループ変数の型推論
3. ✅ `const_prop/` - 定数伝播（追加実装）

### Phase 4: 統合 ✅ 完了
1. ✅ 既存 `inference.rs` との統合（`infer_function_return_type_v2`）
2. ✅ 既存 `expr/infer.rs` との統合
3. ⚠️ 回帰テスト（一部存在、追加が必要）

---

## 関連ドキュメント

- [TYPE_INFERENCE_ENHANCEMENT.md](./TYPE_INFERENCE_ENHANCEMENT.md) - 型推論強化計画
- [docs/aot/IMPLEMENTATION_GUIDE.md](../aot/IMPLEMENTATION_GUIDE.md) - AoT 実装ガイド
- `subset_julia_vm_compile/src/compile/inference.rs` - 現在の型推論実装
- `subset_julia_vm_compile/src/compile/expr/infer.rs` - 式の型推論
