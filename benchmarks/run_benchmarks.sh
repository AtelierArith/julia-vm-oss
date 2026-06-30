#!/bin/bash
# Benchmark comparison script: Julia vs sjulia vs AOT
# Run from the repository root directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

echo "=============================================="
echo "SubsetJuliaVM Benchmark Suite"
echo "=============================================="
echo ""
echo "Building release binaries..."
cargo build --bin sjulia --features repl --release 2>/dev/null
cargo build --bin aot --features aot --release 2>/dev/null

echo "Build complete."
echo ""

# Create temp directory for AOT
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT

echo "=============================================="
echo "Benchmark 1: Pi Estimation (calc_pi)"
echo "=============================================="
echo ""

echo "--- Julia (official) ---"
julia benchmarks/calc_pi_benchmark.jl
echo ""

echo "--- sjulia (SubsetJuliaVM interpreter) ---"
./target/release/sjulia benchmarks/calc_pi_benchmark.jl
echo ""

echo "--- AOT (Ahead-of-Time compiled to native Rust) ---"
# Generate AOT code
./target/release/aot benchmarks/calc_pi_aot.jl -o "$TEMP_DIR/calc_pi_aot.rs" 2>/dev/null

# Create a main.rs that includes the generated code and measures time
cat > "$TEMP_DIR/main.rs" << 'EOF'
mod calc_pi_aot;
use std::time::Instant;

fn main() {
    // Warmup
    let _ = calc_pi_aot::main_result();

    // Benchmark N=100
    let start = Instant::now();
    let result = calc_pi_aot::main_result();
    let elapsed = start.elapsed();
    println!("N=100: π ≈ {:?}", result);
    println!("  {:.6} seconds (AOT native)", elapsed.as_secs_f64());
}
EOF

# Create Cargo.toml
cat > "$TEMP_DIR/Cargo.toml" << 'EOF'
[package]
name = "calc_pi_aot_bench"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "bench"
path = "main.rs"
EOF

# Compile and run
cd "$TEMP_DIR"
if cargo build --release 2>/dev/null; then
    ./target/release/bench
else
    echo "AOT compilation to native not fully supported for this code"
fi
cd - > /dev/null
echo ""

echo "=============================================="
echo "Benchmark 2: Mandelbrot Set"
echo "=============================================="
echo ""

echo "--- Julia (official) ---"
julia benchmarks/mandelbrot_benchmark.jl 2>&1 | head -5
echo ""

echo "--- sjulia (SubsetJuliaVM interpreter) ---"
./target/release/sjulia benchmarks/mandelbrot_benchmark.jl 2>&1 | head -5
echo ""

echo "=============================================="
echo "Benchmark Summary"
echo "=============================================="
echo ""
echo "Note: First run times include compilation overhead."
echo "For accurate benchmarks, use 'cargo bench' with Criterion."
