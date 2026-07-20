# Web Playground アーキテクチャ

## 概要

SubsetJuliaVM の Web Playground は、Pure Rust パーサー (`subset_julia_vm_parser`) を使用することで、Native/iOS と完全に同一のパイプラインを実現しています。これにより、シンプルで一貫性のあるアーキテクチャが実現できました。

## 現在のアーキテクチャ（2025-12-30 以降）

### シンプルなフロー

```
Julia source (JavaScript)
    ↓
Rust WASM (parse → lower → compile → execute)
    ↓
Result (JavaScript)
```

Pure Rust パーサーがデフォルトになったことで、**理想的なシンプルなフローが実現できました**。

### パイプラインの詳細

```
┌─────────────────────────────────────────────────────────┐
│ JavaScript (ブラウザ)                                    │
│                                                         │
│  ┌─────────────┐                                       │
│  │ Monaco      │ → ユーザーが Julia コードを入力        │
│  │ Editor      │                                       │
│  └─────┬───────┘                                       │
│        │ source (string)                               │
└────────┼───────────────────────────────────────────────┘
         │
┌────────┼───────────────────────────────────────────────┐
│ Rust WASM                                               │
│                                                         │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │ Pure Rust   │ → │ Lowering     │ → │ Compiler  │  │
│  │ Parser      │    │ (CST→IR)     │    │ (IR→BC)   │  │
│  │ (subset_    │    │              │    │           │  │
│  │  julia_vm_  │    │              │    │           │  │
│  │  parser)    │    │              │    │           │  │
│  └─────────────┘    └──────────────┘    └─────┬─────┘  │
│                                               │        │
│                                               ↓        │
│                                         ┌───────────┐  │
│                                         │ VM        │  │
│                                         │ (実行)    │  │
│                                         └─────┬─────┘  │
│                                               │        │
└───────────────────────────────────────────────┼────────┘
                                                 │
                                                 ↓
┌─────────────────────────────────────────────────────────┐
│ JavaScript (ブラウザ)                                    │
│                                                         │
│  ┌─────────────┐                                       │
│  │ Result      │ ← 実行結果（値、出力、エラー）         │
│  │ Display     │                                       │
│  └─────────────┘                                       │
└─────────────────────────────────────────────────────────┘
```

### 実装の詳細

#### JavaScript 側 (web/app.js)

```javascript
// シンプルな実行フロー
async function run() {
    const code = editor.getValue();
    const seed = 42;

    // Pure Rust パーサーで直接実行（Native と同一）
    const result = wasm.run_from_source(code, BigInt(seed));
    displayResult(result);
}
```

#### Rust 側 (subset_julia_vm_web/src/lib.rs)

```rust
#[wasm_bindgen]
pub fn run_from_source(source: &str, seed: u64) -> JsValue {
    // 1. Pure Rust パーサーでパース
    let cst = subset_julia_vm_parser::parse(source)?;

    // 2. CST → Core IR (Lowering)
    let mut lowering = Lowering::new(source);
    let program = lowering.lower(parse_outcome)?;

    // 3. IR → バイトコード (Compiler)
    let compiled = compile_core_program(&program)?;

    // 4. 実行 (VM)
    let mut vm = Vm::new_program(compiled, rng);
    vm.run()
}
```

### なぜ Pure Rust パーサーが採用されたか

#### 以前の問題（tree-sitter 使用時）

1. **WASM と C FFI の非互換性**
   - tree-sitter は C コードを生成し、Rust バインディングが C コードとリンクする必要がある
   - `wasm32-unknown-unknown` ターゲットでは C コードとのリンクができない
   - 以前は web-tree-sitter (JavaScript) でパースし、CST JSON を Rust に渡す必要があった

2. **二重管理による不整合**
   ```
   Native: ソースコード → tree-sitter → CST → Lowering
   Web:    ソースコード → web-tree-sitter (JS) → CST JSON → JsonLowering
   ```
   - Native では動作するが Web では動作しないケースが発生
   - `lowering/` と `json_lowering/` の二重実装が必要

#### 現在の解決策（Pure Rust パーサー）

1. **WASM ネイティブ対応**
   - Pure Rust 実装のため、`wasm-pack build` で直接 WASM にコンパイル可能
   - C 依存性なし、追加のツールチェーン不要

2. **統一パイプライン**
   ```
   All Targets: ソースコード → subset_julia_vm_parser → CST → Lowering
   ```
   - Native/iOS/Web で完全に同一のコードパス
   - 単一の Lowering 実装で全プラットフォーム対応

3. **メンテナンス性の向上**
   - 新機能追加時に一箇所の更新で全プラットフォーム対応
   - テストも統一（373+ parser tests, 617+ integration tests）

### コンポーネントの役割

| コンポーネント | 言語 | 役割 |
|--------------|------|------|
| Monaco Editor | JavaScript | コードエディタ（シンタックスハイライト、補完） |
| subset_julia_vm_parser | Rust (WASM) | Julia コードのパース（Pure Rust） |
| Lowering | Rust (WASM) | CST → Core IR 変換 |
| Compiler | Rust (WASM) | IR → バイトコード変換 |
| VM | Rust (WASM) | バイトコード実行 |

### 後方互換性

`run_from_cst_json()` 関数は後方互換性のために残されていますが、**推奨されません**。新しいコードは `run_from_source()` を使用してください。

```rust
// 推奨: Pure Rust パーサーを使用（Native と同一）
wasm.run_from_source(code, seed)

// 非推奨: CST JSON パイプライン（後方互換性のため残存）
wasm.run_from_cst_json(cst_json, source, seed)
```

## JavaScript 側の実装

### 最小限の実装

JavaScript 側で実装する必要があるのは、**WASM モジュールの読み込みと実行呼び出しのみ**です：

```javascript
// 1. WASM モジュールの読み込み（一度だけ）
async function loadWasm() {
    const module = await import('./pkg/subset_julia_vm_web.js');
    await module.default();
    wasm = module;
}

// 2. 実行（実行ごと）
async function run() {
    const code = editor.getValue();
    const seed = 42;

    // Pure Rust パーサーで直接実行
    const result = wasm.run_from_source(code, BigInt(seed));
    displayResult(result);
}
```

### 実装の簡素化

以前の web-tree-sitter ベースの実装と比較：

| 項目 | 以前（web-tree-sitter） | 現在（Pure Rust） |
|------|------------------------|-------------------|
| JavaScript 実装行数 | ~100 行（パーサー初期化、CST シリアライズ） | ~10 行（WASM 呼び出しのみ） |
| 依存関係 | web-tree-sitter, tree-sitter-julia.wasm | なし（WASM モジュールのみ） |
| パース処理 | JavaScript 側 | Rust WASM 側 |
| Native との互換性 | 異なるパイプライン | 完全に同一 |

### Rust 側で実装されているもの

以下は全て Rust WASM で実装済みのため、JavaScript は結果を受け取るだけです：

| 機能 | 関数 | 説明 |
|------|------|------|
| パース | `subset_julia_vm_parser::parse()` | Julia ソースコードを CST に変換 |
| CST → IR 変換 | `Lowering::lower()` | 構文木を中間表現に変換 |
| IR → バイトコード | `compile_core_program()` | 中間表現をコンパイル |
| 実行 | `Vm::run()` | バイトコードを実行 |
| 結果取得 | 自動 | 戻り値と出力を JSON で返却 |

### 責務の分離

```
┌─────────────────────────────────────────────────────────┐
│ JavaScript (ブラウザ)                                    │
│                                                         │
│  ┌─────────────┐                                       │
│  │ Monaco      │ → ユーザーが Julia コードを入力        │
│  │ Editor      │                                       │
│  └─────┬───────┘                                       │
│        │ source (string)                               │
│        │ wasm.run_from_source(code, seed)              │
└────────┼───────────────────────────────────────────────┘
         │
┌────────┼───────────────────────────────────────────────┐
│ Rust WASM                                               │
│                                                         │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────┐  │
│  │ Pure Rust   │ → │ Lowering     │ → │ Compiler  │  │
│  │ Parser      │    │ (CST→IR)     │    │ (IR→BC)   │  │
│  └─────────────┘    └──────────────┘    └─────┬─────┘  │
│                                               │        │
│                                               ↓        │
│                                         ┌───────────┐  │
│                                         │ VM        │  │
│                                         │ (実行)    │  │
│                                         └─────┬─────┘  │
│                                               │        │
└───────────────────────────────────────────────┼────────┘
                                                 │
                                                 ↓
┌─────────────────────────────────────────────────────────┐
│ JavaScript (ブラウザ)                                    │
│                                                         │
│  ┌─────────────┐                                       │
│  │ Result      │ ← 実行結果（値、出力、エラー）         │
│  │ Display     │                                       │
│  └─────────────┘                                       │
└─────────────────────────────────────────────────────────┘
```

### なぜこのアーキテクチャが最適か

1. **JavaScript 側の実装は最小限**
   - WASM モジュールの読み込みと関数呼び出しのみ（~10 行）
   - パース処理は Rust 側で完結

2. **ロジックは Rust に集約**
   - パース（Pure Rust パーサー）: Native と完全に同一
   - Lowering（CST → IR）: 複雑な変換ロジック
   - コンパイル（IR → バイトコード）: 最適化を含む
   - VM 実行: 全ての組み込み関数、型システム

3. **メンテナンス性**
   - Native/iOS/Web で完全に同一のコードパス
   - 新機能追加は Rust 側のみで完結
   - テストも統一（373+ parser tests, 617+ integration tests）

4. **パフォーマンス**
   - CST JSON シリアライズ/デシリアライズのオーバーヘッドなし
   - 直接的なパース → 実行フロー

## 関連ファイル

- `web/app.js` - JavaScript 側の実装（WASM 呼び出し）
- `subset_julia_vm_web/src/lib.rs` - WASM エントリポイント
- `subset_julia_vm_parser/` - Pure Rust パーサー（WASM 対応）
- `subset_julia_vm_lowering/src/lowering/` - CST → IR 変換（Native/Web 共通）
- `subset_julia_vm_compile/src/compile/` - IR → バイトコード変換
- `subset_julia_vm_vm/src/vm/` - バイトコード実行

## 移行履歴

### 2025-12-30: Pure Rust パーサーへの移行完了

- **変更前**: web-tree-sitter (JavaScript) → CST JSON → Rust WASM
- **変更後**: Pure Rust パーサー (WASM) → 直接実行
- **効果**: Native/Web 統一パイプライン、実装の簡素化、パフォーマンス向上
