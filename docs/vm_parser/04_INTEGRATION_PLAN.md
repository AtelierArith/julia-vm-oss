# subset_julia_vm への統合計画

**状態**: ✅ 完了 (2025-12-30)

## 現状 (2025-12-30)

### subset_julia_vm_parser の完成度

```
テスト結果: 373 passed, 0 ignored

内訳:
- lib (unit tests): 30 passed
- corpus_collections: 43 passed
- corpus_definitions: 49 passed
- corpus_expressions: 51 passed
- corpus_literals: 25 passed
- corpus_operators: 46 passed
- corpus_statements: 58 passed
- parser_tests: 69 passed
- doc-tests: 2 passed
```

### 実装済み機能

| カテゴリ | 機能 |
|---------|------|
| リテラル | Integer, Float, String, Char (with `\x`, `\u`, `\N{}`), Bool, Command |
| 式 | Binary, Unary, Ternary, Call, Index, Field, Broadcast |
| 演算子 | 28段階の優先順位、Unicode演算子、ブロードキャスト、ブロードキャスト複合代入 (`.+=`等) |
| 型 | パラメトリック型 `A{T}`、型宣言 `x::T`、`:(::)`クォート |
| 制御構文 | if/elseif/else, for, while, try/catch/finally |
| 定義 | function, struct, abstract type, primitive type, module |
| その他 | splat `...`, adjoint `'`, operators as values `(+)`, string field access `df."col"` |
| マクロ | `@macro`, `@Module.macro`, `@Foo.Bar.baz` (モジュール修飾) |

## 統合の目標 ✅ 達成

1. **Native/WASM 統一パイプライン**: ✅ Pure Rust パーサーで統一
2. **依存関係の削減**: ✅ C コンパイラ不要に
3. **WASM バンドルサイズ削減**: ✅ web-tree-sitter (~1.5MB) が不要に

## 統合ステップ

### Step 1: 依存関係の追加

```toml
# subset_julia_vm/Cargo.toml

[dependencies]
subset_julia_vm_parser = { path = "../subset_julia_vm_parser" }

[features]
default = ["pure-rust-parser"]
pure-rust-parser = []
tree-sitter-parser = ["dep:tree-sitter", "dep:tree-sitter-julia"]
```

### Step 2: CstWalker trait の定義

```rust
// subset_julia_vm/src/parser/walker.rs

use subset_julia_vm_parser::{CstNode, NodeKind, Span};

/// CST を走査するための共通インターフェース
pub trait CstWalker {
    type Node;

    fn kind(&self, node: &Self::Node) -> NodeKind;
    fn text(&self, node: &Self::Node) -> Option<&str>;
    fn span(&self, node: &Self::Node) -> Span;
    fn children(&self, node: &Self::Node) -> &[Self::Node];
    fn child_by_field(&self, node: &Self::Node, field: &str) -> Option<&Self::Node>;
}

/// Pure Rust パーサー用の実装
pub struct PureRustWalker;

impl CstWalker for PureRustWalker {
    type Node = CstNode;

    fn kind(&self, node: &CstNode) -> NodeKind {
        node.kind
    }

    fn text(&self, node: &CstNode) -> Option<&str> {
        node.text.as_deref()
    }

    fn span(&self, node: &CstNode) -> Span {
        node.span
    }

    fn children(&self, node: &CstNode) -> &[CstNode] {
        &node.children
    }

    fn child_by_field(&self, node: &CstNode, field: &str) -> Option<&CstNode> {
        // field 名による子ノード検索 (実装方法は要検討)
        None
    }
}
```

### Step 3: Lowering の更新

```rust
// subset_julia_vm/src/lowering/mod.rs

use crate::parser::walker::{CstWalker, PureRustWalker};

pub struct Lowerer<W: CstWalker> {
    walker: W,
    source: String,
}

impl<W: CstWalker> Lowerer<W> {
    pub fn lower(&mut self, node: &W::Node) -> Result<CoreExpr, LoweringError> {
        match self.walker.kind(node) {
            NodeKind::BinaryExpression => self.lower_binary(node),
            NodeKind::CallExpression => self.lower_call(node),
            // ...
        }
    }
}

// 使用例
pub fn lower_source(source: &str) -> Result<CoreExpr, LoweringError> {
    let (cst, errors) = subset_julia_vm_parser::parse(source);
    if !errors.is_empty() {
        return Err(LoweringError::ParseError(errors));
    }

    let mut lowerer = Lowerer {
        walker: PureRustWalker,
        source: source.to_string(),
    };
    lowerer.lower(&cst)
}
```

### Step 4: NodeKind のマッピング

```rust
// subset_julia_vm/src/parser/node_kind_compat.rs

use subset_julia_vm_parser::NodeKind as ParserNodeKind;

/// tree-sitter との NodeKind 差分を吸収
pub fn normalize_kind(kind: ParserNodeKind) -> ParserNodeKind {
    match kind {
        // 名前の違いを吸収
        ParserNodeKind::Generator => ParserNodeKind::Generator,
        ParserNodeKind::JuxtapositionExpression => ParserNodeKind::JuxtapositionExpression,

        // そのまま
        other => other,
    }
}
```

### Step 5: WASM 対応の確認

```rust
// subset_julia_vm_web/src/lib.rs

use subset_julia_vm_parser::parse;
use subset_julia_vm::lowering::lower_source;

#[wasm_bindgen]
pub fn compile_and_run(source: &str, seed: u64) -> Result<f64, JsValue> {
    // Pure Rust パーサーを直接使用 (JavaScript 不要)
    let core_ir = lower_source(source)
        .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

    let bytecode = compile(&core_ir)?;
    let result = vm::run(&bytecode, seed)?;

    Ok(result)
}
```

### Step 6: json_lowering の廃止

```bash
# 削除対象ファイル
subset_julia_vm/src/json_lowering/
├── mod.rs
├── expr.rs
├── stmt/
│   ├── mod.rs
│   ├── control.rs
│   └── definition.rs
├── function.rs
└── struct_.rs

subset_julia_vm/src/parser/json_cst.rs
```

### Step 7: tree-sitter の optional 化

```toml
# subset_julia_vm/Cargo.toml

[dependencies]
# tree-sitter は optional に
tree-sitter = { version = "0.20", optional = true }
tree-sitter-julia = { path = "../tree-sitter-julia", optional = true }

[features]
default = ["pure-rust-parser"]
pure-rust-parser = ["dep:subset_julia_vm_parser"]
tree-sitter-parser = ["dep:tree-sitter", "dep:tree-sitter-julia"]
```

## 検証計画

### 機能テスト

```bash
# 1. fixture テスト (既存の動作確認)
cargo test --test fixture_tests

# 2. 公式 Julia との比較
julia scripts/run_julia_tests.jl
./scripts/compare_all.sh

# 3. REPL テスト
cargo run --bin sjulia
```

### WASM ビルドテスト

```bash
# 1. パーサー単体
cd subset_julia_vm_parser
wasm-pack build --target web

# 2. VM 全体
cd subset_julia_vm_web
wasm-pack build --target web
```

### パフォーマンステスト

```rust
// benches/parser_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_parse(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/complex/large_program.jl");

    c.bench_function("pure_rust_parse", |b| {
        b.iter(|| subset_julia_vm_parser::parse(source))
    });
}
```

## 懸念事項と対策

### 1. フィールドアクセスの違い

**問題**: tree-sitter は `child_by_field_name("condition")` のようなアクセスが可能

**対策**:
- Pure Rust の CstNode に field 情報を追加
- または、子ノードの順序を固定して index でアクセス

### 2. エラーメッセージの互換性

**問題**: エラースパンの精度が異なる可能性

**対策**:
- テストでエラーメッセージを検証
- Span 情報は Pure Rust の方が詳細なので問題なし

### 3. 簡略化された実装

**現時点の簡略化**:
- キーワード引数の先読み (`is_keyword_argument()` は常に false)
- 深くネストしたブロックコメント `#= #= =# =#`
- 複雑なネストの文字列補間

**対策**:
- 必要に応じて追加実装
- SubsetJuliaVM がサポートしない機能は UnsupportedFeature エラーで対応

## 作業順序 ✅ 完了

```
1. [x] Cargo.toml に依存関係追加
2. [x] CstWalker trait 定義 (Node enum ベースで実装)
3. [x] PureRustWalker 実装
4. [x] lowering/expr.rs を trait ベースに更新
5. [x] lowering/stmt/*.rs を更新
6. [x] lowering/function.rs を更新
7. [x] lowering/struct_.rs を更新
8. [x] fixture テストで動作確認 (94 tests passed)
9. [x] WASM ビルド確認
10. [ ] json_lowering 削除 (将来対応)
11. [x] tree-sitter を optional に
12. [x] ドキュメント更新
```

## 見積もり

| ステップ | 作業量 |
|---------|--------|
| Step 1-2 (依存関係 + trait) | 小 |
| Step 3 (Lowering 更新) | 大 (最も時間がかかる) |
| Step 4 (NodeKind マッピング) | 小 |
| Step 5 (WASM 確認) | 中 |
| Step 6-7 (削除 + optional化) | 小 |

**クリティカルパス**: Step 3 (Lowering の更新) が最も重要
