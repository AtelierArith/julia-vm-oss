# Description

This directory compiles `subset_julia_vm` to WebAssembly (WASM), enabling Julia to be used from web applications.

## Build

### Prerequisites

```bash
# Add WASM target
rustup target add wasm32-unknown-unknown

# Install wasm-pack
cargo install wasm-pack
```

### WASM Build

```bash
# Run inside the subset_julia_vm_web directory
cd path/to/this-directory
wasm-pack build --target web --out-dir ../web/pkg
```

## Local Development

### Quick Start

```bash
# Run inside the subset_julia_vm_web directory
wasm-pack build --target web --out-dir ../web/pkg && \
python3 -m http.server 8080 --directory ../web
```

Open http://localhost:8080 in your browser.
