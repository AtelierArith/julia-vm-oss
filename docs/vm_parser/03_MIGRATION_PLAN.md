# Pure Rust Parser 移行計画 (完了)

> **状態**: ✅ 完了 (2025-12-30) - tree-sitter-julia 依存を完全に削除

## 概要

`subset_julia_vm_parser` (Pure Rust) への移行が完了。WASM/Native 統一パイプラインを達成。

## 現状

### 完了済み

| Phase | 内容 | 状態 |
|-------|------|------|
| 1 | 基盤 (lexer, token, cst, span, error) | ✅ |
| 2 | リテラル・識別子 | ✅ |
| 3 | 式パーサー (Pratt parser) | ✅ |
| 4 | 制御構文 | ✅ |
| 5 | 配列・内包表記 | ✅ |
| 6 | tree-sitter-julia テスト移植 | ✅ |
| 7 | subset_julia_vm 統合 | ✅ 完了 (2025-12-30) |

### テスト結果 (2025-12-30 更新)

```
# subset_julia_vm_parser 単体テスト
373+ passed, 0 failed, 0 ignored

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

# subset_julia_vm 統合テスト
- 617 integration tests passed
- 94 fixture tests passed
```

### 追加実装済み機能 (Phase 6 で追加)

- パラメトリック型: `A{T}`, `Dict{K, V}`
- スプラット演算子: `x...`
- アジョイント演算子: `A'`
- ブロードキャスト比較: `.<`, `.>`, `.<=`, `.>=`, `.==`, `.!=`
- 演算子を値として使用: `(+)`, `map(+, a, b)`
- public 文: `public foo, bar` (Julia 1.11+)
- primitive type: `primitive type Int128 128 end`
- コマンドリテラル: `` `ls -la` ``

### 追加実装済み機能 (Phase 6.5 - 2024-12-30)

- 文字リテラルのエスケープ拡張:
  - 16進エスケープ: `'\x41'`
  - Unicodeエスケープ: `'\u0041'`, `'\U00000041'`
  - 名前付きエスケープ: `'\N{GREEK SMALL LETTER ALPHA}'`
- 文字列フィールドアクセス: `df."column name"`
- モジュール修飾マクロ: `@Foo.bar x`, `@Foo.Bar.baz y`
- ::演算子のクォート: `:(::)`
- ブロードキャスト複合代入: `.+=`, `.-=`, `.*=`, `./=`, `.^=`, `.%=`, `.\\=`, `.//=`

> **完了**: 統合作業の詳細は [04_INTEGRATION_PLAN.md](./04_INTEGRATION_PLAN.md) を参照

## Phase 7: 統合作業 ✅ 完了

### Step 1: ParseOutcome の拡張

```rust
// subset_julia_vm/src/parser/mod.rs

#[cfg(feature = "parser")]
pub enum ParseOutcome {
    TreeSitter(tree_sitter::ParsedSource),
    PureRust(subset_julia_vm_parser::CstNode),  // 追加
}
```

### Step 2: CstWalker の統一

現在の構造:
```
parser/cst.rs         - NodeKind enum + CstWalker for tree-sitter
parser/json_cst.rs    - JsonCstNode + JsonCstWalker for WASM
```

統一後:
```
parser/cst.rs         - 共通の CstWalker trait
parser/tree_sitter.rs - tree-sitter 用実装 (optional)
parser/pure_rust.rs   - Pure Rust 用実装 (新規)
```

### Step 3: Lowering の対応

```rust
// 抽象化された CstWalker trait
pub trait CstWalker {
    type Node;
    fn kind(&self, node: &Self::Node) -> NodeKind;
    fn text(&self, node: &Self::Node) -> &str;
    fn span(&self, node: &Self::Node) -> Span;
    fn children(&self, node: &Self::Node) -> Vec<Self::Node>;
    fn named_children(&self, node: &Self::Node) -> Vec<Self::Node>;
}
```

### Step 4: Feature Flags の整理

```toml
# subset_julia_vm/Cargo.toml

[features]
default = ["pure-rust-parser"]  # デフォルトを Pure Rust に
pure-rust-parser = []           # Pure Rust パーサー
tree-sitter-parser = ["dep:tree-sitter", "dep:tree-sitter-julia"]  # オプション
wasm = ["dep:js-sys", "pure-rust-parser"]  # WASM は必ず Pure Rust
```

### Step 5: json_lowering の廃止

Pure Rust パーサーが WASM 対応すれば不要:

```
削除対象:
├── json_lowering/
│   ├── mod.rs
│   ├── expr.rs
│   ├── stmt.rs
│   ├── function.rs
│   └── struct_.rs
```

## NodeKind マッピング

### 名前の違い

| tree-sitter | Pure Rust | 備考 |
|-------------|-----------|------|
| `generator_expression` | `Generator` | 短縮形 |
| `compound_statement` | `CompoundStatement` / `BeginBlock` | 同一 |
| `coefficient_expression` | `JuxtapositionExpression` | 別名 |
| `elseif_clause` | `ElseifClause` | tree-sitter は `else_clause` として処理 |

### 追加されたノード (Pure Rust のみ)

```rust
// Phase 4 で追加
MacroDefinition
BaremoduleDefinition
LetBindings
PublicStatement

// 構造化
ParameterList
Parameter
TypeParameters
TypeParameter
WhereClause
```

## WASM ビルドの検証 ✅ 完了

### 移行前の WASM アーキテクチャ

```
Web (旧)
└── subset_julia_vm_web (Rust/WASM)
    ├── web-tree-sitter (JavaScript)
    │   └── tree-sitter-julia.wasm
    └── json_lowering (Rust)
        └── CST JSON を受け取って処理
```

### 現在のアーキテクチャ (統一済み)

```
Web/Native 統一
└── subset_julia_vm (Rust/WASM)
    └── subset_julia_vm_parser (Pure Rust)
        └── 直接パース (JavaScript 不要)
```

### WASM ビルドコマンド

```bash
# subset_julia_vm_parser のみ
cd subset_julia_vm_parser
wasm-pack build --target web

# subset_julia_vm_web (統合後)
cd subset_julia_vm_web
wasm-pack build --target web --features pure-rust-parser
```

## リスクと対策

### 1. パース結果の互換性

**リスク**: tree-sitter と Pure Rust で CST 構造が異なる可能性

**対策**:
- 統合テストで主要構文の CST 比較
- Lowering レベルで吸収できる差異は許容

### 2. パフォーマンス

**リスク**: Pure Rust が tree-sitter より遅い可能性

**対策**:
- ベンチマークテストの追加
- 必要に応じて最適化 (logos の効率性に依存)

### 3. エッジケース

**リスク**: tree-sitter がハンドルしていた特殊ケースの見落とし

**対策**:
- fixture テストの拡充
- 実際の Julia コードでの検証

## タイムライン

```
Phase 7.1: ParseOutcome 拡張 ✅
Phase 7.2: CstWalker trait 化 ✅ (enum ベースで実装)
Phase 7.3: Lowering 対応 ✅
Phase 7.4: WASM ビルド検証 ✅
Phase 7.5: json_lowering 廃止 ⏳ (一部残存、将来削除予定)
Phase 7.6: tree-sitter の optional 化 ✅
Phase 7.7: ドキュメント更新 ✅
```

## 検証チェックリスト

### 機能テスト
- [x] fixture テストが全て通る (94 tests)
- [x] REPL が正常動作 (sjulia CLI)
- [x] iOS アプリが正常動作
- [x] Web アプリが正常動作

### パフォーマンステスト
- [x] パース時間: 許容範囲内
- [x] WASM バンドルサイズ: web-tree-sitter 不要で削減

### 互換性テスト
- [x] 既存の Julia コードサンプルがパースできる
- [x] エラーメッセージが適切

## 参考ファイル

### 変更対象
```
subset_julia_vm/
├── Cargo.toml                    # 依存関係
├── src/
│   ├── parser/
│   │   ├── mod.rs                # ParseOutcome 拡張
│   │   ├── cst.rs                # CstWalker trait 化
│   │   └── pure_rust.rs          # 新規: Pure Rust アダプタ
│   └── lowering/                 # CstWalker 使用箇所の更新
│       ├── mod.rs
│       ├── expr.rs
│       ├── stmt/
│       ├── function.rs
│       └── struct_.rs

subset_julia_vm_web/
├── Cargo.toml                    # feature flags
└── src/lib.rs                    # パーサー切り替え
```

### 将来の削除対象 (現在は残存)
```
subset_julia_vm/src/json_lowering/  # 将来削除予定
subset_julia_vm/src/parser/json_cst.rs  # 将来削除予定
```

**注**: 現在は両パーサーが共存可能。Pure Rust パーサーがデフォルトで使用される。
