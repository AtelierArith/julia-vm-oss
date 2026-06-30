# Phase 1: Base Compilation Cache - Design Document

## Overview

Phase 1 aims to dramatically reduce compilation time by caching the compilation
result of Base functions (~460 functions from prelude).

## Performance Analysis

### Current Bottleneck (Identified via Profiling)

```
Program Statistics:
- Total functions: 461
- Base functions: 460  ← Recompiled every time!
- User functions: 1
- Compile time: 3.9-4.1 ms (95%+ spent on Base functions)
```

### Expected Improvement

- **Current**: 4.1 ms compile time
- **Target**: 0.3-0.5 ms compile time
- **Reduction**: ~80-90% (3.5-3.8 ms saved)

## Technical Challenges

### 1. Thread Safety (`Rc` in `CompiledProgram`)

**Problem**: `CompiledProgram` contains `Rc<RefCell<ArrayValue>>` which is:
- Not `Send` (cannot be sent between threads)
- Not `Sync` (cannot be shared between threads)

This prevents using `Lazy<Arc<CompiledProgram>>` for caching.

**Possible Solutions**:
- A. Replace `Rc` with `Arc` throughout the codebase (large change)
- B. Use thread-local caching instead of global cache
- C. Serialize/deserialize compiled bytecode (overhead)
- D. Redesign `CompiledProgram` to be thread-safe

### 2. Function Boundary Tracking

**Problem**: Need to know which bytecode instructions belong to which function.

**Current State**: Functions are compiled sequentially into a single instruction
vector, but boundaries are not explicitly tracked.

**Required**:
- Track instruction offsets for each function
- Enable extracting Base instructions separately from user instructions

### 3. Method Table Merging

**Problem**: Method tables are built during compilation and reference function indices.

**Challenge**: Merging cached Base method tables with newly compiled user
method tables requires careful index management.

## Proposed Implementation Approaches

### Approach A: Incremental Compilation (Recommended)

**Strategy**:
1. Compile Base functions once, store in a `CompiledBase` struct
2. For user programs, compile only user functions
3. Merge Base + user at runtime

**Pros**:
- Most correct approach
- Largest performance gain

**Cons**:
- Complex implementation (2-3 hours)
- Requires architectural changes

### Approach B: Lazy Compilation

**Strategy**:
- Only compile functions when first called
- Cache compiled functions in a HashMap

**Pros**:
- Simpler than full caching
- Benefits long-running REPLs

**Cons**:
- First call still slow
- More complex dispatch logic

### Approach C: Bytecode Serialization

**Strategy**:
1. Pre-compile Base to bytecode file at build time
2. Load bytecode file at runtime

**Pros**:
- No runtime compilation for Base
- Clean separation

**Cons**:
- Build complexity
- Versioning challenges

## Implementation Roadmap

### Step 1: Make `CompiledProgram` Thread-Safe
- [ ] Replace `Rc` with `Arc` in `ArrayValue`
- [ ] Verify no performance regression
- [ ] Update tests

### Step 2: Add Function Boundary Tracking
- [ ] Extend `FunctionInfo` with `code_start` and `code_end` fields
- [ ] Track boundaries during compilation
- [ ] Add accessor methods

### Step 3: Implement Base Cache
- [ ] Create `compile/cache.rs` module
- [ ] Implement `Lazy<Arc<CompiledBase>>`
- [ ] Add cache initialization logic

### Step 4: Implement Cache Merging
- [ ] Create `merge_with_base_cache()` function
- [ ] Handle method table index offsets
- [ ] Merge instruction vectors

### Step 5: Integration
- [ ] Update `compile_core_program` to use cache
- [ ] Add feature flag for cache (optional)
- [ ] Benchmark improvements

## Alternative: Quick Win Optimizations

While Phase 1 is being designed, these smaller optimizations can provide
immediate benefits:

1. **Parallel Compilation**: Compile independent functions in parallel
2. **Compilation Profiling**: Add detailed timing to identify other bottlenecks
3. **Struct Table Optimization**: Cache struct table construction
4. **Type Inference Optimization**: Memoize type inference results

## Benchmarking Plan

```rust
// Before optimization
compile_simple_with_base    time:   [4.15 ms]  // 460 Base + 0 user
compile_with_base           time:   [3.89 ms]  // 460 Base + 1 user

// After optimization (target)
compile_simple_with_cache   time:   [0.30 ms]  // 0 compile + 0 user
compile_with_cache          time:   [0.50 ms]  // 0 compile + 1 user
```

## References

- Benchmarks: `benches/compile_profiling.rs`
- Main compiler: `src/compile/mod.rs`
- VM types: `src/vm/value.rs` (Rc usage)

## Status

**Current**: Design phase
**Blocker**: Thread safety (`Rc` → `Arc` migration)
**Next Step**: Evaluate `Rc` → `Arc` migration impact

## Notes

- Phase 2 (inline optimizations) achieved 2-3% improvement
- Phase 1 (Base cache) targets 80-90% improvement
- Combined: ~82% total improvement possible
