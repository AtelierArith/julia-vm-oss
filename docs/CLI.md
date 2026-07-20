# SubsetJuliaVM CLI / API / 開発者リファレンス

エンドユーザー向けの簡易ガイドは [`README.md`](../README.md) を参照してください。

---

## 目次

- [sjulia のビルド](#sjulia-のビルド)
- [sjulia cache（推奨）](#sjulia-cache推奨)
- [CLI リファレンス](#cli-リファレンス)
- [Rust API](#rust-api)
- [C ABI / iOS](#c-abi--ios)
- [WebAssembly](#webassembly)
- [Tests](#tests)
- [Audits](#audits)

---

## sjulia のビルド

CLI binary `sjulia` は `subset_julia_vm` crate の `repl` feature を有効にしてビルドします。

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
```

`cargo run` から直接実行する場合:

```bash
cargo run -p subset_julia_vm --bin sjulia --features repl -- path/to/file.jl
cargo run -p subset_julia_vm --bin sjulia --features repl -- -e 'println(1 + 2)'
```

---

## sjulia cache（必須）

`sjulia` は冷起動を実用的な速度にするため、以下の 2 種類の cache をバイナリに埋め込みます。

- **prelude Program cache**: parse / lower 済みの prelude を保存したもの
- **Base bytecode cache**: Base 関数を compile 済みの bytecode を保存したもの

いずれもユーザープログラムの bytecode は含みません。cache なしでも動作しますが、冷起動が非常に遅いため、実用には cache 埋め込みを必ず行ってください。

### cache の生成

```bash
mkdir -p target
./target/release/sjulia --precompile-prelude target/prelude_program_cache.bin
./target/release/sjulia --precompile-base target/base_cache.bin
```

### cache の埋め込み

生成した cache ファイルの絶対パスを環境変数で渡し、再度 build します。

```bash
SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
```

2 回目の build 後の `target/release/sjulia` が cache 埋め込み済みバイナリになります。

### 関連ファイル

- `subset_julia_vm/src/bin/sjulia.rs`
- `subset_julia_vm/build.rs`
- `subset_julia_vm_compile/src/compile/precompile.rs`
- `subset_julia_vm_compile/src/compile/cache.rs`

---

## CLI リファレンス

`sjulia --help` の主要な形式:

```text
sjulia                              Start interactive REPL (or execute piped stdin)
sjulia -                            Read and execute script from stdin
sjulia <file.jl>                    Execute Julia file
sjulia <file.sjir>                  Execute Core IR file
sjulia <file.sjvmbc>                Execute VM bytecode file
sjulia -e <code>                    Execute code string
sjulia --compile <file.jl> -o <out> Compile to Core IR file (.sjir)
sjulia --run-ir <file.sjir>         Execute Core IR file
sjulia --compile-vm <file.jl> -o <out> Compile to VM bytecode file (.sjvmbc)
sjulia --run-vm-bytecode <file.sjvmbc> Execute VM bytecode file
sjulia --type-stability <file.jl>   Analyze type stability
sjulia --dump-bytecode <file.jl>    Dump compiled VM bytecode
sjulia --dump-ast <file.jl>         Dump AST structure for debugging
sjulia --precompile-prelude <out.bin> Generate prelude Program cache for embedding
sjulia --precompile-base <out.bin>  Generate Base bytecode cache for embedding
```

主要なオプション:

- `-e <code>`: コード文字列を実行
- `-c, --compile <file>`: Core IR にコンパイル
- `--run-ir`: Core IR ファイルを実行
- `--compile-vm`: VM bytecode にコンパイル
- `--run-vm-bytecode`: VM bytecode ファイルを実行
- `-o, --output <file>`: `--compile` / `--compile-vm` の出力ファイル
- `-t, --type-stability`: 型安定性解析
- `--strict`: 型不安定関数がある場合に exit code 1
- `--json`: `--type-stability` / `--dump-ast` の JSON 出力
- `--dump-bytecode`: VM bytecode をダンプ
- `--all`: `--dump-bytecode` に Base/prelude 関数も含める
- `--precompile-base`: Base bytecode cache を生成
- `--precompile-prelude`: prelude Program cache を生成
- `-h, --help`: ヘルプ表示

VM profiler を有効にする場合は `SJULIA_VM_PROFILE=1` を付けます（`repl` feature build で有効）。

---

## Rust API

Programmatic API は `subset_julia_vm/src/api.rs` にあります。一部は crate root から re-export されています。

代表的な関数:

- `compile_and_run_value(src, seed)`
- `compile_and_run_auto_str(src, seed)`
- `compile_and_run_str(src, seed)`
- `compile_to_ir_str(src)`
- `run_ir_json_str(json, n, seed)`
- `analyze_type_stability(src)`
- `analyze_type_stability_json(src)`

---

## C ABI / iOS

C ABI は `subset_julia_vm_ffi/src/` で定義されています。header は `subset_julia_vm_ffi/include/subset_vm.h` にあります。

`subset_julia_vm` crate 自体は `rlib` のみで、C ABI 用の `subset_julia_vm_ffi` crate が `staticlib`/`cdylib` としてビルドされます。`[lib] name = "subset_julia_vm"` のままなので、生成物名は `libsubset_julia_vm.*` です。

iOS app 側では `SubsetJuliaVMApp/SubsetJuliaVMApp/Services/FFI/` の Swift ファイルが `@_silgen_name` で Rust symbol を参照しています。

Swift app から使われている主な symbol:

- `compile_and_run_detailed`
- `compile_and_run_streaming`
- `free_execution_result`
- `vm_request_cancel`
- `vm_reset_cancel`
- `repl_session_new`
- `repl_session_eval`
- `repl_session_reset`
- `repl_session_free`
- `free_repl_result`
- `is_expression_complete`
- `split_expressions`
- `unicode_lookup`
- `unicode_completions`
- `unicode_expand`
- `unicode_reverse_lookup`
- `free_string`

### iOS build

```bash
rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim

cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios
cargo build --release -p subset_julia_vm_ffi --target aarch64-apple-ios-sim
```

### SwiftUI app build

```bash
xcodebuild \
  -project SubsetJuliaVMApp/SubsetJuliaVMApp.xcodeproj \
  -scheme SubsetJuliaVMApp \
  -sdk iphonesimulator \
  -destination 'platform=iOS Simulator,name=iPad (A16)' \
  build
```

---

## WebAssembly

WebAssembly binding は `subset_julia_vm_web/` にあります。`wasm-bindgen` entry point は `subset_julia_vm_web/src/lib.rs` です。

公開されている主な binding:

- `run_from_source`
- `run_from_source_typed`
- `run_ir_json`
- `run_ir_simple`
- `get_version`
- `get_supported_features`
- `get_unsupported_features`
- Unicode completion helpers

### Build

```bash
cd subset_julia_vm_web
wasm-pack build --target web --profile web-release
```

Browser playground は `web/` にあります。

---

## Tests

このリポジトリでは `cargo nextest` を使います。長いテストは timeout 付きで実行してください。

```bash
timeout 1800 cargo nextest run --release -p subset_julia_vm
timeout 1800 cargo nextest run --release -p subset_julia_vm --test fixture_tests
timeout 1800 cargo nextest run -p subset_julia_vm --test fixture_tests <category>::
timeout 1800 cargo nextest run -p subset_julia_vm --lib
```

fixture test は `subset_julia_vm/tests/fixtures/` にあります。各 category の `manifest.toml` と root manifest を `subset_julia_vm/build.rs` が読み、chunked Rust tests を生成します。

category 一覧:

```bash
cargo nextest list -p subset_julia_vm --test fixture_tests 2>/dev/null \
  | awk '{print $2}' | awk -F'::' '{print $1}' | sort -u
```

変更した fixture に対する fast feedback command:

```bash
scripts/fixture_fast_feedback.sh subset_julia_vm/tests/fixtures/<category>/<file>.jl
```

---

## Audits

代表的な audit / consistency scripts:

```bash
bash scripts/check_docs_vm_refs.sh
bash scripts/check_fixture_test_names.sh
bash scripts/check_base_routing_registry.sh
bash scripts/check_value_array_allowlist.sh
bash scripts/check_workarounds_documented.sh
bash scripts/check_workarounds_sync.sh
```

Clippy policy:

```bash
cargo clippy -p subset_julia_vm --all-targets -- -D warnings
```
