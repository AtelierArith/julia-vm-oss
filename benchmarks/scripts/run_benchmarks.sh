#!/bin/bash
# AoT Benchmark Runner
# Compares performance between Julia, SubsetJuliaVM interpreter, and Rust AoT
# Includes compile time and binary size measurements

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
BENCH_DIR="$ROOT_DIR/benchmarks"
RESULTS_DIR="$BENCH_DIR/results"
JULIA_DIR="$BENCH_DIR/julia"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m' # No Color

# Benchmark configurations
BENCHMARKS=("fib" "calc_pi" "array_sum" "matmul" "mandelbrot")

# Check if Julia is available
JULIA_AVAILABLE=false
if command -v julia &> /dev/null; then
    JULIA_AVAILABLE=true
    JULIA_VERSION=$(julia --version 2>/dev/null | head -1)
    echo -e "${GREEN}Julia found: $JULIA_VERSION${NC}"
else
    echo -e "${YELLOW}Julia not found, skipping Julia benchmarks${NC}"
fi

# Build the project first
echo -e "${BLUE}=== Building SubsetJuliaVM ===${NC}"
cd "$ROOT_DIR"
cargo build --release --features repl 2>/dev/null || {
    echo -e "${RED}Failed to build with repl feature${NC}"
    exit 1
}

# Check if aot feature is available
if cargo build --release --features aot 2>/dev/null; then
    AOT_AVAILABLE=true
    echo -e "${GREEN}AoT compiler available${NC}"
else
    AOT_AVAILABLE=false
    echo -e "${YELLOW}AoT compiler not available, skipping AoT benchmarks${NC}"
fi

# Create results directory with timestamp
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RUN_DIR="$RESULTS_DIR/$TIMESTAMP"
mkdir -p "$RUN_DIR"

echo -e "${BLUE}=== Running Benchmarks ===${NC}"
echo "Results will be saved to: $RUN_DIR"
echo ""

# Function to run Julia benchmark (official Julia)
run_julia() {
    local name=$1
    local julia_file="$JULIA_DIR/${name}.jl"

    if [[ "$JULIA_AVAILABLE" != "true" ]]; then
        echo "skipped" > "$RUN_DIR/${name}_julia.txt"
        return 0
    fi

    if [[ ! -f "$julia_file" ]]; then
        echo -e "${RED}Benchmark file not found: $julia_file${NC}"
        echo "not_found" > "$RUN_DIR/${name}_julia.txt"
        return 1
    fi

    echo -e "${MAGENTA}Running Julia: $name${NC}"

    # Warmup run (includes JIT compilation)
    timeout 120 julia "$julia_file" > /dev/null 2>&1 || true

    # Timed runs
    local total_time=0
    local runs=3

    for i in $(seq 1 $runs); do
        local start=$(date +%s%N)
        timeout 120 julia "$julia_file" > /dev/null 2>&1 || {
            echo -e "${RED}Julia timeout or error for $name${NC}"
            echo "timeout" > "$RUN_DIR/${name}_julia.txt"
            return 1
        }
        local end=$(date +%s%N)
        local duration=$(( (end - start) / 1000000 ))
        total_time=$((total_time + duration))
    done

    local avg_time=$((total_time / runs))
    echo "$avg_time" > "$RUN_DIR/${name}_julia.txt"
    echo -e "  Average time: ${GREEN}${avg_time}ms${NC}"
}

# Function to run interpreter benchmark (sjulia)
run_interpreter() {
    local name=$1
    local julia_file="$JULIA_DIR/${name}.jl"

    if [[ ! -f "$julia_file" ]]; then
        echo -e "${RED}Benchmark file not found: $julia_file${NC}"
        return 1
    fi

    echo -e "${YELLOW}Running sjulia (interpreter): $name${NC}"

    # Warmup run
    timeout 60 "$ROOT_DIR/target/release/sjulia" "$julia_file" > /dev/null 2>&1 || true

    # Timed runs
    local total_time=0
    local runs=3

    for i in $(seq 1 $runs); do
        local start=$(date +%s%N)
        timeout 60 "$ROOT_DIR/target/release/sjulia" "$julia_file" > /dev/null 2>&1 || {
            echo -e "${RED}Interpreter timeout or error for $name${NC}"
            echo "timeout" > "$RUN_DIR/${name}_interpreter.txt"
            return 1
        }
        local end=$(date +%s%N)
        local duration=$(( (end - start) / 1000000 ))
        total_time=$((total_time + duration))
    done

    local avg_time=$((total_time / runs))
    echo "$avg_time" > "$RUN_DIR/${name}_interpreter.txt"
    echo -e "  Average time: ${GREEN}${avg_time}ms${NC}"
}

# Function to run AoT Rust benchmark
run_aot_rust() {
    local name=$1
    local julia_file="$JULIA_DIR/${name}.jl"
    local rust_file="$RUN_DIR/${name}.rs"
    local binary="$RUN_DIR/${name}_aot"

    if [[ "$AOT_AVAILABLE" != "true" ]]; then
        echo "skipped" > "$RUN_DIR/${name}_aot_rust.txt"
        echo "skipped" > "$RUN_DIR/${name}_compile_time.txt"
        echo "skipped" > "$RUN_DIR/${name}_binary_size.txt"
        return 0
    fi

    echo -e "${CYAN}Running AoT (Rust): $name${NC}"

    # Measure AoT compilation time (Julia -> Rust)
    # Use --minimal-prelude for cleaner code generation (no unused structs)
    local aot_start=$(date +%s%N)
    if ! "$ROOT_DIR/target/release/aot" "$julia_file" -o "$rust_file" --minimal-prelude 2>/dev/null; then
        echo -e "${RED}AoT compilation failed for $name${NC}"
        echo "compile_error" > "$RUN_DIR/${name}_aot_rust.txt"
        echo "compile_error" > "$RUN_DIR/${name}_compile_time.txt"
        return 1
    fi
    local aot_end=$(date +%s%N)
    local aot_time=$(( (aot_end - aot_start) / 1000000 ))

    # Measure rustc compilation time
    local rustc_start=$(date +%s%N)
    if ! rustc -O "$rust_file" -o "$binary" \
        --extern subset_julia_vm_runtime="$ROOT_DIR/target/release/libsubset_julia_vm_runtime.rlib" \
        -L "$ROOT_DIR/target/release/deps" 2>/dev/null; then
        echo -e "${RED}Rust compilation failed for $name${NC}"
        echo "rustc_error" > "$RUN_DIR/${name}_aot_rust.txt"
        echo "rustc_error" > "$RUN_DIR/${name}_compile_time.txt"
        return 1
    fi
    local rustc_end=$(date +%s%N)
    local rustc_time=$(( (rustc_end - rustc_start) / 1000000 ))
    local total_compile_time=$((aot_time + rustc_time))

    echo "$total_compile_time" > "$RUN_DIR/${name}_compile_time.txt"
    echo "$aot_time" > "$RUN_DIR/${name}_aot_gen_time.txt"
    echo "$rustc_time" > "$RUN_DIR/${name}_rustc_time.txt"
    echo -e "  Compile time: ${CYAN}${total_compile_time}ms${NC} (aot: ${aot_time}ms, rustc: ${rustc_time}ms)"

    # Get binary size
    local binary_size=$(stat -f%z "$binary" 2>/dev/null || stat --printf="%s" "$binary" 2>/dev/null || echo "N/A")
    echo "$binary_size" > "$RUN_DIR/${name}_binary_size.txt"
    if [[ "$binary_size" != "N/A" ]]; then
        local human_size=$(numfmt --to=iec $binary_size 2>/dev/null || echo "${binary_size}B")
        echo -e "  Binary size: ${CYAN}${human_size}${NC}"
    fi

    # Get source code size
    local source_size=$(stat -f%z "$rust_file" 2>/dev/null || stat --printf="%s" "$rust_file" 2>/dev/null || echo "N/A")
    echo "$source_size" > "$RUN_DIR/${name}_source_size.txt"

    # Timed runs
    local total_time=0
    local runs=3

    for i in $(seq 1 $runs); do
        local start=$(date +%s%N)
        timeout 60 "$binary" > /dev/null 2>&1 || {
            echo -e "${RED}AoT execution timeout or error for $name${NC}"
            echo "runtime_error" > "$RUN_DIR/${name}_aot_rust.txt"
            return 1
        }
        local end=$(date +%s%N)
        local duration=$(( (end - start) / 1000000 ))
        total_time=$((total_time + duration))
    done

    local avg_time=$((total_time / runs))
    echo "$avg_time" > "$RUN_DIR/${name}_aot_rust.txt"
    echo -e "  Average time: ${GREEN}${avg_time}ms${NC}"
}

# Generate summary report
generate_report() {
    local report_file="$RUN_DIR/report.md"

    echo "# AoT Benchmark Report" > "$report_file"
    echo "" >> "$report_file"
    echo "**Date**: $(date)" >> "$report_file"
    echo "**Platform**: $(uname -s) $(uname -m)" >> "$report_file"
    echo "**CPU**: $(sysctl -n machdep.cpu.brand_string 2>/dev/null || lscpu | grep 'Model name' | cut -d':' -f2 | xargs 2>/dev/null || echo 'Unknown')" >> "$report_file"
    if [[ "$JULIA_AVAILABLE" == "true" ]]; then
        echo "**Julia**: $JULIA_VERSION" >> "$report_file"
    fi
    echo "" >> "$report_file"

    # Execution time comparison
    echo "## Execution Time Comparison" >> "$report_file"
    echo "" >> "$report_file"
    echo "| Benchmark | Julia (ms) | sjulia (ms) | AoT Rust (ms) | AoT vs Julia | AoT vs sjulia |" >> "$report_file"
    echo "|-----------|------------|-------------|---------------|--------------|---------------|" >> "$report_file"

    for bench in "${BENCHMARKS[@]}"; do
        local julia_file="$RUN_DIR/${bench}_julia.txt"
        local interp_file="$RUN_DIR/${bench}_interpreter.txt"
        local aot_file="$RUN_DIR/${bench}_aot_rust.txt"

        local julia_time="N/A"
        local interp_time="N/A"
        local aot_time="N/A"
        local speedup_julia="N/A"
        local speedup_interp="N/A"

        if [[ -f "$julia_file" ]]; then
            julia_time=$(cat "$julia_file")
        fi

        if [[ -f "$interp_file" ]]; then
            interp_time=$(cat "$interp_file")
        fi

        if [[ -f "$aot_file" ]]; then
            aot_time=$(cat "$aot_file")
        fi

        # Calculate speedup vs Julia
        if [[ "$julia_time" =~ ^[0-9]+$ ]] && [[ "$aot_time" =~ ^[0-9]+$ ]] && [[ "$aot_time" -gt 0 ]]; then
            speedup_julia=$(echo "scale=2; $julia_time / $aot_time" | bc)
            speedup_julia="${speedup_julia}x"
        fi

        # Calculate speedup vs sjulia
        if [[ "$interp_time" =~ ^[0-9]+$ ]] && [[ "$aot_time" =~ ^[0-9]+$ ]] && [[ "$aot_time" -gt 0 ]]; then
            speedup_interp=$(echo "scale=2; $interp_time / $aot_time" | bc)
            speedup_interp="${speedup_interp}x"
        fi

        echo "| $bench | $julia_time | $interp_time | $aot_time | $speedup_julia | $speedup_interp |" >> "$report_file"
    done

    # Compile time breakdown
    echo "" >> "$report_file"
    echo "## Compilation Time Breakdown" >> "$report_file"
    echo "" >> "$report_file"
    echo "| Benchmark | AoT Gen (ms) | rustc -O (ms) | Total (ms) |" >> "$report_file"
    echo "|-----------|--------------|---------------|------------|" >> "$report_file"

    for bench in "${BENCHMARKS[@]}"; do
        local aot_gen_file="$RUN_DIR/${bench}_aot_gen_time.txt"
        local rustc_file="$RUN_DIR/${bench}_rustc_time.txt"
        local total_file="$RUN_DIR/${bench}_compile_time.txt"

        local aot_gen="N/A"
        local rustc_time="N/A"
        local total="N/A"

        if [[ -f "$aot_gen_file" ]]; then
            aot_gen=$(cat "$aot_gen_file")
        fi
        if [[ -f "$rustc_file" ]]; then
            rustc_time=$(cat "$rustc_file")
        fi
        if [[ -f "$total_file" ]]; then
            total=$(cat "$total_file")
        fi

        echo "| $bench | $aot_gen | $rustc_time | $total |" >> "$report_file"
    done

    # Binary/source size
    echo "" >> "$report_file"
    echo "## Generated Code Size" >> "$report_file"
    echo "" >> "$report_file"
    echo "| Benchmark | Rust Source (bytes) | Binary (bytes) |" >> "$report_file"
    echo "|-----------|---------------------|----------------|" >> "$report_file"

    for bench in "${BENCHMARKS[@]}"; do
        local source_file="$RUN_DIR/${bench}_source_size.txt"
        local binary_file="$RUN_DIR/${bench}_binary_size.txt"

        local source_size="N/A"
        local binary_size="N/A"

        if [[ -f "$source_file" ]]; then
            source_size=$(cat "$source_file")
        fi
        if [[ -f "$binary_file" ]]; then
            binary_size=$(cat "$binary_file")
        fi

        echo "| $bench | $source_size | $binary_size |" >> "$report_file"
    done

    # Notes
    echo "" >> "$report_file"
    echo "## Notes" >> "$report_file"
    echo "" >> "$report_file"
    echo "- Execution times are averages of 3 runs (after warmup)" >> "$report_file"
    echo "- **Julia**: Official Julia interpreter with JIT compilation" >> "$report_file"
    echo "- **sjulia**: SubsetJuliaVM bytecode interpreter (no JIT)" >> "$report_file"
    echo "- **AoT Rust**: Julia → Rust → rustc -O (ahead-of-time compilation)" >> "$report_file"
    echo "- Compile time includes both AoT generation and rustc compilation" >> "$report_file"
    echo "" >> "$report_file"

    # Backend comparison summary
    echo "## Backend Comparison Summary" >> "$report_file"
    echo "" >> "$report_file"
    echo "| Feature | Julia JIT | sjulia (Interpreter) | AoT Rust |" >> "$report_file"
    echo "|---------|-----------|---------------------|----------|" >> "$report_file"
    echo "| Startup Time | Slow (JIT warmup) | Fast | Fast |" >> "$report_file"
    echo "| Execution Speed | Fast (after JIT) | Slow | Fastest |" >> "$report_file"
    echo "| Memory Usage | High | Medium | Low |" >> "$report_file"
    echo "| iOS Compatible | No | Yes | Yes |" >> "$report_file"
    echo "| External Dependencies | Julia runtime | None | rustc (build only) |" >> "$report_file"

    echo -e "${GREEN}Report generated: $report_file${NC}"
}

# Run all benchmarks
for bench in "${BENCHMARKS[@]}"; do
    echo ""
    echo -e "${BLUE}=== Benchmark: $bench ===${NC}"
    run_julia "$bench" || true
    run_interpreter "$bench" || true
    run_aot_rust "$bench" || true
done

echo ""
echo -e "${BLUE}=== Generating Report ===${NC}"
generate_report

echo ""
echo -e "${GREEN}=== Benchmark Complete ===${NC}"
echo "Results saved to: $RUN_DIR"
cat "$RUN_DIR/report.md"
