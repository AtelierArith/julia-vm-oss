# SubsetJuliaVM 型推論強化計画

**作成日**: 2026-01-13
**最終更新**: 2026-01-17
**ステータス**: 実装完了（Phase 1-5）

> **Archive note (2026-06-11):** この 2026-01-17 時点の計画完了版は
> 履歴として保持します。現行ガイドは
> [TYPE_INFERENCE_COMPLETE.md](../TYPE_INFERENCE_COMPLETE.md)、旧統合ドキュメントは
> [TYPE_INFERENCE_COMPLETE_20260116.md](TYPE_INFERENCE_COMPLETE_20260116.md)
> を参照してください。

## 概要

SubsetJuliaVM の型推論を Julia 本家のアプローチに基づいて強化し、より精密な型情報を取得することで、実行時の型チェックオーバーヘッドを削減し、将来の AoT コンパイルに向けた基盤を構築する。

## 目次

1. [現状の型推論の課題](#現状の型推論の課題)
2. [Julia 本家の型推論アーキテクチャ](#julia-本家の型推論アーキテクチャ)
3. [SubsetJuliaVM への適用提案](#subsetjuliavm-への適用提案)
4. [実装計画](#実装計画)
5. [テスト戦略](#テスト戦略)

---

## 現状の型推論の課題

### 現在のアーキテクチャ

型推論は `compile/abstract_interp/` モジュールに新しいアーキテクチャで実装されている：

```rust
// compile/abstract_interp/engine.rs - 抽象解釈エンジン
pub struct InferenceEngine {
    tfuncs: TransferFunctions,           // 転送関数テーブル
    return_type_cache: HashMap<String, LatticeType>,  // 関数返り値キャッシュ
    struct_table: HashMap<String, StructTypeInfo>,    // 構造体テーブル
    function_table: HashMap<String, Function>,        // 関数テーブル（IPO用）
}

// compile/lattice/types.rs - 型格子
pub enum LatticeType {
    Bottom,              // 到達不能
    Const(ConstValue),   // 定数値
    Concrete(ConcreteType),  // 具体型
    Union(BTreeSet<ConcreteType>),  // Union 型
    Conditional { ... }, // 条件付き型
    Top,                 // Any
}
```

### 課題一覧（実装状況）

| 課題 | 説明 | ステータス |
|------|------|------|
| **Any への過度なフォールバック** | 型が決定できない場合すぐに `Any` になる | ✅ 改善済み（型格子導入） |
| **ループ変数の型推論欠如** | `for x in arr` の `x` が常に `Any` | ✅ 実装済み（loop_analysis.rs） |
| **関数間型伝播なし** | 呼び出し先の返り値型が不明 | ✅ 実装済み（IPO分析） |
| **条件分岐での型絞り込み欠如** | `if x isa Int` 後も `x` の型が不変 | ✅ 実装済み（環境分割アプローチ） |
| **Union 型の簡易実装** | Union 型の適切な結合・簡約がない | ✅ 実装済み（Union型サポート） |
| **構造体フィールド型ルックアップ** | フィールドアクセスの型推論が不正確 | ✅ 実装済み（struct_table統合） |
| **高階関数の完全対応** | map/filter の引数関数の戻り値型推論 | 🔄 部分的実装 |

### 実装済み機能

```julia
# ✅ ループ変数の型推論
function sum_array(arr)
    total = 0
    for x in arr  # x: 配列の要素型として推論される
        total += x
    end
    total
end

# ✅ 条件分岐での型絞り込み（環境分割アプローチ）
function process(val)
    if val isa Int
        val + 1  # 環境が分割され、then分岐では val: Int64
    else
        0
    end
end

# ✅ 関数間型伝播
function helper(x, y)
    x + y
end

function caller()
    helper(1, 2)  # helper の本体を解析して Int64 を推論
end
```

### 参照
- 型推論エンジン: `subset_julia_vm/src/compile/abstract_interp/engine.rs`
- 型格子: `subset_julia_vm/src/compile/lattice/types.rs`
- ループ解析: `subset_julia_vm/src/compile/abstract_interp/loop_analysis.rs`
- 条件分岐: `subset_julia_vm/src/compile/abstract_interp/conditional.rs`
- 転送関数: `subset_julia_vm/src/compile/tfuncs/`

---

## Julia 本家の型推論アーキテクチャ

### 抽象解釈 (Abstract Interpretation)

Julia の型推論は **抽象解釈** に基づくデータフロー解析である：

```
プログラムの具体的な実行
    ↓ 抽象化
抽象ドメイン（型）での実行をシミュレート
    ↓
不動点に到達するまで繰り返し
    ↓
型情報を収集
```

### 型格子 (Type Lattice)

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

**格子演算**:
- `⊑` (subtype): 型の包含関係
- `⊔` (join): 型の結合（Union）
- `⊓` (meet): 型の交差

### Conditional 型

制御フローに敏感な型絞り込み：

```julia
# Julia の内部表現
if x isa Int
    # x: Conditional { slot=x, thentype=Int, elsetype=Union{} }
    x + 1  # この分岐では x: Int
end
```

```rust
// Julia の Conditional 型（概念的な Rust 表現）
struct Conditional {
    slot: SlotNumber,      // 変数スロット
    thentype: Type,        // then 分岐での型
    elsetype: Type,        // else 分岐での型
}
```

**実装上の注意**: 設計仕様では Conditional 型を使用することを想定していますが、
実際の実装では**環境分割（Environment Splitting）**アプローチを採用しています。
これは機能的に等価であり、実装の簡潔性と保守性を優先した選択です。
詳細は `subset_julia_vm/src/compile/abstract_interp/conditional.rs` を参照してください。

### MustAlias

`===` や `isa` チェック後のフィールド制約伝播：

```julia
if x.field === nothing
    # x.field: Nothing (確定)
end
```

### 型拡大 (Type Widening)

無限ループ防止のための制約：

```julia
# Julia の定数
const MAX_TYPEUNION_COMPLEXITY = 3  # Union のネスト深度
const MAX_TYPEUNION_LENGTH = 3      # Union の要素数
```

複雑すぎる Union は自動的に拡大される：

```
Union{Int, Float64, String, Bool}  # 4要素
    ↓ widening
Any  # または共通のスーパータイプ
```

### 転送関数 (Transfer Functions)

組み込み関数の返り値型を定義：

```julia
# tfuncs の例（概念的）
tfunc(+, Int, Int) = Int
tfunc(+, Int, Float64) = Float64
tfunc(length, Array{T}) = Int
tfunc(getindex, Array{T}, Int) = T
```

---

## SubsetJuliaVM への適用提案

### Phase 1: 型格子の導入

#### 新しい型システム

```rust
// src/compile/types/lattice.rs (新規)

/// 型格子の要素
#[derive(Clone, Debug, PartialEq)]
pub enum LatticeType {
    /// 底型（空集合、到達不能）
    Bottom,

    /// 具体型
    Concrete(ConcreteType),

    /// Union 型
    Union(Vec<ConcreteType>),

    /// Conditional 型（制御フロー依存）
    Conditional {
        slot: String,
        then_type: Box<LatticeType>,
        else_type: Box<LatticeType>,
    },

    /// 頂型（任意の型）
    Top,
}

/// 具体型
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConcreteType {
    Int64,
    Float64,
    Bool,
    String,
    Char,
    Nothing,
    Missing,
    Symbol,
    Array { element: Box<ConcreteType> },
    Tuple { elements: Vec<ConcreteType> },
    NamedTuple { fields: Vec<(String, ConcreteType)> },
    Struct { name: String, type_id: usize },
    Function { name: String },
}

impl LatticeType {
    /// 型の結合（Union）
    pub fn join(&self, other: &LatticeType) -> LatticeType {
        match (self, other) {
            (LatticeType::Bottom, t) | (t, LatticeType::Bottom) => t.clone(),
            (LatticeType::Top, _) | (_, LatticeType::Top) => LatticeType::Top,
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => {
                if a == b {
                    LatticeType::Concrete(a.clone())
                } else {
                    LatticeType::Union(vec![a.clone(), b.clone()])
                }
            }
            (LatticeType::Union(us), LatticeType::Concrete(c)) => {
                let mut new_union = us.clone();
                if !new_union.contains(c) {
                    new_union.push(c.clone());
                }
                Self::simplify_union(new_union)
            }
            // ... 他のケース
            _ => LatticeType::Top,
        }
    }

    /// 型の包含関係
    pub fn is_subtype_of(&self, other: &LatticeType) -> bool {
        match (self, other) {
            (LatticeType::Bottom, _) => true,
            (_, LatticeType::Top) => true,
            (LatticeType::Concrete(a), LatticeType::Concrete(b)) => a == b,
            (LatticeType::Concrete(c), LatticeType::Union(us)) => us.contains(c),
            _ => false,
        }
    }

    /// Union の簡約
    fn simplify_union(types: Vec<ConcreteType>) -> LatticeType {
        const MAX_UNION_LENGTH: usize = 4;

        if types.is_empty() {
            LatticeType::Bottom
        } else if types.len() == 1 {
            LatticeType::Concrete(types[0].clone())
        } else if types.len() > MAX_UNION_LENGTH {
            // 拡大: 共通スーパータイプまたは Top
            Self::widen_union(&types)
        } else {
            LatticeType::Union(types)
        }
    }

    /// Union の拡大
    fn widen_union(types: &[ConcreteType]) -> LatticeType {
        // 全て数値型なら Number へ
        let all_numeric = types.iter().all(|t| matches!(
            t,
            ConcreteType::Int64 | ConcreteType::Float64
        ));
        if all_numeric {
            return LatticeType::Union(vec![
                ConcreteType::Int64,
                ConcreteType::Float64,
            ]);
        }

        // それ以外は Top
        LatticeType::Top
    }
}
```

### Phase 2: 抽象解釈エンジン

```rust
// src/compile/inference/engine.rs (新規)

use std::collections::HashMap;

/// 抽象解釈による型推論エンジン
pub struct TypeInferenceEngine {
    /// 変数の型環境
    env: HashMap<String, LatticeType>,

    /// 関数の返り値型キャッシュ
    function_returns: HashMap<String, LatticeType>,

    /// 転送関数テーブル
    tfuncs: TransferFunctions,

    /// 最大反復回数
    max_iterations: usize,
}

impl TypeInferenceEngine {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            function_returns: HashMap::new(),
            tfuncs: TransferFunctions::new(),
            max_iterations: 100,
        }
    }

    /// 関数本体の型推論
    pub fn infer_function(
        &mut self,
        params: &[(String, Option<LatticeType>)],
        body: &[CoreExpr],
    ) -> LatticeType {
        // パラメータの型を環境に追加
        for (name, ty) in params {
            let param_type = ty.clone().unwrap_or(LatticeType::Top);
            self.env.insert(name.clone(), param_type);
        }

        // 不動点計算
        let mut changed = true;
        let mut iterations = 0;

        while changed && iterations < self.max_iterations {
            changed = false;
            iterations += 1;

            for expr in body {
                if self.infer_expr(expr) {
                    changed = true;
                }
            }
        }

        // 返り値型の収集
        self.collect_return_type(body)
    }

    /// 式の型推論（環境が更新されたら true）
    fn infer_expr(&mut self, expr: &CoreExpr) -> bool {
        match expr {
            CoreExpr::Assign { name, value, .. } => {
                let value_type = self.expr_type(value);
                self.update_var(name, value_type)
            }

            CoreExpr::If { cond, then_body, else_body, .. } => {
                // 条件による型絞り込みを適用
                let (then_env, else_env) = self.split_env_by_condition(cond);

                // then 分岐の推論
                let old_env = self.env.clone();
                self.env = then_env;
                for e in then_body {
                    self.infer_expr(e);
                }
                let then_env_after = self.env.clone();

                // else 分岐の推論
                self.env = else_env;
                if let Some(else_body) = else_body {
                    for e in else_body {
                        self.infer_expr(e);
                    }
                }
                let else_env_after = self.env.clone();

                // 環境のマージ
                self.env = Self::merge_envs(then_env_after, else_env_after);
                self.env != old_env
            }

            CoreExpr::ForEach { var, iter, body, .. } => {
                // イテレータの要素型を推論
                let iter_type = self.expr_type(iter);
                let elem_type = self.element_type(&iter_type);

                // ループ変数の型を設定
                self.update_var(var, elem_type);

                // ループ本体の推論
                let mut changed = false;
                for e in body {
                    if self.infer_expr(e) {
                        changed = true;
                    }
                }
                changed
            }

            _ => false,
        }
    }

    /// 条件による環境分割（Conditional 型の生成）
    fn split_env_by_condition(
        &self,
        cond: &CoreExpr,
    ) -> (HashMap<String, LatticeType>, HashMap<String, LatticeType>) {
        let mut then_env = self.env.clone();
        let mut else_env = self.env.clone();

        match cond {
            // x isa T パターン
            CoreExpr::Call { func, args, .. }
                if func == "isa" && args.len() == 2 =>
            {
                if let CoreExpr::Var { name, .. } = &args[0] {
                    if let Some(type_const) = self.extract_type_constant(&args[1]) {
                        // then: x は T 型
                        then_env.insert(name.clone(), type_const.clone());
                        // else: x は T 以外
                        if let Some(current) = self.env.get(name) {
                            let narrowed = self.subtract_type(current, &type_const);
                            else_env.insert(name.clone(), narrowed);
                        }
                    }
                }
            }

            // x === nothing パターン
            CoreExpr::Call { func, args, .. }
                if func == "===" && args.len() == 2 =>
            {
                if let CoreExpr::Var { name, .. } = &args[0] {
                    if let CoreExpr::Literal { value: Value::Nothing, .. } = &args[1] {
                        then_env.insert(name.clone(), LatticeType::Concrete(ConcreteType::Nothing));
                        // else: Nothing 以外
                        if let Some(current) = self.env.get(name) {
                            let narrowed = self.subtract_type(
                                current,
                                &LatticeType::Concrete(ConcreteType::Nothing),
                            );
                            else_env.insert(name.clone(), narrowed);
                        }
                    }
                }
            }

            _ => {}
        }

        (then_env, else_env)
    }

    /// 配列/イテラブルの要素型を取得
    fn element_type(&self, iter_type: &LatticeType) -> LatticeType {
        match iter_type {
            LatticeType::Concrete(ConcreteType::Array { element }) => {
                LatticeType::Concrete((**element).clone())
            }
            LatticeType::Concrete(ConcreteType::Tuple { elements }) => {
                if elements.is_empty() {
                    LatticeType::Bottom
                } else if elements.iter().all(|e| e == &elements[0]) {
                    LatticeType::Concrete(elements[0].clone())
                } else {
                    LatticeType::Union(elements.clone())
                }
            }
            // Range{Int} -> Int
            LatticeType::Concrete(ConcreteType::Struct { name, .. })
                if name.starts_with("UnitRange") || name.starts_with("StepRange") =>
            {
                LatticeType::Concrete(ConcreteType::Int64)
            }
            _ => LatticeType::Top,
        }
    }

    /// 変数の型を更新（変更があれば true）
    fn update_var(&mut self, name: &str, new_type: LatticeType) -> bool {
        match self.env.get(name) {
            Some(old_type) => {
                let joined = old_type.join(&new_type);
                if &joined != old_type {
                    self.env.insert(name.to_string(), joined);
                    true
                } else {
                    false
                }
            }
            None => {
                self.env.insert(name.to_string(), new_type);
                true
            }
        }
    }

    /// 環境のマージ
    fn merge_envs(
        env1: HashMap<String, LatticeType>,
        env2: HashMap<String, LatticeType>,
    ) -> HashMap<String, LatticeType> {
        let mut result = env1;
        for (name, ty) in env2 {
            result
                .entry(name)
                .and_modify(|t| *t = t.join(&ty))
                .or_insert(ty);
        }
        result
    }
}
```

### Phase 3: 転送関数 (Transfer Functions)

```rust
// src/compile/inference/tfuncs.rs (新規)

use std::collections::HashMap;

/// 組み込み関数の返り値型定義
pub struct TransferFunctions {
    funcs: HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType>>,
}

impl TransferFunctions {
    pub fn new() -> Self {
        let mut funcs = HashMap::new();

        // 算術演算子
        Self::register_arithmetic(&mut funcs);

        // 比較演算子
        Self::register_comparison(&mut funcs);

        // 配列操作
        Self::register_array_ops(&mut funcs);

        // 文字列操作
        Self::register_string_ops(&mut funcs);

        // 型変換
        Self::register_conversions(&mut funcs);

        Self { funcs }
    }

    fn register_arithmetic(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType>>,
    ) {
        // + 演算子
        funcs.insert("+", Box::new(|args| {
            if args.len() != 2 {
                return LatticeType::Top;
            }
            match (&args[0], &args[1]) {
                (
                    LatticeType::Concrete(ConcreteType::Int64),
                    LatticeType::Concrete(ConcreteType::Int64),
                ) => LatticeType::Concrete(ConcreteType::Int64),
                (
                    LatticeType::Concrete(ConcreteType::Float64),
                    LatticeType::Concrete(ConcreteType::Float64),
                ) => LatticeType::Concrete(ConcreteType::Float64),
                (
                    LatticeType::Concrete(ConcreteType::Int64),
                    LatticeType::Concrete(ConcreteType::Float64),
                ) |
                (
                    LatticeType::Concrete(ConcreteType::Float64),
                    LatticeType::Concrete(ConcreteType::Int64),
                ) => LatticeType::Concrete(ConcreteType::Float64),
                (
                    LatticeType::Concrete(ConcreteType::String),
                    LatticeType::Concrete(ConcreteType::String),
                ) => LatticeType::Concrete(ConcreteType::String),
                _ => LatticeType::Top,
            }
        }));

        // 他の演算子も同様に登録
        funcs.insert("-", Box::new(|args| {
            // +と同様のロジック
            Self::numeric_binary_op(args)
        }));

        funcs.insert("*", Box::new(|args| {
            Self::numeric_binary_op(args)
        }));

        funcs.insert("/", Box::new(|_args| {
            // 除算は常に Float64
            LatticeType::Concrete(ConcreteType::Float64)
        }));

        funcs.insert("div", Box::new(|args| {
            if args.iter().all(|a| matches!(
                a,
                LatticeType::Concrete(ConcreteType::Int64)
            )) {
                LatticeType::Concrete(ConcreteType::Int64)
            } else {
                LatticeType::Top
            }
        }));
    }

    fn register_comparison(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType>>,
    ) {
        for op in ["==", "!=", "<", "<=", ">", ">=", "===", "!=="].iter() {
            funcs.insert(*op, Box::new(|_| {
                LatticeType::Concrete(ConcreteType::Bool)
            }));
        }

        funcs.insert("isa", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));
    }

    fn register_array_ops(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType>>,
    ) {
        // length
        funcs.insert("length", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Int64)
        }));

        // size
        funcs.insert("size", Box::new(|args| {
            if args.len() == 2 {
                // size(arr, dim)
                LatticeType::Concrete(ConcreteType::Int64)
            } else {
                // size(arr)
                LatticeType::Concrete(ConcreteType::Tuple {
                    elements: vec![ConcreteType::Int64], // 簡易実装
                })
            }
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
                        LatticeType::Union(elements.clone())
                    }
                }
                _ => LatticeType::Top,
            }
        }));

        // push!
        funcs.insert("push!", Box::new(|args| {
            if !args.is_empty() {
                args[0].clone() // 元の配列を返す
            } else {
                LatticeType::Top
            }
        }));

        // map
        funcs.insert("map", Box::new(|_args| {
            // map の返り値型推論は関数の返り値型に依存
            // 簡易実装では Top
            LatticeType::Top
        }));

        // filter
        funcs.insert("filter", Box::new(|args| {
            // filter は元の配列と同じ型
            if args.len() >= 2 {
                args[1].clone()
            } else {
                LatticeType::Top
            }
        }));
    }

    fn register_string_ops(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType>>,
    ) {
        funcs.insert("string", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));

        funcs.insert("repr", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));

        funcs.insert("uppercase", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));

        funcs.insert("lowercase", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));

        funcs.insert("split", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::String),
            })
        }));

        funcs.insert("join", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::String)
        }));
    }

    fn register_conversions(
        funcs: &mut HashMap<&'static str, Box<dyn Fn(&[LatticeType]) -> LatticeType>>,
    ) {
        funcs.insert("Int", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Int64)
        }));

        funcs.insert("Int64", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Int64)
        }));

        funcs.insert("Float64", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Float64)
        }));

        funcs.insert("Bool", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Bool)
        }));

        funcs.insert("Char", Box::new(|_| {
            LatticeType::Concrete(ConcreteType::Char)
        }));
    }

    fn numeric_binary_op(args: &[LatticeType]) -> LatticeType {
        if args.len() != 2 {
            return LatticeType::Top;
        }
        match (&args[0], &args[1]) {
            (
                LatticeType::Concrete(ConcreteType::Int64),
                LatticeType::Concrete(ConcreteType::Int64),
            ) => LatticeType::Concrete(ConcreteType::Int64),
            (
                LatticeType::Concrete(ConcreteType::Float64),
                LatticeType::Concrete(ConcreteType::Float64),
            ) |
            (
                LatticeType::Concrete(ConcreteType::Int64),
                LatticeType::Concrete(ConcreteType::Float64),
            ) |
            (
                LatticeType::Concrete(ConcreteType::Float64),
                LatticeType::Concrete(ConcreteType::Int64),
            ) => LatticeType::Concrete(ConcreteType::Float64),
            _ => LatticeType::Top,
        }
    }

    /// 関数呼び出しの返り値型を取得
    pub fn infer_return_type(&self, func_name: &str, arg_types: &[LatticeType]) -> LatticeType {
        if let Some(tfunc) = self.funcs.get(func_name) {
            tfunc(arg_types)
        } else {
            LatticeType::Top
        }
    }
}
```

### Phase 4: 既存コードとの統合

#### 現在の ValueType との対応

```rust
// src/compile/types/bridge.rs (新規)

impl From<ValueType> for LatticeType {
    fn from(vt: ValueType) -> Self {
        match vt {
            ValueType::Int => LatticeType::Concrete(ConcreteType::Int64),
            ValueType::Float => LatticeType::Concrete(ConcreteType::Float64),
            ValueType::Bool => LatticeType::Concrete(ConcreteType::Bool),
            ValueType::String => LatticeType::Concrete(ConcreteType::String),
            ValueType::Char => LatticeType::Concrete(ConcreteType::Char),
            ValueType::Nothing => LatticeType::Concrete(ConcreteType::Nothing),
            ValueType::Symbol => LatticeType::Concrete(ConcreteType::Symbol),
            ValueType::Array => LatticeType::Concrete(ConcreteType::Array {
                element: Box::new(ConcreteType::Int64), // 要素型不明時のデフォルト
            }),
            ValueType::Tuple => LatticeType::Concrete(ConcreteType::Tuple {
                elements: vec![],
            }),
            ValueType::Any => LatticeType::Top,
            _ => LatticeType::Top,
        }
    }
}

impl From<LatticeType> for ValueType {
    fn from(lt: LatticeType) -> Self {
        match lt {
            LatticeType::Bottom => ValueType::Nothing,
            LatticeType::Concrete(ct) => match ct {
                ConcreteType::Int64 => ValueType::Int,
                ConcreteType::Float64 => ValueType::Float,
                ConcreteType::Bool => ValueType::Bool,
                ConcreteType::String => ValueType::String,
                ConcreteType::Char => ValueType::Char,
                ConcreteType::Nothing => ValueType::Nothing,
                ConcreteType::Symbol => ValueType::Symbol,
                ConcreteType::Array { .. } => ValueType::Array,
                ConcreteType::Tuple { .. } => ValueType::Tuple,
                _ => ValueType::Any,
            },
            LatticeType::Union(_) => ValueType::Any, // Union は Any に
            LatticeType::Conditional { .. } => ValueType::Any,
            LatticeType::Top => ValueType::Any,
        }
    }
}
```

---

## 実装計画

### Phase 1: 基盤整備 (推定作業量: 小)

1. **型格子モジュールの追加**
   - `src/compile/types/lattice.rs` 作成
   - `LatticeType`, `ConcreteType` の定義
   - `join`, `is_subtype_of` の実装

2. **ブリッジモジュール**
   - `src/compile/types/bridge.rs` 作成
   - `ValueType` ↔ `LatticeType` 変換

### Phase 2: 推論エンジン (推定作業量: 中)

1. **TypeInferenceEngine の実装**
   - `src/compile/inference/engine.rs` 作成
   - 抽象解釈ループ
   - 環境管理

2. **条件分岐での型絞り込み**
   - `split_env_by_condition` の実装
   - `isa` チェック対応
   - `=== nothing` パターン対応

### Phase 3: 転送関数 (推定作業量: 中)

1. **TransferFunctions の実装**
   - `src/compile/inference/tfuncs.rs` 作成
   - 算術/比較/配列/文字列操作の登録

2. **ユーザー定義関数の返り値型キャッシュ**
   - 関数本体からの返り値型収集
   - キャッシュ管理

### Phase 4: ループ変数の型推論 (推定作業量: 小)

1. **ForEach の要素型推論**
   - イテレータ型からの要素型抽出
   - Range, Array, Tuple 対応

### Phase 5: 統合とテスト (推定作業量: 中)

1. **既存コンパイラとの統合**
   - `compile/expr/infer.rs` への新エンジン適用
   - 段階的な移行

2. **テストケース追加**
   - 型推論精度テスト
   - 回帰テスト

---

## テスト戦略

### 型推論精度テスト

```julia
# tests/fixtures/type_inference/basic.jl

# 基本的な数値型推論
function test_numeric()
    a = 1       # a: Int64
    b = 2.0     # b: Float64
    c = a + b   # c: Float64
    c
end

# ループ変数の型推論
function test_loop()
    arr = [1, 2, 3]  # arr: Array{Int64}
    sum = 0
    for x in arr     # x: Int64 (推論)
        sum += x     # sum: Int64
    end
    sum
end

# 条件分岐での型絞り込み
function test_narrowing(val)
    if val isa Int
        val + 1  # val: Int64
    else
        0
    end
end

# Union 型の推論
function test_union(flag)
    if flag
        result = 1    # Int64
    else
        result = 2.0  # Float64
    end
    result  # Union{Int64, Float64}
end
```

### 期待される推論結果の検証

```rust
#[test]
fn test_loop_variable_inference() {
    let code = r#"
        function test()
            arr = [1, 2, 3]
            for x in arr
                println(x)
            end
        end
    "#;

    let engine = TypeInferenceEngine::new();
    // x の型が Int64 と推論されることを検証
}

#[test]
fn test_type_narrowing() {
    let code = r#"
        function test(val)
            if val isa Int
                val + 1
            end
        end
    "#;

    let engine = TypeInferenceEngine::new();
    // if ブロック内で val が Int64 と推論されることを検証
}
```

---

## 関連ドキュメント

- [docs/vm/STATUS.md](../STATUS.md) - プロジェクト状況
- [docs/vm/DESIGN.md](../DESIGN.md) - VM 設計
- [docs/aot/README.md](../../aot/README.md) - AoT コンパイル設計
- [docs/aot/IMPLEMENTATION_GUIDE.md](../../aot/IMPLEMENTATION_GUIDE.md) - 実装ガイド

## 参考文献

- [Julia Compiler Internals](https://docs.julialang.org/en/v1/devdocs/inference/) - Julia 公式ドキュメント
- `julia/base/compiler/abstractinterpretation.jl` - 抽象解釈実装
- `julia/base/compiler/typelattice.jl` - 型格子実装
- `julia/base/compiler/tfuncs.jl` - 転送関数定義
