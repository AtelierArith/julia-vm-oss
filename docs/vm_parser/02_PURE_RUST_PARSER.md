# subset_julia_vm_parser

Pure Rust で実装された Julia サブセットパーサー。すべてのプラットフォーム（Native/WASM）で使用される唯一のパーサー。

## 概要

| 項目 | 値 |
|------|-----|
| クレート名 | `subset_julia_vm_parser` |
| バージョン | 0.1.0 |
| 依存 Lexer | `logos` 0.15 |
| ターゲット | Native + WASM |
| テスト数 | 373 passed (0 ignored) |
| C 依存 | なし |

## 特徴

- **Pure Rust**: C 依存なし、完全にポータブル
- **WASM 互換**: `wasm32-unknown-unknown` でコンパイル可能
- **統一パイプライン**: Native/WASM で同一コード

```
All Targets:
└── ソースコード → subset_julia_vm_parser (Rust) → CST → Lowering
    ↑ 単一のコードパス
```

## ディレクトリ構成

```
subset_julia_vm_parser/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Public API (parse, tokenize)
│   ├── lexer.rs         # Lexer (logos 使用)
│   ├── token.rs         # Token 定義 + 優先順位
│   ├── parser.rs        # Recursive descent + Pratt parser
│   ├── cst.rs           # CstNode 定義
│   ├── node_kind.rs     # NodeKind enum (100+ 種別)
│   ├── span.rs          # Span 情報
│   └── error.rs         # ParseError
└── tests/
```

## 実装状況

### Phase 1: 基盤 ✅
- [x] Cargo.toml 作成
- [x] Token enum 定義 (logos)
- [x] Span 構造体
- [x] CstNode 構造体
- [x] 基本的な Lexer

### Phase 2: リテラル・識別子 ✅
- [x] IntegerLiteral (0b, 0o, 0x, アンダースコア対応)
- [x] FloatLiteral
- [x] StringLiteral (補間対応)
- [x] CharacterLiteral
- [x] Identifier (Unicode対応)
- [x] BooleanLiteral

### Phase 3: 式パーサー ✅
- [x] Pratt parser for operator precedence (28段階)
- [x] BinaryExpression
- [x] UnaryExpression
- [x] CallExpression
- [x] BroadcastCallExpression
- [x] IndexExpression
- [x] FieldExpression
- [x] RangeExpression
- [x] TernaryExpression
- [x] TypedExpression
- [x] ArrowFunctionExpression

### Phase 4: 制御構文 ✅
- [x] FunctionDefinition
- [x] MacroDefinition
- [x] IfStatement (elseif/else)
- [x] ForStatement
- [x] WhileStatement
- [x] TryStatement (catch/finally)
- [x] BreakStatement, ContinueStatement, ReturnStatement
- [x] LetExpression, BeginBlock
- [x] ConstDeclaration, GlobalDeclaration, LocalDeclaration
- [x] UsingStatement, ImportStatement, ExportStatement
- [x] StructDefinition, MutableStructDefinition
- [x] AbstractDefinition
- [x] ModuleDefinition, BaremoduleDefinition

### Phase 5: 配列・内包表記 ✅
- [x] VectorExpression `[1, 2, 3]`
- [x] MatrixExpression `[1 2; 3 4]`
- [x] MatrixRow
- [x] ComprehensionExpression `[x for x in 1:10]`
- [x] Generator `(x for x in 1:10)`
- [x] Generator in call `sum(x^2 for x in 1:10)`

### Phase 6: 統合テスト ✅
- [x] do 構文 `map([1,2,3]) do x; x^2 end`
- [x] マクロ呼び出し `@time`, `@assert`
- [x] 全 fixture ファイルのパース成功
- [x] 16/16 統合テスト成功

### Phase 6.5: 追加構文サポート ✅ (2024-12-30)
- [x] 文字リテラルのエスケープ拡張
  - [x] 16進エスケープ: `'\x41'`
  - [x] Unicodeエスケープ: `'\u0041'`, `'\U00000041'`
  - [x] 名前付きエスケープ: `'\N{GREEK SMALL LETTER ALPHA}'`
- [x] 文字列フィールドアクセス: `df."column name"`
- [x] モジュール修飾マクロ: `@Foo.bar x`, `@Foo.Bar.baz y`
- [x] ::演算子のクォート: `:(::)`
- [x] ブロードキャスト複合代入: `.+=`, `.-=`, `.*=`, `./=`, `.^=`, `.%=`, `.\\=`, `.//=`

### Phase 7: subset_julia_vm 統合 ✅ 完了 (2025-12-30)
- [x] ParseOutcome に PureRust variant 追加
- [x] 統一 Node enum で両パーサーをサポート
- [x] lowering 統合 (17 ファイル更新)
- [x] tree-sitter を optional 化
- [x] WASM/Native 統一パイプライン実現

## API

### 基本使用法

```rust
use subset_julia_vm_parser::{parse, parse_with_errors, NodeKind};

// エラーがあれば失敗
let cst = parse("1 + 2").expect("parse failed");
assert_eq!(cst.kind, NodeKind::SourceFile);

// エラー回復付きパース
let (cst, errors) = parse_with_errors("1 + ");
assert!(!errors.is_empty());
```

### CST 構造

```rust
pub struct CstNode {
    pub kind: NodeKind,
    pub span: Span,
    pub children: Vec<CstNode>,
    pub text: Option<String>,  // リーフノード用
}

pub struct Span {
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_column: usize,
    pub end_column: usize,
}
```

### トークナイズ

```rust
use subset_julia_vm_parser::tokenize;

let tokens = tokenize("1 + 2");
// Vec<Result<SpannedToken, ParseError>>
```

## 演算子優先順位

tree-sitter-julia と同一の優先順位を実装:

| 優先順位 | 名前 | 演算子例 | 結合性 |
|---------|------|---------|-------|
| -2 | Assign | `=`, `+=`, `-=` | 右 |
| 10 | Afunc | `->` (ラムダ) | 右 |
| 11 | Pair | `=>` | 右 |
| 12 | Conditional | `? :` | 右 |
| 13 | Arrow | `<--`, `-->`, `↔` | 右 |
| 14 | LazyOr | `\|\|` | 左 |
| 15 | LazyAnd | `&&` | 左 |
| 17 | Comparison | `<`, `>`, `==`, `∈`, `isa` | 左 |
| 18 | PipeLeft | `<\|` | 右 |
| 19 | PipeRight | `\|>` | 左 |
| 20 | Colon | `:` (range), `…` | 左 |
| 21 | Plus | `+`, `-`, `\|`, `∪` | 左 |
| 22 | Times | `*`, `/`, `%`, `&`, `∩` | 左 |
| 23 | Rational | `//` | 左 |
| 24 | Bitshift | `<<`, `>>`, `>>>` | 左 |
| 25 | Prefix | 前置単項 | 右 |
| 26 | Power | `^` | 右 |
| 27 | Decl | `::` (型注釈) | - |
| 28 | Dot | `.` (フィールド) | - |

## NodeKind 一覧 (主要)

### 定義
- `SourceFile`, `Block`
- `FunctionDefinition`, `MacroDefinition`
- `StructDefinition`, `MutableStructDefinition`
- `AbstractDefinition`, `PrimitiveDefinition`
- `ModuleDefinition`, `BaremoduleDefinition`

### 制御構文
- `IfStatement`, `ElseifClause`, `ElseClause`
- `ForStatement`, `WhileStatement`
- `TryStatement`, `CatchClause`, `FinallyClause`
- `BreakStatement`, `ContinueStatement`, `ReturnStatement`
- `LetExpression`, `BeginBlock`

### 式
- `BinaryExpression`, `UnaryExpression`
- `CallExpression`, `BroadcastCallExpression`
- `IndexExpression`, `FieldExpression`
- `RangeExpression`, `TernaryExpression`
- `ArrowFunctionExpression`, `DoClause`
- `TypedExpression`, `ParametrizedTypeExpression`
- `MacrocallExpression`

### リテラル
- `IntegerLiteral`, `FloatLiteral`
- `StringLiteral`, `CharacterLiteral`
- `BooleanLiteral`
- `Identifier`

### 配列
- `VectorExpression`, `MatrixExpression`, `MatrixRow`
- `ComprehensionExpression`, `Generator`
- `ForClause`, `IfClause`

### その他
- `TupleExpression`, `ParenthesizedExpression`
- `Assignment`, `CompoundAssignmentExpression`
- `UsingStatement`, `ImportStatement`, `ExportStatement`
- `ConstDeclaration`, `GlobalDeclaration`, `LocalDeclaration`

## AST Debugging / デバッグ

### CLI Tool: `sjulia --dump-ast`

The `sjulia` CLI provides AST visualization for debugging parser issues:

```bash
# Human-readable output with source code and line annotations
sjulia --dump-ast -e "x = 1 + 2"

# From file
sjulia --dump-ast path/to/file.jl

# JSON output for machine processing / scripting
sjulia --dump-ast --json -e "x = 1 + 2"
sjulia --dump-ast --json path/to/file.jl
```

### Human-readable Output Example

```
=== Source Code ===

   1 | x = 1 + 2

=== AST Structure ===

SourceFile [L1:1]
  Assignment [L1:1]
    Identifier = "x" [L1:1]
    Operator = "=" [L1:3]
    BinaryExpression [L1:5]
      IntegerLiteral = "1" [L1:5]
      Operator = "+" [L1:7]
      IntegerLiteral = "2" [L1:9]
```

The `[Lx:y]` annotation shows `[Line:Column]` for each node.

### JSON Output Format

```json
{
  "ast": { ... },       // Full AST tree
  "errors": [],         // Parse errors (if any)
  "has_error": false,   // Whether tree contains error nodes
  "source_lines": [     // Source code with line numbers
    { "line": 1, "content": "x = 1 + 2" }
  ]
}
```

### Programmatic API

```rust
use subset_julia_vm_parser::{parse_with_errors, CstNode};

let (cst, errors) = parse_with_errors("x = 1 + 2");

// Print AST structure
cst.debug_ast(0);

// Get AST as string
let ast_string = cst.debug_ast_string();

// JSON serialization
let json = cst.to_json();
```

### Debugging Workflow

1. **When a parser test fails**: Use `sjulia --dump-ast` to visualize the actual AST
2. **Compare with expected**: Check NodeKind and field names match expectations
3. **Locate issues**: Use line annotations `[L:C]` to find source location
4. **For CI/scripting**: Use `--json` flag for machine-readable output

## テスト

```bash
# パーサーテスト
cargo test --manifest-path subset_julia_vm_parser/Cargo.toml

# 結果 (2025-12-30):
# - lib (unit tests): 30 passed
# - corpus_collections: 43 passed
# - corpus_definitions: 49 passed
# - corpus_expressions: 51 passed
# - corpus_literals: 25 passed
# - corpus_operators: 46 passed
# - corpus_statements: 58 passed
# - parser_tests: 69 passed
# - doc-tests: 2 passed
# 合計: 373+ passed, 0 ignored

# 統合テスト (subset_julia_vm から)
cargo test --manifest-path subset_julia_vm/Cargo.toml

# 結果 (2025-12-30):
# - 617 integration tests passed
# - 94 fixture tests passed
```

## 既知の制限

### 簡略化された実装
1. キーワード引数の先読み
   - `is_keyword_argument()` は常に false を返す
   - 代入式として正しくパースされるため動作に問題なし

2. 文字列補間
   - 基本的な `$var` と `$(expr)` をサポート
   - 複雑なネストケースは未テスト

3. ブロックコメント `#= ... =#`
   - 単純なブロックコメントは動作
   - 深くネストしたケースは未テスト

## 技術選定

| コンポーネント | ライブラリ | 理由 |
|--------------|-----------|------|
| Lexer | `logos` 0.15 | 高速、WASM対応、Unicode対応 |
| Serialization | `serde` | JSON 出力用 |
| Error handling | `thiserror` | エラー型定義 |

## 参考資料

- [tree-sitter-julia grammar.js](../tree-sitter-julia/grammar.js)
- [Julia Parser (公式)](https://github.com/JuliaLang/julia/blob/master/src/julia-parser.scm)
- [logos ドキュメント](https://docs.rs/logos/)
- [Pratt Parser 解説](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
