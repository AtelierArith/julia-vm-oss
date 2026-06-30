# subset_julia_vm_web

SubsetJuliaVM の WebAssembly バインディング。ブラウザで Julia サブセットコードを実行するための
WASM ランタイムを提供し、`web/` の Playground から呼び出します。

## 目的と役割

- Rust VM を wasm32 にビルドし、ブラウザ側から実行できる形で提供
- Pure Rust パーサー (`subset_julia_vm_parser`) を使用して Native/iOS と完全に同一のパイプラインを実現
- Julia ソースコードを直接受け取り、WASM 内でパース→Lowering→実行

## アーキテクチャ概要

```
Julia source (JavaScript)
  ↓
Rust WASM (Pure Rust Parser → Lowering → Compile → VM)
  ↓
ExecutionResult (value/output/error)
```

**Native/iOS と完全に同一のコードパス**を使用します。

## ビルド

```bash
# 1. wasm32 ターゲット追加
rustup target add wasm32-unknown-unknown

# 2. wasm-pack のインストール (未導入の場合)
cargo install wasm-pack

# 3. WASM ビルド (subset_julia_vm_web ディレクトリ内)
wasm-pack build --target web --profile web-release --out-dir ../web/pkg
```

## ローカル開発

```bash
# WASM をビルドして簡易サーバー起動
wasm-pack build --target web --profile web-release --out-dir ../web/pkg && \
python3 -m http.server 8080 --directory ../web
```

ブラウザで http://localhost:8080 を開く。

## API

### ExecutionResult

```typescript
interface ExecutionResult {
  success: boolean;
  value: number;
  typed_value: unknown;
  output: string;
  error_message: string | null;
  artifact_mime: string | null;
  artifact_data: string | null;
}
```

### run_from_source(source: string, seed: number): ExecutionResult

**推奨**: Julia ソースコードを直接受け取り、Pure Rust パーサーでパース→実行します。
Native/iOS と完全に同一のパイプラインを使用します。
戻り値の `typed_value` には配列・複素数・構造体などの型タグ付き JSON object が入ります。

```javascript
import init, { run_from_source, run_from_source_typed } from './pkg/subset_julia_vm_web.js';

await init();
const result = run_from_source('println("Hello, World!")', 42);
const typed = run_from_source_typed('complex(1.5, 2.25)', 42);
console.log(typed.typed_value); // { type: "complex", real: 1.5, imag: 2.25, ... }
```

`run_from_source_typed` は `run_from_source` と同じ result shape を返す
typed-result 明示用 alias です。

### run_ir_json(ir_json: string, seed: number): ExecutionResult

IR JSON を受け取り、コンパイル・実行して結果を返す。
（後方互換性のため残存、新規コードでは `run_from_source()` を使用推奨）

```javascript
import init, { run_ir_json } from './pkg/subset_julia_vm_web.js';

await init();
const result = run_ir_json(irJsonString, 42);
```

### run_ir_simple(ir_json: string, seed: number): number

IR JSON を受け取り、数値結果のみを返す（エラー時は NaN）。

```javascript
import { run_ir_simple } from './pkg/subset_julia_vm_web.js';
const value = run_ir_simple(irJsonString, 42);
```

### get_version(): string

```javascript
import { get_version } from './pkg/subset_julia_vm_web.js';
console.log(get_version());
```

### get_supported_features(): string[]

```javascript
import { get_supported_features } from './pkg/subset_julia_vm_web.js';
console.log(get_supported_features());
```

### get_unsupported_features(): string[]

```javascript
import { get_unsupported_features } from './pkg/subset_julia_vm_web.js';
console.log(get_unsupported_features());
```

## 設計上の注意

- **Pure Rust パーサー** (`subset_julia_vm_parser`) を使用することで、WASM でも Native と完全に同一のパイプラインを実現
- Base ライブラリは起動時に Pure Rust パーサーでパースされる（`base_loader.rs` 参照）
- C 依存性なし、追加のツールチェーン不要

### Feature フラグ

WASM ビルドでは `parser-rust` 機能が有効化されます：

```toml
[dependencies.subset_julia_vm]
path = "../subset_julia_vm"
default-features = false
features = ["parser-rust", "wasm"]
```

これにより：
- Pure Rust パーサーが使用される
- tree-sitter 依存なし
- Native/iOS と同一の Lowering コードパス

## 参考

- `subset_julia_vm_web/README.md`
- `docs/web/WEB_ARCHITECTURE.md`
