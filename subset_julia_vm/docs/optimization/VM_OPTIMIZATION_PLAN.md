# VM Execution Optimization Plan

## Profiling Results (calc_pi N=100)

**Total instructions**: 801,106

### Top Instructions by Frequency

| Rank | Instruction | Count | Percent | Category |
|------|-------------|-------|---------|----------|
| 1 | LoadI64 | 296,021 | 36.95% | Load/Store |
| 2 | StoreI64 | 135,969 | 16.97% | Load/Store |
| 3 | JumpIfZero | 80,228 | 10.01% | Control Flow |
| 4 | PushI64 | 76,318 | 9.53% | Stack Ops |
| 5 | Jump | 56,114 | 7.00% | Control Flow |
| 6 | NeI64 | 49,826 | 6.22% | Comparison |
| 7 | ModI64 | 39,826 | 4.97% | Arithmetic |
| 8 | GtI64 | 20,402 | 2.55% | Comparison |
| 9 | AddI64 | 10,100 | 1.26% | Arithmetic |
| 10 | CallDynamicBinaryBoth | 10,003 | 1.25% | Dynamic Dispatch |

### Category Breakdown

- **Load/Store**: 53.92% (432K instructions)
- **Control Flow**: 17.01% (136K instructions)
- **Stack Operations**: 9.53% (76K instructions)
- **Comparisons**: 8.77% (70K instructions)
- **Arithmetic**: 6.23% (50K instructions)

## Key Insights

### 1. Load/Store Dominates (54% of execution time)

**Problem**: Variables are loaded from HashMap on every access
- `LoadI64` checks `frame.locals_i64.get()` - HashMap lookup
- `StoreI64` does `frame.locals_i64.insert()` - HashMap insert
- 432K HashMap operations for just 100×100 iterations!

**Root Cause**: Stack-based VM with named variables stored in HashMaps

### 2. Control Flow Overhead (17%)

**Problem**: Loops require multiple jumps and condition checks
```
while b != 0:
  1. Evaluate condition (NeI64)
  2. JumpIfZero to end
  3. Loop body
  4. Jump back to condition
```

### 3. Dynamic Dispatch Overhead

**CallDynamicBinaryBoth**: 10,003 calls (1.25%)
- Runtime type checking for operators
- Method table lookup

## Optimization Strategies

### Phase 1: Instruction Fusion (Quick Win) 🎯

**Target**: Reduce Load/Store overhead by 30-40%

**Approach**: Combine common instruction pairs into fused instructions

#### 1.1 Load-Op Fusion
```rust
// Before: 2 instructions
LoadI64("a")
AddI64

// After: 1 instruction
LoadAddI64("a")  // Load a, pop stack, add, push result
```

**Impact**: Eliminates 1 HashMap lookup per fusion
- LoadAdd, LoadSub, LoadMul, LoadMod
- Expected: 100K→70K load operations (30% reduction)

#### 1.2 Store-Arithmetic Fusion
```rust
// Before: 3 instructions
LoadI64("cnt")
AddI64
StoreI64("cnt")

// After: 1 instruction
IncVar("cnt")  // cnt += (top of stack)
```

**Impact**: Pattern appears ~10K times in calc_pi
- Eliminates 20K HashMap operations
- Expected: 5-10% overall speedup

#### 1.3 Compare-Jump Fusion
```rust
// Before: 2 instructions
NeI64
JumpIfZero(addr)

// After: 1 instruction
JumpIfNe(addr)  // Compare and jump in one step
```

**Impact**: Reduces instruction dispatch overhead
- Expected: 2-3% speedup

**Total Phase 1 Expected Speedup**: 15-25%

### Phase 2: Local Variable Optimization (High Impact) 🎯

**Target**: Replace HashMap with array-based locals

**Current**:
```rust
frame.locals_i64: HashMap<String, i64>
frame.locals_f64: HashMap<String, f64>
```

**Optimized**:
```rust
frame.local_slots_i64: Vec<i64>  // Index-based access
frame.local_slots_f64: Vec<f64>
frame.var_to_slot: HashMap<String, usize>  // Compile-time mapping
```

**Compiler Changes**:
1. Assign slot numbers to variables during compilation
2. Emit `LoadI64Slot(slot_index)` instead of `LoadI64(name)`
3. `LoadI64Slot(3)` → `frame.local_slots_i64[3]` (array access, not HashMap)

**Impact**:
- HashMap lookup: O(log n) or O(1) with collisions
- Array access: O(1) always, much faster
- Expected: 30-50% speedup on load-heavy code

**Complexity**: Medium (requires compiler changes)

### Phase 3: Loop Optimizations (Medium Impact)

#### 3.1 Loop Counter Optimization
```julia
for i in 1:N  # Very common pattern
```

**Current**: Creates Range object, iterates with calls
**Optimized**: Dedicated `ForRangeI64` instruction with internal counter

**Impact**: 20-30% faster for simple for-loops

#### 3.2 While Loop Specialization
```julia
while b != 0  # Hot loop in GCD
```

**Optimized**: `WhileNe` instruction that combines condition check + jump

### Phase 4: Specialized Instructions

#### 4.1 ModI64 Fast Path
ModI64 appears 39K times (5% of execution)
- Check for common cases (power of 2 moduli)
- Inline fast path for small divisors

#### 4.2 Small Function Inlining
```julia
function gcd(a, b)  # Called 10K times
```

**Compiler optimization**: Inline functions with <10 instructions

## Implementation Priority

### Immediate (Phase 1 - This PR)
- [ ] Implement LoadAddI64, LoadSubI64, LoadMulI64, LoadModI64
- [ ] Implement IncVarI64, DecVarI64
- [ ] Implement JumpIfNe, JumpIfEq, JumpIfLt, JumpIfGt
- [ ] Update compiler to emit fused instructions
- [ ] Benchmark impact

**Expected Result**: 15-25% VM speedup

### Near-term (Phase 2 - Next PR)
- [ ] Add slot-based variable storage
- [ ] Update compiler to assign variable slots
- [ ] Emit LoadI64Slot/StoreI64Slot instructions
- [ ] Benchmark impact

**Expected Result**: Additional 30-50% VM speedup

### Future (Phase 3+)
- [ ] Loop optimizations
- [ ] Function inlining
- [ ] JIT compilation for hot functions

## Benchmark Plan

### Test Programs

1. **calc_pi(100)** - Heavy on loops, modulo, comparisons
2. **fibonacci(20)** - Recursive calls
3. **matrix_ops** - Array operations
4. **simple_arithmetic** - Baseline

### Metrics

- Total instruction count
- Execution time (µs)
- Instructions per microsecond (throughput)

### Success Criteria

**Phase 1**: 15-25% speedup on calc_pi
**Phase 2**: Additional 30-50% speedup (total 45-75% from baseline)

## Implementation Notes

### Adding Fused Instructions

1. Define in `src/vm/instr.rs`:
```rust
pub enum Instr {
    // ...
    LoadAddI64(String),  // Load var, add to stack top, push result
    LoadSubI64(String),
    LoadMulI64(String),
    LoadModI64(String),
    IncVarI64(String),   // var += stack top
    JumpIfNe(usize),     // Compare stack top two values, jump if !=
    // ...
}
```

2. Implement in `src/vm/exec.rs`:
```rust
Instr::LoadAddI64(name) => {
    let var_val = /* load from locals */;
    let stack_val = pop_i64(&mut self.stack)?;
    self.stack.push(Value::I64(var_val + stack_val));
}
```

3. Emit from compiler in `src/compile/mod.rs`:
```rust
// Detect pattern: LoadI64(x) + AddI64
if matches!(prev_instr, Instr::LoadI64(name)) && matches!(current, AddI64) {
    emit(Instr::LoadAddI64(name));  // Fuse!
} else {
    emit(prev_instr);
    emit(current);
}
```

### Maintaining Correctness

- Fused instructions must be semantically equivalent
- Test with fixture_tests to ensure identical behavior
- Profile before/after to verify improvement

## References

- Profiler: `examples/profile_vm.rs`
- VM execution: `src/vm/exec.rs`
- Instruction definitions: `src/vm/instr.rs`
- Compiler: `src/compile/mod.rs`
