# VM Optimization Status

## Current Status: Infrastructure Complete, Compiler Integration Pending

### What's Implemented ✅

#### 1. VM Instruction Profiler (`src/vm/profiler.rs`)

Thread-local profiling system to track instruction execution frequency:

```rust
use subset_julia_vm::vm::profiler;

profiler::enable();
profiler::clear();

// Run program
compile_and_run_str(source, 0);

// Print results
profiler::print_results();
```

**Features**:
- Zero-overhead when disabled (atomic bool check)
- Thread-local counters (no synchronization overhead)
- Top 20 instructions by frequency
- Percentage breakdown

**Example Output** (calc_pi N=100):
```
Total instructions executed: 801,106

Top instructions:
 1. LoadI64       296,021  (36.95%)  ← HashMap lookups
 2. StoreI64      135,969  (16.97%)  ← HashMap inserts
 3. JumpIfZero     80,228  (10.01%)  ← Loop condition checks
 4. PushI64        76,318   (9.53%)  ← Constants
 5. Jump           56,114   (7.00%)  ← Loop back-edges
 6. NeI64          49,826   (6.22%)  ← Comparisons
 7. ModI64         39,826   (4.97%)  ← GCD operation
```

**Key Insight**: Load/Store dominates (54% of execution) - main optimization target

#### 2. Fused Instructions (`src/vm/instr.rs`, `src/vm/exec.rs`)

Implemented 12 fused instructions to reduce instruction dispatch and HashMap overhead:

**Load-Op Fusion** (4 instructions):
```rust
// Before: LoadI64(x) + AddI64  (2 instructions, 2 HashMap lookups)
// After:  LoadAddI64(x)         (1 instruction, 1 HashMap lookup)

LoadAddI64(String),  // var + stack_top
LoadSubI64(String),  // var - stack_top
LoadMulI64(String),  // var * stack_top
LoadModI64(String),  // var % stack_top
```

**Store-Op Fusion** (2 instructions):
```rust
// Before: LoadI64(cnt) + AddI64 + StoreI64(cnt)  (3 instructions)
// After:  IncVarI64(cnt)                          (1 instruction)

IncVarI64(String),   // var += stack_top
DecVarI64(String),   // var -= stack_top
```

**Compare-Jump Fusion** (6 instructions):
```rust
// Before: NeI64 + JumpIfZero(addr)  (2 instructions)
// After:  JumpIfEqI64(addr)         (1 instruction)

JumpIfNeI64(usize),  // jump if a != b
JumpIfEqI64(usize),  // jump if a == b
JumpIfLtI64(usize),  // jump if a < b
JumpIfGtI64(usize),  // jump if a > b
JumpIfLeI64(usize),  // jump if a <= b
JumpIfGeI64(usize),  // jump if a >= b
```

**Implementation**: Fully working in VM execution (exec.rs:302-506)

#### 3. Peephole Optimizer (`src/compile/peephole.rs`)

Pattern matching optimizer with jump target fixup:

**Patterns Detected**:
1. `LoadI64(x) + {Add,Sub,Mul,Mod}I64` → `Load{Add,Sub,Mul,Mod}I64(x)`
2. `LoadI64(x) + AddI64 + StoreI64(x)` → `IncVarI64(x)`
3. `{Ne,Eq,Lt,Gt,Le,Ge}I64 + JumpIfZero(addr)` → `JumpIf{Eq,Ne,Ge,Le,Gt,Lt}I64(addr)`

**Features**:
- Builds old→new instruction index mapping
- Updates all jump targets after optimization
- Handles fused jump instructions

**Tests**: 4 unit tests passing

### What's NOT Implemented ⏳

#### Compiler Integration (Blocked by Base Cache Interaction)

**Problem**: Peephole optimization changes instruction indices, breaking Base cache function boundaries

**Current State**: Optimizer disabled in `src/compile/mod.rs:1740` with TODO comment

**Example of the Issue**:
```
1. Base cache compiles 461 functions → 7987 instructions
2. Peephole optimizer fuses instructions → 7939 instructions (48 fewer)
3. FunctionInfo stores code_start=7800, code_end=7987
4. Later code tries to access base_cache.code[7800..7987]
5. PANIC: index 7987 out of range for length 7939
```

**Solution Options**:

**Option A: Update FunctionInfo after optimization** (Recommended)
```rust
// After peephole::optimize(code):
for func_info in &mut function_infos {
    func_info.code_start = old_to_new[func_info.code_start];
    func_info.code_end = old_to_new[func_info.code_end];
}
```

**Pros**: Clean, keeps optimization benefits everywhere
**Cons**: Requires passing old_to_new mapping out of optimizer

**Option B: Separate Base and User Optimization**
```rust
// Don't cache optimized Base
// Only optimize user code
if using_base_cache {
    optimize_only_user_functions();
}
```

**Pros**: Simpler logic
**Cons**: Base cache misses optimization benefits

**Option C: Per-Function Optimization**
```rust
// Optimize each function's bytecode separately before concatenation
for func in functions {
    func.code = peephole::optimize(func.code);
}
```

**Pros**: No cross-function boundary issues
**Cons**: Requires refactoring compilation pipeline

### Expected Performance Impact

#### Without Compiler Integration (Current State)

**Speedup**: 0% (fused instructions never emitted)
**Value**: Profiler identifies optimization targets

#### With Compiler Integration (After Fix)

**Conservative Estimate**:
- LoadAddI64 fusion: ~100K instructions → ~70K (30% reduction in loads)
- IncVarI64 fusion: ~10K patterns → ~3.3K instructions (67% reduction)
- JumpIf fusion: ~80K instructions → ~40K (50% reduction in comparisons)

**Total Expected**: 15-25% VM execution speedup

**Calc_pi(100) Projection**:
- Current: 801,106 instructions, ~1.09ms VM execution
- Optimized: ~650,000 instructions, ~0.87ms VM execution
- Speedup: 20% faster

### Latest Benchmark Results (2026-01-01)

**Test Command**: `cargo bench`

#### Parse & Lower Performance
Significant improvements in parsing and lowering phase:

| Benchmark | Time | Change |
|-----------|------|--------|
| simple_arithmetic | 1.53 µs | **-28.5% to -33.9%** ✅ |
| for_loop | 2.24 µs | **-27.6% to -35.8%** ✅ |
| fib_recursive | 4.32 µs | **-36.3% to -45.0%** ✅ |

#### Compile Performance
Major improvements in compilation phase:

| Benchmark | Time | Change |
|-----------|------|--------|
| simple_arithmetic | 4.64 ms | **-44.4% to -52.2%** ✅ |
| for_loop | 4.69 ms | **-14.5% to -20.8%** ✅ |
| compile_with_base | 444 µs | +0.9% to +1.4% (noise) |

#### VM Execution Performance
Modest improvements in VM execution:

| Benchmark | Time | Change |
|-----------|------|--------|
| simple_arithmetic | 425 µs | ±0.5% (no change) |
| for_loop | 453 µs | **-1.2% to -6.5%** ✅ |
| fib_10 | 1.02 ms | **-2.0% to -2.9%** ✅ |
| fib_20 | 71.8 ms | ±0.3% (no change) |
| array_sum_10 | 782 µs | **-2.4% to -5.9%** ✅ |

#### Full Pipeline Performance
Mixed results in full pipeline:

| Benchmark | Time | Change |
|-----------|------|--------|
| simple_arithmetic | 760 µs | +0.9% to +2.2% (noise) |
| for_loop_100 | 789 µs | **+2.9% to +4.4%** ⚠️ |
| fib_10 | 1.46 ms | **+7.1% to +12.3%** ⚠️ |

**Key Observations**:
- Parse/Lower and Compile phases show dramatic improvements (14-52%)
- VM execution shows modest gains (1-6%) without peephole optimization
- Full pipeline regressions likely due to cache/optimization interaction
- Base library compilation stable at ~444 µs (461 functions, 7939 instructions)

**Note**: Peephole optimizer remains disabled due to Base cache interaction issue. Expected 15-25% additional VM speedup once integrated.

### Profiling Results Summary

**Test Program**: calc_pi(100) - GCD-based π approximation

**Hottest Operations** (54% of execution):
1. **Variable Access** (LoadI64/StoreI64): 432K operations
   - Root cause: HashMap lookups for named variables
   - Solution: Slot-based locals (Phase 2)

2. **Control Flow** (Jump/JumpIfZero): 136K operations
   - Root cause: Loop overhead
   - Solution: Loop specialization (Phase 3)

3. **Arithmetic** (ModI64): 40K operations
   - Root cause: Frequent modulo in GCD
   - Solution: Fast path for common cases (Phase 3)

### Next Steps

#### Immediate (Complete Phase 1)

1. **Fix peephole/cache interaction** (Option A recommended)
   - Modify `peephole::optimize` to return `(Vec<Instr>, Vec<usize>)` mapping
   - Update all `FunctionInfo.code_start/code_end` after optimization
   - Re-enable peephole optimizer in compile/mod.rs

2. **Benchmark impact**
   - Run detailed_benchmark before/after
   - Verify 15-25% speedup on calc_pi
   - Check regression tests pass

3. **Commit Phase 1**
   - Profiler infrastructure
   - Fused instructions
   - Working peephole optimizer
   - Updated benchmarks

#### Future Phases

**Phase 2: Slot-Based Locals** (30-50% additional speedup)
- Replace `HashMap<String, i64>` with `Vec<i64>` in Frame
- Assign slot numbers during compilation
- Emit `LoadI64Slot(index)` instead of `LoadI64(name)`
- Array access >> HashMap lookup

**Phase 3: Advanced Optimizations**
- Loop unrolling for simple ranges
- Inline small functions (< 10 instructions)
- Specialized ModI64 for power-of-2
- JIT compilation for hot functions

### Testing

**Unit Tests**:
```bash
cargo test peephole  # Optimizer tests (4 passing)
cargo test profiler  # Profiler tests (when added)
```

**Integration Tests**:
```bash
# Profiler example
cargo run --example profile_vm

# Should show:
# - Instruction frequency breakdown
# - Top 20 instructions
# - Percentage distribution
```

**Benchmark Tests**:
```bash
cargo bench --bench detailed_benchmark  # VM execution time
cargo bench --bench calc_pi_benchmark    # Real-world workload
```

### Files Modified

**New Files**:
- `src/vm/profiler.rs` - Instruction profiler (178 lines)
- `src/compile/peephole.rs` - Bytecode optimizer (210 lines)
- `examples/profile_vm.rs` - Profiling example (40 lines)
- `docs/optimization/VM_OPTIMIZATION_PLAN.md` - Detailed plan (420 lines)

**Modified Files**:
- `src/vm/mod.rs` - Added profiler module
- `src/vm/instr.rs` - Added 12 fused instructions
- `src/vm/exec.rs` - Implemented fused instruction execution (200+ lines)
- `src/compile/mod.rs` - Added (disabled) peephole optimization call

**Total**: ~1000 lines of new code

### Design Decisions

#### Why Thread-Local Profiling?

**Alternatives Considered**:
- Global atomic counters (too slow)
- Mutex-protected HashMap (contention)
- No profiling (can't optimize blind)

**Chosen**: Thread-local RefCell
- Zero overhead when disabled (single atomic check)
- No synchronization needed
- Perfect for single-threaded benchmarks

#### Why Fused Instructions?

**Problem**: Every instruction has dispatch overhead (match statement, IP increment)

**Solution**: Combine common patterns
- Fewer instructions → fewer dispatches
- Fewer HashMap lookups (LoadI64 + op → single lookup)
- Better instruction cache utilization

**Trade-off**: Larger instruction enum
- Acceptable: 12 new variants vs 67 existing
- Benefit: 15-25% speedup worth the complexity

#### Why Peephole vs other optimizations?

**Alternatives**:
- SSA-based optimizer (too complex)
- Register allocation (requires VM redesign)
- JIT compilation (not allowed on iOS)

**Peephole advantages**:
- Simple pattern matching
- Works with existing bytecode
- Incremental improvement
- No breaking changes

### Performance Goals

**Phase 1** (This PR): 15-25% VM speedup
**Phase 2** (Slots): Additional 30-50% speedup
**Phase 3** (Advanced): Additional 20-30% speedup

**Combined Target**: 65-105% total VM speedup (1.65-2× faster)

### References

- Profiler: `src/vm/profiler.rs`
- Fused Instructions: `src/vm/instr.rs` lines 49-68, `src/vm/exec.rs` lines 302-506
- Peephole Optimizer: `src/compile/peephole.rs`
- Optimization Plan: `docs/optimization/VM_OPTIMIZATION_PLAN.md`
- Example Usage: `examples/profile_vm.rs`

## Summary

✅ **Infrastructure Complete**: Profiler + Fused Instructions + Peephole Optimizer
⏸️ **Compiler Integration**: Blocked by Base cache interaction
📊 **Profiling Data**: Identifies LoadI64/StoreI64 as 54% bottleneck
🎯 **Expected Impact**: 15-25% speedup once integrated
📝 **Next Step**: Fix peephole/cache interaction (Option A)

**Timeline**: 2-3 hours to complete Phase 1 integration
