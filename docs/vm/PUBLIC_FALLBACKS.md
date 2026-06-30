# Public Base Fallback Routes

This is the documented inventory for public Base names that still have a Rust
builtin route or compiler-recognized route in sjulia.

The source of truth is `BASE_FUNCTION_ROUTES` in
`subset_julia_vm/src/compile/base_functions.rs`. The audit
`scripts/check_base_routing_registry.sh` verifies that this document stays in
sync with the registry.

## Route Kinds

| Kind | Meaning |
|------|---------|
| `DispatchFirst` | Public calls must try Julia/user methods first; Rust is only a primitive representation or cache-compatibility fallback. |
| `DirectBuiltin` | Public call intentionally compiles directly to a Rust builtin for this supported slice. |
| `RuntimeBoundary` | Operation crosses VM, OS, RNG, native ABI, or external library state. |
| `InternalIntrinsic` | Underscored helper used by sjulia's Pure Julia Base implementation. |
| `CompilerIntrinsic` | Constructor or compiler helper with special lowering semantics. |

## Inventory

| Name | Kind | Upstream Reference |
|------|------|--------------------|
| `rand` | `RuntimeBoundary` | `julia/stdlib/Random/src/Random.jl` |
| `sqrt` | `DispatchFirst` | `julia/base/math.jl` |
| `time_ns` | `RuntimeBoundary` | `julia/base/c.jl` |
| `length` | `DispatchFirst` | `julia/base/abstractarray.jl` |
| `size` | `DispatchFirst` | `julia/base/abstractarray.jl` |
| `ndims` | `DispatchFirst` | `julia/base/abstractarray.jl` |
| `push!` | `DispatchFirst` | `julia/base/array.jl` |
| `pop!` | `DispatchFirst` | `julia/base/array.jl` |
| `pushfirst!` | `DispatchFirst` | `julia/base/array.jl` |
| `popfirst!` | `DispatchFirst` | `julia/base/array.jl` |
| `insert!` | `DispatchFirst` | `julia/base/array.jl` |
| `deleteat!` | `DispatchFirst` | `julia/base/array.jl` |
| `reshape` | `DispatchFirst` | `julia/base/array.jl` |
| `zero` | `DispatchFirst` | `julia/base/number.jl` |
| `lu` | `DispatchFirst` | `julia/stdlib/LinearAlgebra/src/lu.jl` |
| `det` | `DispatchFirst` | `julia/stdlib/LinearAlgebra/src/dense.jl` |
| `StableRNG` | `RuntimeBoundary` | `julia/stdlib/Random/src/RNGs.jl` |
| `Xoshiro` | `RuntimeBoundary` | `julia/stdlib/Random/src/Xoshiro.jl` |
| `MersenneTwister` | `RuntimeBoundary` | `julia/stdlib/Random/src/RNGs.jl` |
| `randn` | `RuntimeBoundary` | `julia/stdlib/Random/src/normal.jl` |
| `_tuple_first` | `InternalIntrinsic` | `julia/base/tuple.jl` |
| `_tuple_last` | `InternalIntrinsic` | `julia/base/tuple.jl` |
| `delete!` | `DispatchFirst` | `julia/base/dict.jl` |
| `get!` | `DispatchFirst` | `julia/base/dict.jl` |
| `empty!` | `DispatchFirst` | `julia/base/dict.jl` |
| `keys` | `DispatchFirst` | `julia/base/abstractdict.jl` |
| `values` | `DispatchFirst` | `julia/base/abstractdict.jl` |
| `pairs` | `DispatchFirst` | `julia/base/abstractdict.jl` |
| `merge!` | `DispatchFirst` | `julia/base/dict.jl` |
| `Ref` | `CompilerIntrinsic` | `julia/base/refvalue.jl` |
| `typeof` | `DirectBuiltin` | `julia/base/essentials.jl` |
| `isa` | `DirectBuiltin` | `julia/base/operators.jl` |
| `eltype` | `DispatchFirst` | `julia/base/abstractarray.jl` |
| `keytype` | `DispatchFirst` | `julia/base/abstractdict.jl` |
| `valtype` | `DispatchFirst` | `julia/base/abstractdict.jl` |
| `sizeof` | `DispatchFirst` | `julia/base/essentials.jl` |
| `isbits` | `DispatchFirst` | `julia/base/runtime_internals.jl` |
| `isbitstype` | `DispatchFirst` | `julia/base/runtime_internals.jl` |
| `_supertype` | `InternalIntrinsic` | `julia/base/reflection.jl` |
| `_typename` | `InternalIntrinsic` | `julia/base/reflection.jl` |
| `_function_name` | `InternalIntrinsic` | `julia/base/reflection.jl` |
| `subtypes` | `DirectBuiltin` | `julia/base/reflection.jl` |
| `hasfield` | `DispatchFirst` | `julia/base/runtime_internals.jl` |
| `ismutable` | `DispatchFirst` | `julia/base/runtime_internals.jl` |
| `objectid` | `DispatchFirst` | `julia/base/runtime_internals.jl` |
| `_methods_by_ftype` | `InternalIntrinsic` | `julia/Compiler/src/methodtable.jl` |
| `hasmethod` | `DispatchFirst` | `julia/base/reflection.jl` |
| `in` | `DispatchFirst` | `julia/base/operators.jl` |
| `∈` | `DispatchFirst` | `julia/base/operators.jl` |
| `∉` | `DispatchFirst` | `julia/base/operators.jl` |
| `∋` | `DispatchFirst` | `julia/base/operators.jl` |
| `∌` | `DispatchFirst` | `julia/base/operators.jl` |
| `iterate` | `DispatchFirst` | `julia/base/essentials.jl` |
| `collect` | `DispatchFirst` | `julia/base/array.jl` |
| `Generator` | `CompilerIntrinsic` | `julia/base/generator.jl` |
| `gensym` | `CompilerIntrinsic` | `julia/base/expr.jl` |
| `macroexpand` | `CompilerIntrinsic` | `julia/base/reflection.jl` |
| `macroexpand!` | `CompilerIntrinsic` | `julia/base/reflection.jl` |
| `getindex` | `DispatchFirst` | `julia/base/abstractarray.jl` |
| `setindex!` | `DispatchFirst` | `julia/base/abstractarray.jl` |
| `ncodeunits` | `DispatchFirst` | `julia/base/strings/basic.jl` |
| `codeunit` | `DispatchFirst` | `julia/base/strings/basic.jl` |
| `codeunits` | `DispatchFirst` | `julia/base/strings/basic.jl` |
| `isvalid` | `DispatchFirst` | `julia/base/strings/basic.jl` |
| `string` | `DirectBuiltin` | `julia/base/strings/io.jl` |
| `sprintf` | `DirectBuiltin` | `julia/stdlib/Printf/src/Printf.jl` |
| `bitstring` | `DispatchFirst` | `julia/base/intfuncs.jl` |
| `codepoint` | `DispatchFirst` | `julia/base/char.jl` |
| `isnumeric` | `DispatchFirst` | `julia/base/strings/unicode.jl` |
| `unescape_string` | `DispatchFirst` | `julia/base/strings/io.jl` |
| `parse` | `DispatchFirst` | `julia/base/parse.jl` |
| `tryparse` | `DispatchFirst` | `julia/base/parse.jl` |
| `_tryparse_float64` | `InternalIntrinsic` | `julia/base/parse.jl` |
| `big` | `DispatchFirst` | `julia/base/gmp.jl` |
| `convert` | `DispatchFirst` | `julia/base/essentials.jl` |
| `promote` | `DispatchFirst` | `julia/base/promotion.jl` |
| `signed` | `DispatchFirst` | `julia/base/number.jl` |
| `unsigned` | `DispatchFirst` | `julia/base/number.jl` |
| `memoryref` | `InternalIntrinsic` | `julia/base/essentials.jl` |
| `memoryrefnew` | `InternalIntrinsic` | `julia/base/essentials.jl` |
| `memoryrefget` | `InternalIntrinsic` | `julia/base/essentials.jl` |
| `memoryrefset!` | `InternalIntrinsic` | `julia/base/essentials.jl` |
| `memoryrefoffset` | `InternalIntrinsic` | `julia/base/genericmemory.jl` |
| `memoryrefparent` | `InternalIntrinsic` | `julia/base/genericmemory.jl` |

## Current Boundary

This inventory does not mean every route is fully Julia-compatible. It prevents
unclassified public fallback expansion and classifies the retained routes as
dispatch-first boundaries, primitive/runtime representation bridges, or direct
intrinsics. Issue #4276 closed after adding the route inventory, CI sync audit,
and representative dispatch-first fixtures. Remaining final cleanup is tracked
by Issue #4568. `Value::Array` has already been retired; the remaining
VM-native array boundaries use explicit `Value::NativeArray` converter helpers
while call sites continue moving toward Memory primitives and Pure Julia
`Array{T,N}` dispatch.
