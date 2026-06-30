# SubsetJuliaVM

SubsetJuliaVM は、Julia の静的サブセットを JIT なしで実行する Rust 実装です。Julia ソースを pure Rust parser で解析し、lowering → Core IR → bytecode compiler → stack VM というパイプラインで実行します。

iOS 向けの C ABI、Web 向けの WebAssembly binding も提供しています。

## Quick Start

`sjulia` を使うには、cache をバイナリに埋め込む必要があります。cache なしでも動作しますが、冷起動が非常に遅いため、実用には必須です。

```bash
cargo build --release -p subset_julia_vm --bin sjulia --features repl
mkdir -p target
./target/release/sjulia --precompile-prelude target/prelude_program_cache.bin
./target/release/sjulia --precompile-base target/base_cache.bin

SJULIA_PRELUDE_PROGRAM_CACHE="$(pwd)/target/prelude_program_cache.bin" \
SJULIA_BASE_CACHE="$(pwd)/target/base_cache.bin" \
  cargo build --release -p subset_julia_vm --bin sjulia --features repl
```

2 回目の build 後の `target/release/sjulia` が cache 埋め込み済みバイナリです。

```bash
./target/release/sjulia path/to/file.jl
./target/release/sjulia -e 'println(1 + 2)'
```

## WebAssembly package のビルド

Web Playground 用の WASM package は以下で build します。

```bash
scripts/wasm_build_with_cache.sh --target web --out-dir ./web/pkg
```

このスクリプトは prelude/Base cache を生成・埋め込み、`subset_julia_vm_web` を `wasm-pack` で build します。詳細は [`docs/CLI.md`](docs/CLI.md) を参照。

## SubsetJuliaVM とは

- **静的パイプライン**: Julia source → parser → lowering → Core IR → bytecode compiler → stack VM
- **JIT なし**: iOS など JIT が使えない環境でも動作
- **Julia 互換を目指す**: 対応範囲は `subset_julia_vm/tests/fixtures/`、`subset_julia_vm/src/julia/`、`docs/vm/STATUS.md`、`docs/vm/UNIMPLEMENTED.md` を参照

Workspace は以下の crate で構成されています。

```text
subset_julia_vm/          Core VM, compiler, lowering, CLI
subset_julia_vm_ffi/      C ABI staticlib/cdylib (iOS / native)
subset_julia_vm_parser/   pure Rust Julia parser
subset_julia_vm_web/      wasm-bindgen bindings
subset_julia_vm_runtime/  AoT compiled code 用 runtime crate
```

## ドキュメント

- [`docs/CLI.md`](docs/CLI.md): 詳細な CLI リファレンス、cache 手順、Rust API、C ABI / iOS、WebAssembly、Tests、Audits
- [`docs/vm/`](docs/vm/): アーキテクチャ、実装状況、テスト・監査ポリシーなど

主要なドキュメント:

- `docs/vm/ARCHITECTURE_OVERVIEW.md`
- `docs/vm/CHECKLISTS.md`
- `docs/vm/TESTING_GUIDE.md`
- `docs/vm/STATUS.md`
- `docs/vm/DONE.md`
- `docs/vm/UNIMPLEMENTED.md`
- `docs/vm/PURE_JULIA_DESIGN.md`
- `docs/vm/WORKAROUNDS.md`
- `docs/vm/CODE_AUDITS.md`
