# SubsetJuliaVM

SubsetJuliaVM は、Julia の厳格な部分集合を Rust で実行する実装です。

既定の実行経路は、Julia ソースを pure Rust parser で解析し、lowering、Core IR、bytecode compiler を経て stack VM で解釈します。

C ABI 経由の iOS ホストと wasm-bindgen 経由の WebAssembly ホストは、この既定のバイトコード VM を使います。

この経路は実行時のネイティブコード生成に依存しません。

AoT と Cranelift は opt-in の別経路です。

AoT は Rust ソースを生成し、cranelift feature は object 出力とデスクトップ向け JIT API を追加します。

これらの feature は、既定の iOS または WebAssembly 実行経路には含まれません。

## Linux のビルド依存関係

Linux ではリンカーとして `clang` と LLVM の `lld` を使います。

Raspberry Pi OS、Debian、Ubuntu では、ビルド前に両方をインストールしてください。

```bash
sudo apt update
sudo apt install clang lld
```

`clang: error: invalid linker name in argument '-fuse-ld=lld'` と表示された場合は、`lld` がインストールされているか確認します。

```bash
clang --version
ld.lld --version
```

## Quick Start

`sjulia` を使うには、cache をバイナリに埋め込む必要があります。cache なしでも動作しますが、冷起動が非常に遅いため、実用には必須です。

インストールスクリプトは cache の生成と埋め込みを自動化します。

```bash
# macOS / Linux
./scripts/sjulia_install.sh
```

```powershell
# Windows
pwsh -File scripts/sjulia_install.ps1
```

cache を強制的に再生成する場合は、どちらのスクリプトにも `--force-cache` を渡します。

手動でビルドする場合は、以下を実行します。

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

- **既定パイプライン**: Julia source → parser → lowering → Core IR → bytecode compiler → stack VM
- **iOS と WebAssembly**: 同じバイトコード VM を C ABI または wasm-bindgen から呼び出す
- **opt-in 経路**: AoT feature は Rust を生成し、cranelift feature は object 出力とデスクトップ JIT API を追加する
- **Julia 互換を目指す**: 対応範囲は `subset_julia_vm/tests/fixtures/`、`subset_julia_vm/src/julia/`、`docs/vm/STATUS.md`、`docs/vm/UNIMPLEMENTED.md` を参照

Workspace は以下の crate で構成されています。

```text
subset_julia_vm/          Core VM, compiler, lowering, CLI
subset_julia_vm_bytecode/ Shared Instr, Value, CompiledProgram, wire IDs
subset_julia_vm_types/    Julia type system and inference primitives
subset_julia_vm_ir/       Shared source spans and errors
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
