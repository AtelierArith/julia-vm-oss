# subset_julia_vm_web

`subset_julia_vm_web` は SubsetJuliaVM の `wasm-bindgen` ラッパーです。ブラウザ JavaScript から、パーサー、lowering、コンパイラ、VM 実行、plot artifact 取得、Unicode 補助 API を呼び出せるようにします。

ブラウザアプリ本体は `../web` にあります。

## 現在のパイプライン

推奨 API は `run_from_source` です。

```text
Julia source
  -> subset_julia_vm_parser の Pure Rust parser
  -> lowering / package loading
  -> compile_with_cache
  -> VM
  -> ExecutionResult
```

現在の Playground には web-tree-sitter 経路はありません。WASM ビルドも native 実行と同じ Pure Rust parser 経路を使います。`subset_julia_vm` は `default-features = false`、`features = ["wasm"]` でビルドします。

## ビルド

Playground 用の WASM package を作る通常手順はこれだけです。この README は
`subset_julia_vm_web/` にあるため、まずリポジトリルートへ戻って helper を実行します。

```bash
cd ../
scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg
```

出力先は `web/pkg` です。すでにリポジトリルートにいる場合は、`cd ../` は不要です。

この helper は host 用 `sjulia` をビルドし、Base bytecode cache と parsed/lowered
prelude Program cache を作成または再利用してから、cache を埋め込んだ WASM artifact
を `wasm-pack` で生成します。`wasm-pack` が未インストールの場合は
`cargo install wasm-pack` が必要です。

注意: 埋め込まれるのは Base bytecode cache と prelude Program cache です。
`run_from_source` の初回には user source の parser/lowering、embedded Base cache
deserialize/restore、user program compile がまだ残ります。Playground では Run button
を有効化する前の startup warmup でこの cold path を先に通します (Issue #6127)。

## ローカル確認

ビルド後:

```bash
cd web
python3 server.py
```

`http://localhost:8080` を開きます。

Rust 側の web binding test:

```bash
timeout 1800 cargo nextest run --release -p subset_julia_vm_web
```

## JavaScript API

### `run_from_source(source, seed)`

Julia source をフルパイプラインで実行します。Playground はこの API を使います。
`typed_value` には戻り値の型タグ付き JSON object が入ります。

```javascript
import init, { run_from_source, run_from_source_typed } from './pkg/subset_julia_vm_web.js';

await init();
const result = run_from_source('using Plots\nplot(sin)\n', BigInt(42));
const typed = run_from_source_typed('[1.0, 2.5, 3.0]', BigInt(42));
console.log(typed.typed_value.type); // "array"
```

`run_from_source_typed` は `typed_value` を明示的に使う呼び出し側向けの
alias です。戻り値の shape は `run_from_source` と同じです。

### `run_ir_json(irJson, seed)`

serialized Core IR JSON を実行します。古い sample/test 経路との互換用です。この入口では plot artifact を抽出しません。Playground と同じ挙動が必要なら `run_from_source` を使います。

### `run_ir_simple(irJson, seed)`

serialized Core IR JSON を実行して、数値だけを `number` で返します。エラー時は `NaN` です。

### metadata / Unicode helper

```javascript
get_version();
get_supported_features();
get_unsupported_features();

unicode_lookup('\\alpha');
unicode_reverse_lookup('α');
unicode_completions('\\alp');
unicode_expand('f(\\alpha, \\beta)');
```

## ExecutionResult

`run_from_source` と `run_ir_json` は Rust から serialize された JS object を返します。

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

plot の場合、`run_from_source` は次を返します。

```text
artifact_mime = "application/vnd.plotly+json"
artifact_data = { traces, layout } を持つ JSON string
```

この artifact を Plotly.js で描画する責任は host page 側にあります。

`typed_value` の代表例:

```json
{"type":"array","element_type":"Float64","shape":[3],"elements":[...]}
{"type":"complex","real":1.5,"imag":2.25}
```

## 実装メモ

- `src/lib.rs` が web app 向けの唯一の wasm-bindgen surface です。
- `run_from_source` は `compile_with_cache` を使います。繰り返し実行や Playground の startup warmup で、compiled Base / program state を再利用できます。`scripts/wasm_build_with_cache.sh` で作った artifact は prelude Program も埋め込むため、WASM 初回実行で prelude source を parse/lower しません。
- `getrandom = { features = ["js"] }` は意図的な直接依存です。間接的な RNG 利用が WASM 上で browser の `crypto.getRandomValues()` を使うために必要です。
- `../web/pkg` は `wasm-pack` の生成物です。手編集せず、再ビルドしてください。
