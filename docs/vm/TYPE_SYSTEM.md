# Type System Architecture

*Last updated: 2026-07-02*

This document describes the type system used in SubsetJuliaVM, including the relationships between different type representations.

## Executable documentation sweep status (Issue #8721)

Initial sweep date: 2026-07-02.

- Reviewed as one of the five major docs named by #8694/#8721.
- Corrected stale type-system ownership/count claims in the `ConcreteType` and
  `Value` sections.
- Reclassified the old parametric `where T` dispatch limitation as historical:
  #8608 fixed the documented `foo(Int64(6), Int64(2))` example so it now returns
  `3 :: Int64` instead of the previously documented `3.0 :: Float64`.
- Representative behavior claims are now covered by `julia-doctest` blocks
  and the nightly docs doctest gate.

```julia-doctest
function foo(x::T, y::T) where T
    x ÷ y
end

result = foo(6, 2)
println(result)
println(typeof(result))
# output
3
Int64
```

## Type Representations

SubsetJuliaVM uses four distinct type representations at different compilation stages:

> **Runtime dispatch identity** is a separate concern from these compile-time
> representations. Issue #9197 introduces a fifth, runtime-only identity layer —
> the session-scoped `ConcreteTypeId(u32)` intern registry — that replaces the
> type-name strings and unverified hashes the dispatch caches key on today. Its
> design (key structure, invalidation, REPL boundary, per-slice consumers) lives
> in [TYPE_INTERNING.md](./TYPE_INTERNING.md).
>
> **Semantic ownership identity** is the broader migration tracked by Issue
> #10459: TypeVars, structs, functions, and methods need owner-scoped IDs rather
> than display-name strings. The inventory and phased target model live in
> [SEMANTIC_IDENTITIES.md](./SEMANTIC_IDENTITIES.md).

### 0. CoreType (Shared Migration Target)

Located in: `subset_julia_vm/src/inference_core/type_core.rs`

`CoreType` is the shared structured Julia type core introduced as the migration
target for the compiler, VM-facing type logic, and AoT inference (Issues #3826
and #3829). It does not replace `JuliaType`, `LatticeType`, `ValueType`, or
`StaticType` in one step; it provides a common semantic shape that those
projections can convert into before answering shared questions.

The first supported shape includes:

- primitive concrete types (`Int64`, `Float64`, `Bool`, `String`, `Nothing`, etc.)
- built-in abstract types (`Number`, `Real`, `Integer`, `AbstractArray`,
  `AbstractVector`, `AbstractMatrix`, `AbstractDict`, `AbstractSet`,
  `AbstractRange`, `AbstractUnitRange`, `IO`, `Type`, etc.)
- user-defined abstract types with parent metadata
- structured parametric structs (`Struct { name, params }`) instead of opaque `Foo{T}` strings
- tuples, named tuples, `Union`, `UnionAll`, `TypeVar`, `Type{T}`, `Vararg`,
  fixed-length `Vararg{T,N}`, and bits-like value parameters used by type
  expressions such as `Val{1}` and `Array{T,2}`
- string type names for `Union{...}`, `Tuple{...}`, `Type{...}`, and
  `Vararg{...}` are parsed into those structured forms rather than treated as
  ordinary parametric structs

Initial production users:

- `types::JuliaType::is_builtin_numeric()` now converts through `CoreType`
- VM runtime primitive numeric value checks also convert through `CoreType`
- VM `isbits(x)` / `isbitstype(T)` now use `CoreType::is_builtin_bits_type()`
  for built-in primitive layout facts
- VM `sizeof(x)` now uses `CoreType::builtin_sizeof_bytes()` for fixed-size
  built-in primitive and singleton layout facts
- VM `isprimitivetype(T)` now uses
  `CoreType::is_builtin_primitive_datatype()` for built-in DataType flag facts
- VM `isabstracttype(T)` now uses
  `CoreType::is_builtin_abstract_datatype()` for built-in DataType flag facts
- VM `isconcretetype(T)` now uses
  `CoreType::is_builtin_concrete_datatype()` for built-in DataType flag facts
- VM `isstructtype(T)` now uses
  `CoreType::is_builtin_struct_datatype()` for built-in DataType flag facts
- VM `ismutabletype(T)` now uses
  `CoreType::is_builtin_mutable_datatype()` for built-in DataType flag facts
- `types::JuliaType::specificity()` delegates to `CoreType::specificity()`
- `types::JuliaType::is_subtype_of()` now asks `CoreType` for positive
  structured built-in subtype facts before falling back to its legacy cases.
  Since Issue #5915 (wave 5) the Union decomposition and the `Type{}`
  invariance/covariance arms are decided entirely by the engine's `CoreType`
  solver (the local copies were deleted); the only `Type{}`-related local
  residue is the permissive fallback for `Type{<:Bound}` bound names
  `JuliaType::from_name` cannot resolve (this enum-level check has no struct
  hierarchy to consult)
- `compile::lattice::{ConcreteType, LatticeType}` can now convert into
  `CoreType`; lattice numeric classification delegates its shared hierarchy
  checks through the same core while preserving lattice-specific Bool handling.
  `LatticeType::is_subtype_of()` also uses `CoreType` for concrete and union
  hierarchy queries before retaining tuple-specific lattice rules.
- AoT `StaticType::primitive_numeric()` now converts through `CoreType`
- AoT `StaticType::core_typejoin()` now joins through `CoreType::typejoin()`
  and projects the result back with `StaticType::from_core_type_lossy()` when
  the joined shape still has a backend/codegen representation. This moves
  tuple/union normalization for AoT merge points into the shared type core
  while keeping `StaticType` as a projection rather than a semantic owner.
- `compile::method_table` now uses `CoreType::specificity()` for its base
  method-parameter specificity score
- VM untyped dynamic dispatch now computes its high-priority runtime
  pre-score through `CoreType::dispatch_pattern_score()` for exact matches,
  parametric type-variable patterns, tuple `Any` patterns, bare struct-family
  matches, and array-family matches. Ordinary subtype fallback scoring remains
  in the VM caller so existing method priority is preserved.
- VM typed dynamic dispatch now uses `CoreType` for runtime pattern hierarchy
  checks and pattern specificity, aligning candidate selection with compiler
  method table hierarchy facts.
- `MethodSig::core_signature()` exposes method signatures as
  `Tuple{...}` optionally wrapped in `UnionAll`
- `compile::method_table::MethodSig` now stores that structured signature as
  `core_signature` for newly registered methods. Historical `params` and
  `type_params` remain as compatibility projections, but duplicate
  replacement uses the structured signature when available.
- `MethodSig::arg_core_types()` (Issue #6336) projects the argument core
  types out of `core_signature` (`UnionAll` wrappers stripped) and is the
  accessor dispatch readers migrate onto instead of re-deriving types from
  the legacy `params` projection; the empty-trailing-vararg dominance
  pre-check already reads it. The shared specificity module
  (`inference_core/specificity.rs`) no longer performs ad-hoc type-name
  string parsing in dispatch: abstract container parameters and `where`
  upper bounds are structured once through the central `CoreType` bridge
  (`CoreType::from` / `from_julia_name`, `type_param_upper_bound_core`) and
  compared as `CoreType` values.
- `core_signature` is the single SERIALIZED source of truth for method
  signatures (Issue #6336, CACHE_VERSION 45): the `MethodSig` wire format
  carries only `core_signature` + display `param_names`; the legacy
  `params`/`type_params` are non-serialized in-memory projections,
  reconstructed at deserialization through the canonical inverse
  (`inference_core::core_type_to_julia_type` /
  `core_type_var_to_type_param`). Reconstruction exactness over the whole
  Base corpus (the only deserialized population) is pinned by the
  `compile/cache.rs` round-trip gates; user signature shapes are covered in
  `method_table.rs`. The projections themselves are retired when the legacy
  `JuliaType`-shaped matcher pipeline is ported to `CoreType`
  (Issue #6495); the `CallDynamic`/`CallTypedDispatch` name-string
  candidate payloads are Issue #6496.
- VM runtime subtype checks now ask `CoreType` first for structured built-in
  relationships before falling back to reflection/user-defined ancestry. This
  covers `Tuple{...}`, `Union{...}`, `Type{T}`, array/vector/matrix,
  dict/set, range, and `IOBuffer <: IO` families from the same shared path.
- VM runtime `typeintersect(a, b)` now uses `CoreType::type_intersect()` for
  structured built-in non-subtype cases after preserving user-defined subtype
  registry checks
- VM runtime `.parameters` reflection now uses `CoreType::type_parameters()`
  instead of a local braced-string parser
- VM runtime type-object reflection now goes through
  `vm::type_objects::RuntimeTypeRegistry`, a read model over
  `Value::DataType(JuliaType)` plus `CoreType`. This is the central place for
  `DataType`, `UnionAll`, and `TypeVar` metadata used by reflection helpers
  (`.parameters`, `.var`, `.body`, TypeVar `.name` / `.lb` / `.ub`,
  built-in field names/types), while
  `Value::DataType` remains the compact runtime value projection.
- VM runtime `fieldnames` / `fieldtypes` reflection for builtin AST-like types
  now reads field metadata from `CoreType::builtin_field_metadata()`
- VM runtime `isstructtype` / `isconcretetype` / `ismutabletype` classify
  builtin AST/runtime object types such as `Expr`, `QuoteNode`, `LineNumberNode`,
  `GlobalRef`, and `Module` through `CoreType`
- VM runtime reflection for `supertype(T)` now asks
  `CoreType::direct_builtin_supertype_name()` first for built-in direct parent
  names. User-defined struct and abstract type parents still come from the VM
  registry fallback; unknown parametric names fall back to their base name.
  **Exception (Issues #10282, #10283):** rank/dim-generic type ALIASES
  (`Vector`/`Matrix`/`AbstractVector`/`AbstractMatrix`/`DenseVector`/
  `DenseMatrix` — upstream `UnionAll`s such as `AbstractMatrix{T} =
  AbstractArray{T,2}`) are handled by
  `vm::type_objects::rank_generic_alias_bare_supertype_name()` BEFORE reaching
  `direct_builtin_supertype_name()`, because upstream `supertype` on a
  `UnionAll` recurses through the body (`supertype(u::UnionAll) =
  UnionAll(u.var, supertype(u.body))`) rather than walking the is-a/subtype
  hierarchy: `supertype(AbstractMatrix) == Any` (not `AbstractArray`) and
  `supertype(Vector) == DenseVector` (not the rank-erased `DenseArray`), even
  though `AbstractMatrix <: AbstractArray` and `Vector <: DenseArray` remain
  true for `<:`/dispatch purposes via the unmodified
  `direct_builtin_supertype_name()` chain.
- VM runtime reflection for `subtypes(T)` now asks
  `CoreType::direct_builtin_subtype_names()` for built-in direct children,
  keeping the built-in child list aligned with the same direct-supertype table.
  User-defined children still come from the VM registry fallback.
- `compile::type_helpers::check_type_satisfies_bound()` now uses `CoreType`
  before falling back to historical ancestor lists, so bounded type variables
  see the same structured built-in families.
- `compile::type_helpers::get_builtin_type_ancestors()` now builds known
  built-in ancestor chains from `CoreType::builtin_supertype_chain_names()`;
  unknown user-defined types still use the historical fallback.
- `types::JuliaType::{is_concrete,is_concrete_primitive,is_abstract_integer,is_abstract_numeric,is_primitive}`
  now project through `CoreType` classifiers. `JuliaType` remains a
  stage-specific representation; built-in dispatch primitive and abstract
  numeric classification policy is owned by the shared core.
- `compile::method_table` type-variable bound checks now use `CoreType`
  directly, so structured families such as `Vector{T} <: AbstractVector` are
  validated by the same semantic operation used by runtime/reflection paths.
- `CoreType` now keeps representative value parameters as structured
  `CoreValueParam` values instead of opaque `Named` fallback strings. This
  covers `Val{1}`, array dimensional parameters (`Array{T,1}` /
  `Array{T,2}`), fixed `Vararg{T,N}`, and `NTuple{N,T}` alias-style tuple
  patterns for shared subtype checks.

This keeps existing behavior while creating a safe migration path for later
Julia-compatible subtyping, type intersection, method specificity, and lattice
unification work.

Shared operations currently available on `CoreType`:

- `is_subtype_of(a, b)` for primitive/abstract hierarchy, unions, tuple
  covariance including trailing and fixed-length `Vararg`, `UnionAll`
  right-hand patterns with bounded type variables, repeated covariant `TypeVar`
  diagonal constraints, `Type{T}`, invariant parametric structs, representative
  value parameters, and selected built-in families such as array/range/dict/set
  / IO structs
- `is_builtin_bits_type()` for built-in primitive `isbits` / `isbitstype`
  reflection
- `is_builtin_primitive_datatype()` for built-in `isprimitivetype`
  reflection
- `is_builtin_abstract_datatype()` for built-in `isabstracttype`
  reflection
- `is_builtin_concrete_datatype()` for built-in `isconcretetype`
  reflection, including fully-specified built-in parametric struct families
- `is_builtin_mutable_datatype()` for built-in `ismutabletype`
  reflection
- `type_intersect(a, b)` with union distribution, tuple element-wise
  intersection including trailing `Vararg`, `Type{T}` intersection, and
  `Bottom` for provable disjointness
- `typejoin(a, b)` for conservative shared joins, including fixed tuple
  element-wise joins
- `specificity()` as the bridge toward replacing local dispatch scoring
- `dispatch_pattern_score()` as the bridge for VM runtime dispatch pre-scoring
- `direct_builtin_supertype_name()` as the bridge toward moving runtime type
  object reflection onto the same hierarchy facts
- `direct_builtin_subtype_names()` for built-in `subtypes(T)` reflection
- `builtin_supertype_chain_names()` for callers that still need an ordered
  ancestor chain during migration
- `is_builtin_dispatch_primitive()`, `is_builtin_abstract_numeric()`,
  `is_builtin_abstract_integer_accepting()`, and
  `is_builtin_dispatch_primitive_or_abstract_numeric()` for legacy dispatch
  classifier projections that should not keep local `JuliaType` match tables

### 1. LatticeType (Compile-time)

Located in: `subset_julia_vm_compile/src/compile/lattice/types.rs`

`LatticeType` is the compile-time abstract interpretation type used during type inference. It provides the most precise type information and supports:

- **Bottom**: Unreachable code marker
- **Const**: Specific constant values known at compile time (e.g., `Const(42)`)
- **Concrete types**: Single known types (e.g., `Int64`, `Float64`)
- **Union types**: Multiple possible types (e.g., `Union{Int64, Float64}`)
- **Conditional types**: Flow-sensitive types that depend on control flow
- **Top**: Unknown type (fallback, equivalent to `Any`)

```rust
pub enum LatticeType {
    Bottom,                              // Unreachable
    Const(ConstValue),                   // Constant value (more specific than Concrete)
    Concrete(ConcreteType),              // Known type
    Union(BTreeSet<ConcreteType>),       // Union of types
    Conditional { slot, then_type, else_type }, // Flow-sensitive
    Top,                                 // Unknown (Any)
}
```

`ConstValue` supports `Int64`, `Float64`, `Bool`, `String`, `Symbol`, and `Nothing` constant values for compile-time constant propagation.

#### ConcreteType

Located in: `subset_julia_vm/src/runtime_types/lattice.rs`
(`subset_julia_vm_compile/src/compile/lattice/types.rs` is now only a re-export shim).

`ConcreteType` represents specific Julia types inside the lattice. Since the
CoreType unification work, nullary primitive/abstract/builtin facts (`Int*`,
`Float*`, `Bool`, `String`, `Char`, `Symbol`, `Nothing`, `Missing`, `Number`,
`Integer`, `AbstractFloat`, `IO`, `Any`, and related builtins) are represented
by the shared `Core(CoreType)` variant. Dedicated `ConcreteType` variants are
reserved for parametric/container/callable/runtime carrier shapes. There are
currently **24 variants**:

- **Shared core projection** (1): `Core(CoreType)`
- **Composite/Collection** (9): `Array{element, ndims}`, `Tuple{elements}`,
  `TupleVararg{elements, tail}`, `NamedTuple{fields}`, `Range{element}`,
  `Dict{key, value}`, `Set{element}`, `Generator{element}`, `Pairs`
- **User-defined** (1): `Struct{name, type_id}`
- **Callable** (3): `Function{name}`, `Closure{name, captures}`, `ComposedFunction{outer, inner}`
- **Type system** (2): `DataType{name}`, `Module{name}`
- **Metaprogramming** (4): `Expr`, `QuoteNode`, `LineNumberNode`, `GlobalRef`
- **Pattern matching** (2): `Regex`, `RegexMatch`
- **Type unions** (1): `UnionOf(Vec<ConcreteType>)`
- **Enum** (1): `Enum{name}` — for Julia `@enum` types (Issue #2863)

Note: `Dict{key, value}` is parametric in `ConcreteType`, allowing the compile-time lattice to track key/value types for `Dict{K,V}`.

### 2. ValueType (Runtime)

Located in: `subset_julia_vm_vm/src/vm/value/value_enum.rs`

`ValueType` is the runtime type tag used for:
- Method dispatch
- VM instruction selection
- Type checking at runtime

There are currently **49 variants** in the `ValueType` enum:

```rust
pub enum ValueType {
    // Signed integers
    I8, I16, I32, I64, I128, BigInt,
    // Unsigned integers
    U8, U16, U32, U64, U128,
    // Boolean
    Bool,
    // Floating point
    F16, F32, F64, BigFloat,
    // Collections
    Array, ArrayOf(ArrayElementType, Option<usize>),
    Memory, MemoryOf(ArrayElementType),
    Range,
    // String types
    Str, Char,
    // Special types
    Nothing, Missing,
    Struct(usize), Rng, Tuple, NamedTuple, Pairs,
    Dict, Set, Generator,
    DataType, Module, Function, IO,
    // Macro system types
    Symbol, Expr, QuoteNode, LineNumberNode, GlobalRef,
    // Regex types
    Regex, RegexMatch,
    // Dynamic / union / enum
    Any,
    Union(Vec<ValueType>),
    Enum,
    // Concrete Complex scalar tags for runtime specialization
    ComplexF32, ComplexF64,
}
```

#### `ArrayOf(ArrayElementType::Any, Some(n))`: Known vs Unknown (Issue #10267, #10206)

`ArrayOf`'s `Any` element tag is produced by two structurally different
sources that share the exact same shape, and this matters because
`infer_julia_type`'s `Expr::Var` bridge must decide, from `ValueType` alone,
whether to report the *concrete* `VectorOf(Any)`/`MatrixOf(Any)` (lets a
`::Array{Any,N}`-typed method statically bind) or the rank-only bare alias
`Struct("Vector")`/`Struct("Matrix")` (defers element-specific methods to
runtime dispatch on the concrete value):

1. **Genuinely `Any`** — a value that really is `Vector{Any}`/`Matrix{Any}` at
   both compile time and runtime (e.g. `expr.args`, an array literal like
   `x = []`/`[1, "a"]`). Upstream `typeof` is exactly `Vector{Any}` — a
   concrete, dispatchable type — so the bridge must report the concrete form.
2. **Unknown due to incomplete inference** — a comprehension whose body
   expression's return type could not be statically resolved (Issue #6817).
   The *rank* is known (iterator clause count) but the *element* is a
   placeholder, not proof the runtime value is `Vector{Any}` (it could be
   `Vector{Int64}` etc). The bridge must report the bare alias so
   element-specific methods defer to runtime dispatch instead of a wrong
   static bind.

The distinction is **not** carried on `ArrayElementType` itself (the
bincode-serialized storage tag used across ~50 files and embedded in
`Instr` payloads / the persistent Base cache — adding a marker there would
leak into runtime storage and cache compatibility) nor on `ValueType`'s
`ArrayOf` tuple shape (32+ pattern-match sites). Instead, the compiler tracks
provenance in a dedicated, non-serialized, per-scope side table,
`known_any_rank_array_locals: HashSet<String>` (`compile/core_compiler.rs`),
scoped identically to `julia_type_locals` (saved/restored/cleared at the same
call sites in `stmt.rs`/`unary.rs`/`expr/mod.rs`). It is populated
**conservatively**: only a proven-`Any` producer (currently: assigning
`expr.args`) marks a variable; every other `ArrayOf(Any, Some(n))` producer
(comprehensions) is left unmarked and keeps the safe "unknown, defer to
runtime dispatch" bridge behavior by default. A future array producer that
forgets to opt in defaults to the conservative treatment, not a silent wrong
static bind — this is the mechanism that prevents `elem_unknown`-style ad-hoc
heuristics from proliferating as new array producers are added.

The dispatch-resolution fallback that defers a rank-known/element-unknown
bare-alias argument to runtime dispatch (`is_rank_unknown_array_julia_type` in
`compile/expr/call/mod.rs`, consumed by the `Err(NoMethodFound)` guard in
`compile/expr/call/dispatch.rs`) originally recognized only *abstract*
array-family candidate methods (`::AbstractVector`/`::AbstractMatrix`,
Issue #7266); `core_is_array_family_type` (Issue #10206) broadens this to
*concrete* `Array`/`Vector`/`Matrix` candidates too, so a multi-iterator
comprehension's bare `Matrix` argument against a candidate typed
`::Array{Any,2}` defers to runtime dispatch (which resolves correctly) instead
of raising a spurious compile-time `MethodError`.

### 3. JuliaType (User-facing)

Located in: `subset_julia_vm/src/types/julia_type/mod.rs`

`JuliaType` represents Julia types as users see them (e.g., in `typeof()` results):

```rust
pub enum JuliaType {
    Int64, Float64, Bool, String, ...
    Array { element: Box<JuliaType>, ndims: usize },
    Struct(String),
    Union(Vec<JuliaType>),
    ...
}
```

### 4. Value (Runtime values)

Located in: `subset_julia_vm_vm/src/vm/value/value_enum.rs`

`Value` is the main runtime value enum. Every Julia value at runtime is represented as a `Value` variant. There are currently **53 variants** (verified by compile-time exhaustiveness test `test_all_value_variants_constructed`):

| Category | Variants | Count |
|----------|----------|-------|
| Signed integers | `I8(i8)`, `I16(i16)`, `I32(i32)`, `I64(i64)`, `I128(i128)`, `BigInt(RustBigInt)` | 6 |
| Unsigned integers | `U8(u8)`, `U16(u16)`, `U32(u32)`, `U64(u64)`, `U128(u128)` | 5 |
| Boolean | `Bool(bool)` | 1 |
| Floating point | `F16(f16)`, `F32(f32)`, `F64(f64)`, `BigFloat(RustBigFloat)` | 4 |
| String types | `Str(String)`, `Char(char)` | 2 |
| Singletons / sentinels | `Nothing`, `Missing`, `Undef`, `SliceAll` | 4 |
| Collections / storage carriers | `ExprArgs(ArrayRef)`, `Memory(MemoryRef)`, `MemoryRef(Box<MemoryRefValue>)`, `Range(RangeValue)`, `Tuple(TupleValue)`, `SimpleVector(TupleValue)`, `NamedTuple(NamedTupleValue)`, `Pairs(PairsValue)`, `Ref(RefCellRef)`, `Generator(Box<GeneratorValue>)`, `StaticArray(Box<StaticRealValue>)`, `StaticArrayInline(StaticArrayInlineData)` | 12 |
| Struct types | `Struct(StructInstance)`, `StructRef(usize)` | 2 |
| RNG | `Rng(RngInstance)` | 1 |
| Type/Module | `DataType(JuliaType)`, `RuntimeTypeVar(Box<RuntimeTypeVarValue>)`, `RuntimeTypeName(Box<RuntimeTypeNameValue>)`, `Module(Box<ModuleValue>)` | 4 |
| Callable | `Function(FunctionValue)`, `Closure(ClosureValue)`, `ComposedFunction(ComposedFunctionValue)` | 3 |
| IO | `IO(IORef)` | 1 |
| Macro system | `Symbol(SymbolValue)`, `Expr(ExprValue)`, `QuoteNode(Box<Value>)`, `LineNumberNode(LineNumberNodeValue)`, `GlobalRef(GlobalRefValue)` | 5 |
| Regex | `Regex(RegexValue)`, `RegexMatch(Box<RegexMatchValue>)` | 2 |
| Enum | `Enum { type_name, value }` | 1 |

#### Memory(MemoryRef)

`Value::Memory` represents Julia's `Memory{T}` type, a flat typed memory buffer introduced in Julia 1.11+. Unlike the native-array compatibility carrier, `Memory` has no shape or dimensions -- it is a 1D mutable buffer with a known element type. It serves as the low-level storage backend underlying `Vector` in Julia 1.11+.

At the `ValueType` level, `Memory` maps to either `ValueType::Memory` (element type unknown) or `ValueType::MemoryOf(ArrayElementType)` (element type known at compile time).

## Type Conversion Flow

```
Julia Source Code
       │
       ▼
   [Parsing]
       │
       ▼
   Core IR (type annotations from source)
       │
       ▼
   [Type Inference]
       │
       ▼
   LatticeType (precise compile-time types)
       │
       ▼
   [Code Generation]
       │
       ▼
   ValueType (runtime type tags)
       │
       ▼
   [Execution]
       │
       ▼
   JuliaType (user-visible types via typeof())
```

## Type Representation Conversion Inventory (Issue #5916)

> **Full inventory**: the complete per-conversion map (every `From` impl /
> helper with `file:line`, lossiness flags, dead/disagreeing/string-round-trip
> findings, and the phased migration roadmap) lives in
> [TYPE_REPRESENTATIONS.md](./TYPE_REPRESENTATIONS.md). The table below is the
> coarse summary.

The current system still has multiple type representations because each stage
needs a different projection. `CoreType` is the semantic convergence target, but
not every conversion can become lossless until user-defined registry metadata,
world visibility, and backend layout constraints are carried through the same
path. New shared type operations should prefer `CoreType` first and convert to
`ValueType` or `StaticType` only at codegen/backend boundaries.

Where-clause and `UnionAll` scoping is the companion problem to representation
conversion: before a type can be preserved structurally, each bare or qualified
name must be resolved in the correct lexical binder environment. The target
model and migration plan live in
[WHERE_BINDER_ENVIRONMENT.md](./WHERE_BINDER_ENVIRONMENT.md) (Issue #10436).

| From | To | Main entry points | Current role | Precision / loss notes |
|------|----|-------------------|--------------|------------------------|
| `JuliaType` | `CoreType` | `CoreType::from(&JuliaType)`, `CoreType::from_julia_name()` | Shared semantic queries for runtime, dispatch, reflection, and compiler helpers | Opaque string-like `Struct("Foo{...}")` values are parsed best-effort. User-defined hierarchy still needs registry/context metadata. |
| `CoreType` | `JuliaType` | `CoreType::to_julia_name()`, `JuliaType::from_name_or_struct()`, runtime type-object registry projections | User-visible type objects and legacy callers that still require `JuliaType` | Structured `UnionAll`, `TypeVar`, and value parameters often round-trip through names; parent/registry metadata is not carried by the name alone. |
| `JuliaType` | `ValueType` | `julia_type_to_value_type()`, `julia_type_to_value_type_with_table()`, `julia_type_to_value_type_with_ctx()` | Codegen slot and VM instruction type tags | Abstract numeric families and many parametric forms collapse to concrete or generic VM carriers; element/type-parameter detail is preserved only when side metadata is available. |
| `ValueType` | `LatticeType` | `impl From<&ValueType> for LatticeType`, `value_type_to_lattice_with_struct_table()` | Legacy pre-scan and bridge path from VM tags back into inference facts | Runtime tags do not contain full Julia type structure; struct tables can recover some names but not full parametric/world context. |
| `LatticeType` / `ConcreteType` | `CoreType` | `impl From<&LatticeType> for CoreType`, `impl From<&ConcreteType> for CoreType` | Shared subtype, join, and classification queries from inference/lattice code | Lattice-only facts such as `Top`, `Conditional`, provenance, and limited constants must widen to broader core shapes. |
| `LatticeType` / `ConcreteType` | `JuliaType` | `lattice_to_julia_type()`, `lattice_to_parametric_julia_type()`, `concrete_type_to_julia_type()` | Reflection return types, `Base.return_types`, and legacy dispatch metadata | Flow-sensitive annotations and some lattice provenance do not have a `JuliaType` carrier; world-specific method visibility is not represented. |
| `LatticeType` | `ValueType` | `lattice_to_value_type()`, bridge conversions in `compile/bridge.rs` | Codegen locals, stack slots, and instruction selection | Unions, parametric structs, and abstract families may collapse to `Any` or generic carriers when the VM tag set cannot represent the shape. |
| `TypeExpr` | `JuliaType` | `TypeExpr::to_julia_type_lossy()`, `TypeExpr::substitute_to_julia_type_lossy()`, `StructField::as_julia_type()` | Field/default/type-alias projection into concrete type metadata | `TypeVar`, `UnionAll`, and unresolved parametric expressions cannot always become a concrete `JuliaType`; callers may widen to `Any` or preserve unresolved names as struct placeholders. |
| `TypeExpr` / `JuliaType` | `StaticType` | `StaticType::from(&JuliaType)`, AoT inference `TypeExpr::Concrete(jt) => StaticType::from(jt)` | AoT backend projection and static layout selection | Unsupported `UnionAll`, `TypeVar`, and value-parameter shapes are widened or rejected when the backend has no direct representation. |
| `CoreType` | `StaticType` | `StaticType::from_core_type_lossy()`, `StaticType::core_typejoin()`, `StaticType::core_typeintersect()` | AoT joins/intersections through the shared semantic core, then projection back to backend types | Returns `None` or widens when the semantic result has no backend/codegen representation. |

Immediate reduction targets:

- Keep `CoreType` as the owner for shared type semantics (`<:`,
  `typeintersect`, `typejoin`, specificity, reflection facts).
- Keep `ValueType` and `StaticType` as backend/codegen projections rather than
  semantic owners.
- When adding a new type form, update this table and state whether the
  conversion is lossless, lossy, context-dependent, or backend-only.
- Legacy fallbacks should be kept only where runtime registry data, user-defined
  hierarchy, or world-specific visibility is still unavailable to `CoreType`.

### Call-dependent type-object returns (Issue #10133)

`MethodSig.return_julia_type` is a persisted, call-independent summary. It can
represent a direct `Type{T}` return and substitute `T` at dispatch, but a
type-level branch such as
`T === BigInt ? BigFloat : Float64` is summarized before `T` is bound as an
erased `Union{DataType,DataType}`. Converting that summary to the inference
lattice yields `Top`; treating it as final loses the distinction between the
two type-object values.

For overloaded functions, the name-only `function_table` intentionally drops
all bodies rather than guessing an overload. Method-table dispatch, however,
selects an exact `MethodSig` and its `global_index`. The inference engine keeps
the exact body only for methods whose return snapshot is a union composed
entirely of type-object shapes. When that selected snapshot widens to `Top`, it
analyzes the retained body with the call-site argument/type-parameter bindings.
Ordinary `Any`/`Top` returns do not retain or re-walk bodies; widening remains
the conservative default. This preserves structural dispatch ownership while
avoiding both a `float`-specific rule and a broad inference-cost regression.

Coverage: `reflection_infer_float_type_ternary_10281` checks the direct query
and wrapper-call forms for the `Int64` and `BigInt` branches, plus the abstract
type-parameter no-false-fold guard.

### Receiver-sensitive indexed-result inference (Issue #10887)

Index shape alone never proves result shape. Receiver family and index shape
are independent inference inputs. Recognized array-family receivers may retain
known element and rank information in the `JuliaType` slice projection; the
`ValueType` projection retains only its coarser Array result. It also has a
limited Range preservation for a range receiver on the slice-shaped path, which
does not prove whether integer-vector or Bool-vector indexing returns a Range
or an Array. Range, tuple, custom, or unknown result shapes not established by
the relevant channel stay conservative, and runtime `getindex` dispatch is
authoritative because custom methods may use arrays or ranges as scalar keys.

The static-channel gates are
`index_slice_value_type_is_receiver_sensitive_issue_10887`,
`index_slice_julia_type_is_receiver_sensitive_issue_10887`. The 61-assertion
`type_inference_index_receiver_shape_matrix_10887` fixture separately covers
runtime value/`typeof` parity and equality/dispatch consumers; it is not proof
of precise static projections for every receiver family.

### Semantic distinctions must not be flattened (Issue #10245)

Inference facts, Julia values, existential type structure, effects, and
display strings are different semantic layers. A representation may project
into a less precise layer only when it retains provenance or defaults to a
conservative result; the projection must never be treated as evidence for a
more precise Julia-visible fact.

The closure audit for #10245 maps each previously conflated distinction to one
owner and an executable regression gate:

| Distinction | Owner / invariant | Regression coverage |
|---|---|---|
| Concrete `Array{Any,N}` vs inference-unknown element (#10206) | `known_any_rank_array_locals` is a non-serialized, scope-local provenance table; unmarked `ArrayOf(Any,Some(n))` defaults to bare-family/runtime dispatch | `dispatch_known_any_field_array_family_10206`, `dispatch_comprehension_concrete_array_family_10206` |
| Partial `UnionAll` vs concrete `DataType` (#10192) | `Core.apply_type` preserves remaining binders and already-applied prefixes; it does not materialize a concrete type before full application | `types_apply_type_partial_unionall_10192` |
| Runtime `UnionAll` base vs static type spelling (#10191) | `ApplyTypeDynamicSplat` dispatches on the runtime type object and applies the flattened argument sequence with bound/arity checks | `types_apply_type_dynamic_splat_10191`, `apply_type_dynamic_splat_expands_and_instantiates_unionall_issue_10191` |
| Control-flow-local effect vs whole-method purity (#10145) | reflection and optimization share the same effect walker and join every branch/loop; method registration populations are identical | `reflection_infer_effects_control_flow_10145` |
| Type-object value vs generic `DataType` class (#10133) | direct `Type{T}` snapshots substitute `T`; erased type-object branches re-enter only the exact method-table winner's body by `global_index` | `reflection_infer_float_type_ternary_10281`, `issue_10133_method_body_retention_is_narrowly_type_object_gated` |
| Semantic `UnionAll` vs its display projection (#10195) | trailing free binders may be elided only by `show_can_elide`-equivalent checks; equality/subtyping never depend on a lossy display shortcut | `prefix_partial_unionall_prints_like_upstream_issue_10192`, `diagonal_unionall_does_not_print_as_partial_application_issue_10635` |
| Lattice relation vs container/name heuristics (#10049) | `CoreType` owns subtype/join/intersection; backend and display forms are projections | `scripts/check_type_application_matrix.sh`, `scripts/check_no_typevar_name_heuristic.sh`, and the #10049 acceptance fixture matrix in [SUBTYPING.md](./SUBTYPING.md) |

Rules for future additions:

1. Never use analysis `Top`/unknown as proof that a Julia value is concretely
   `Any`; carry provenance or defer.
2. Preserve `UnionAll`/`TypeVar` structure and binder identity until the
   operation that legitimately instantiates it.
3. Join effects over the complete control-flow graph and expose the same
   summary to reflection and optimization.
4. Treat rendering as a final projection. Printed names must not become the
   semantic owner for dispatch, equality, or subtyping.
5. If a persisted summary loses call dependence, re-enter a richer
   representation through exact method identity; never select by a bare name.

The broader owner-scoped identity migration remains tracked independently by
#10279; it is not a correctness blocker for these now-gated distinctions.

## Type Correspondence Table

| Julia Type | LatticeType | ValueType | Notes |
|------------|-------------|-----------|-------|
| `Int64` | `Concrete(Int64)` | `I64` | |
| `Int32` | `Concrete(Int32)` | `I32` | |
| `UInt64` | `Concrete(UInt64)` | `U64` | |
| `Float64` | `Concrete(Float64)` | `F64` | |
| `Float32` | `Concrete(Float32)` | `F32` | |
| `BigInt` | `Concrete(BigInt)` | `BigInt` | Arbitrary precision |
| `BigFloat` | `Concrete(BigFloat)` | `BigFloat` | Arbitrary precision |
| `Bool` | `Concrete(Bool)` | `Bool` | |
| `String` | `Concrete(String)` | `Str` | |
| `Char` | `Concrete(Char)` | `Char` | 32-bit Unicode codepoint |
| `Nothing` | `Concrete(Nothing)` | `Nothing` | |
| `Missing` | `Concrete(Missing)` | `Missing` | |
| `Vector{Int64}` | `Concrete(Array{Int64})` | `ArrayOf(I64)` | |
| `Memory{Float64}` | N/A | `MemoryOf(F64)` | Julia 1.11+ low-level storage |
| `Dict{K,V}` | `Concrete(Dict{K,V})` | `Dict` | Parametric via Pure Julia struct (Milestone 9) |
| `Set{T}` | `Concrete(Set{T})` | `Set` | |
| `Union{Int64,Float64}` | `Union({Int64,Float64})` | `Union([I64,F64])` | Preserved! |
| Unknown | `Top` | `Any` | |

## Dict{K,V} Parametric Type Support (Milestone 9)

Pure Julia `Dict{K,V}` is now implemented as a `mutable struct Dict{K,V} <: AbstractDict{K,V}`. The system uses a **dual dispatch** approach where Rust-backed `Value::Dict` and Pure Julia `Dict{K,V}` struct coexist:

| Aspect | Rust-backed `Value::Dict` | Pure Julia `Dict{K,V}` struct |
|--------|---------------------------|-------------------------------|
| Construction | `Dict()` / `Dict{K,V}()` with pair/empty args | struct constructor (non-pair arguments) |
| Runtime value | `Value::Dict(DictRef)` | `Value::Struct(StructInstance)` |
| Dispatch | `::Dict` bare annotation | `::Dict{K,V} where {K,V}` annotation |

At the `ConcreteType` level, `Dict{key, value}` is parametric, allowing compile-time type tracking. The lowering stage supports `AbstractDict{K,V}` as a parametric abstract type.

A dispatch guard (`is_rust_dict_parametric_mismatch()`) in all `CallDynamic*` handlers prevents Rust-backed `Value::Dict` from incorrectly matching parametric `Dict{K,V}` method signatures.

See `docs/vm/DONE.md` (Milestone 9) and `docs/vm/STATUS.md` for full details on the implementation phases and issue references.

## Union Type Handling

### Issue #1635/#1676: Union Type Preservation

Previously, Union types were collapsed to `Any` during code generation because `ValueType` lacked a `Union` variant. This caused type information loss.

**Solution**: Added `ValueType::Union(Vec<ValueType>)` variant that preserves Union type information through code generation.

### Conversions in bridge.rs

The `compile/bridge.rs` module provides bidirectional conversions:

```rust
// LatticeType → ValueType
impl From<&LatticeType> for ValueType {
    fn from(lattice_type: &LatticeType) -> Self {
        match lattice_type {
            LatticeType::Union(types) => {
                // Preserve Union type information
                let value_types: Vec<ValueType> = types
                    .iter()
                    .map(|ct| ValueType::from(&LatticeType::Concrete(ct.clone())))
                    .collect();
                ValueType::Union(value_types)
            }
            // ... other cases
        }
    }
}

// ValueType → LatticeType
impl From<&ValueType> for LatticeType {
    fn from(value_type: &ValueType) -> Self {
        match value_type {
            ValueType::Union(types) => {
                // Convert back to LatticeType::Union
                let concrete_types: Vec<ConcreteType> = types
                    .iter()
                    .filter_map(|vt| match LatticeType::from(vt) {
                        LatticeType::Concrete(ct) => Some(ct),
                        _ => None,
                    })
                    .collect();
                LatticeType::Union(concrete_types.into_iter().collect())
            }
            // ... other cases
        }
    }
}
```

## Testing Type Preservation

### Unit Tests

Located in: `subset_julia_vm_compile/src/compile/bridge.rs`

```bash
# Run bridge conversion tests
timeout 1800 cargo nextest run --release --lib compile::bridge::tests
```

Key tests:
- `test_latticetype_to_valuetype_union` - Verifies Union preservation
- `test_all_valuetype_variants_to_lattice` - Exhaustive variant coverage

### Fixture Tests

Located in: `subset_julia_vm/tests/fixtures/type_inference/`

- `union_types.jl` - Basic Union type inference
- `union_preservation.jl` - Union preservation through codegen (Issue #1682)

```bash
# Run type inference fixture tests
timeout 1800 cargo nextest run --release --test fixture_tests type_inference
```

## Adding New Types

When adding a new type to the VM:

1. **Add to `ValueType`** in `vm/value/value_enum.rs`
2. **Update bridge conversions** in `compile/bridge.rs`:
   - `From<&ValueType> for LatticeType`
   - `From<&LatticeType> for ValueType`
3. **Update code generation** in `compile/`:
   - `stmt.rs` (return/store instructions)
   - `expr/mod.rs` (type conversions)
   - `expr/infer.rs` (type inference)
4. **Update reflection** in `vm/builtins_reflection/`
5. **Add tests** to verify round-trip conversions
6. **Document** in this file's correspondence table

## Adding New ValueType Variants

When adding a new variant to the `ValueType` enum in `subset_julia_vm_vm/src/vm/value/value_enum.rs`, you **MUST** update all match statements throughout the codebase. Failure to do so will cause compilation errors or runtime failures.

### Required Updates Checklist

- [ ] Add the variant to `src/vm/value/value_enum.rs`
- [ ] Update `compile/bridge.rs`:
  - [ ] `impl From<&ValueType> for LatticeType` (ValueType -> LatticeType conversion)
  - [ ] `impl From<&LatticeType> for ValueType` (LatticeType -> ValueType conversion)
  - [ ] Add the variant to `test_all_valuetype_variants_to_lattice` test
- [ ] Update `compile/stmt.rs`:
  - [ ] `emit_return_for_type` function (Return instructions)
  - [ ] Store instruction match in `Stmt::While` handling
  - [ ] Return instruction match in `Stmt::Return` handling
- [ ] Update `compile/expr/infer/`:
  - [ ] `infer_literal_type` function
  - [ ] `value_type_to_julia_type` function
- [ ] Update `compile/expr/mod.rs`:
  - [ ] Type conversion rules in `compile_expr_as` (see Type Conversion Rules below)
- [ ] Update `vm/builtins_reflection/`:
  - [ ] `value_type_to_julia_type` function
- [ ] Update promotion rules if the new variant participates in numeric promotion:
  - [ ] Add the Julia-facing rule in `src/julia/base/promotion.jl` when it is a real Julia promotion rule
  - [ ] Update the shared fallback in `src/promotion.rs` only for bootstrapping/cache-miss coverage
  - [ ] Add promotion tests for `promote_type`, `promote`, and the relevant arithmetic path
- [ ] Update `vm/convert.rs` (Issue #2267):
  - [ ] Add a `convert_to_<type>()` function handling all numeric source Value variants
  - [ ] Add a dispatch arm in `convert_value()` for the new type name string
  - [ ] Add the new type to the `test_convert_handler_completeness` unit test iteration list
- [ ] Update `vm/stack_ops.rs` (Issue #2218):
  - [ ] `pop_f64_or_i64()` — add the new Value variant arm to convert to f64
- [ ] Update `vm/exec/conversion.rs` (Issue #2218):
  - [ ] `ToF64` instruction handler — add the new Value variant
  - [ ] `ToI64` instruction handler — add the new Value variant
  - [ ] `DynamicToF64` / `DynamicToI64` handlers — add the new Value variant
- [ ] Update `vm/exec/array_index.rs` (Issue #2218):
  - [ ] `IndexStore` typed value conversion — add the new ArrayData variant
- [ ] Update `bin/sjulia.rs` (Issue #1736):
  - [ ] `format_value_with_vm()` function — REPL result formatting
  - [ ] Match arms in `run_file()` and `run_code()` return value handling
- [ ] Update `vm/formatting.rs` (Issue #1736):
  - [ ] `format_value()` function — Value display formatting
  - [ ] `value_to_string()` function — Value-to-string conversion
- [ ] Update `ffi/format.rs` (Issue #1736):
  - [ ] `format_value()` function — FFI value formatting for iOS/Swift integration
- [ ] Update `ffi/basic.rs` (Issue #1736):
  - [ ] All functions returning or formatting Value results (e.g., `run_ir_json_f64_N_seed`)
- [ ] Add the variant to `test_all_value_variants_constructed` test in `vm/value/value_enum.rs`
- [ ] Run `cargo build` to find any remaining match statements
- [ ] Run `timeout 1800 cargo nextest run --release` to verify all tests pass

### Quick Check Command

After adding a new variant, run:
```bash
cargo build 2>&1 | rg "non-exhaustive patterns"
```
This will show any match statements that need updating.

## Fast-Path Guard Pattern: Whitelist vs Blocklist (Issue #2257)

When implementing fast-path optimizations that bypass Julia fallback code, use **whitelist** patterns instead of **blocklist** patterns to ensure type safety.

### The Problem with Blocklists

Blocklist guards list types that need special handling and let everything else through:

```rust
// ANTI-PATTERN: Blocklist - new variants silently pass through
fn needs_special_handling(value: &Value) -> bool {
    matches!(value, Value::F32(_) | Value::F16(_) | Value::Struct(_))
    // New variants NOT in this list will fall through to fast path
    // even if the fast path doesn't handle them correctly!
}
```

When a new `Value` variant is added (e.g., `Value::BigInt`), it's NOT in the blocklist, so it passes through to a fast path that doesn't know how to handle it. The fast path may:
- Silently return incorrect results (`other => other` catch-alls)
- Cause runtime panics
- Produce subtle type promotion bugs

### The Solution: Whitelist Pattern

Whitelist guards explicitly list types that the fast path can handle:

```rust
// CORRECT: Whitelist - new variants are rejected by default
fn can_use_fast_path(value: &Value) -> bool {
    matches!(value, Value::I64(_) | Value::F64(_) | Value::Bool(_))
    // New variants NOT in this list will use the Julia fallback
    // which is the safe default!
}
```

With a whitelist, new variants are automatically excluded from the fast path, ensuring they use the correct Julia fallback.

### Current VM Dynamic Arithmetic Boundary

The old `needs_julia_promote()` / `promote_hardcoded()` VM guard was removed. Mixed-type primitive arithmetic now tries the Julia `promotion.jl` dispatch path first; only clearly bounded inline cases stay on the VM fast path.

The active guard is `should_use_inline_dynamic_op()` in `vm/dynamic_ops/dispatch.rs`:

```rust
if matches!(
    (a, b),
    (Value::I64(_), Value::I64(_))
        | (Value::F64(_), Value::F64(_))
        | (Value::F32(_), Value::F32(_))
        | (Value::F16(_), Value::F16(_))
        | (Value::Bool(_), Value::Bool(_))
) {
    return true;
}
```

That whitelist is intentionally narrow: same-type primitives, selected literal-like values, BigInt, and supported array-like carriers can use inline VM dynamic operations; mixed primitive pairs, Complex, and Rational route through Julia dispatch or the shared binary resolver.

### Catch-All Arms: `unreachable!` vs `other => other`

Fast paths protected by a whitelist guard should use `unreachable!` for unexpected types:

```rust
// ANTI-PATTERN: Silent pass-through
match value {
    Value::I64(n) => Value::F64(n as f64),
    Value::Bool(b) => Value::F64(if b { 1.0 } else { 0.0 }),
    other => other,  // Silently returns wrong type!
}

// CORRECT: Fail-fast with unreachable!
match value {
    Value::I64(n) => Value::F64(n as f64),
    Value::Bool(b) => Value::F64(if b { 1.0 } else { 0.0 }),
    // Safety: should_use_inline_dynamic_op() must be called first to ensure
    // only whitelisted types reach this path
    other => unreachable!("fast_path received unexpected type: {:?}", other),
}
```

The `unreachable!` assertion turns silent bugs into immediate, diagnosable failures.

### Checklist for Fast-Path Guards

When adding or modifying fast-path optimizations:

- [ ] Use **whitelist** pattern (explicitly list types the fast path handles)
- [ ] Document which types are whitelisted and why
- [ ] Replace `other => other` catch-alls with `unreachable!` in protected code paths
- [ ] Add a comment referencing the guard function (e.g., "must call `should_use_inline_dynamic_op()` first")
- [ ] When adding new `Value` variants, verify the whitelist still makes sense

### Audit Command

Find potential blocklist guards and catch-all arms:

```bash
# Find VM fast-path guards and dynamic arithmetic boundaries
rg -n "should_use_inline_dynamic_op|inline_dynamic|fast_path|PrimitiveFallbackFirst" subset_julia_vm_vm/src/vm -g '*.rs'

# Find pass-through catch-all arms that need context review
rg -n "other => other" subset_julia_vm_vm/src/vm -g '*.rs'
```

## Specificity Scoring Rules (Issue #2302, #2321)

The `JuliaType::specificity()` function in `subset_julia_vm/src/types/julia_type/comparison.rs` returns a score used for method dispatch. Higher scores indicate more specific types, and more specific methods are preferred during dispatch.

### Scoring Hierarchy

| Type Category | Score | Examples |
|--------------|-------|----------|
| Any | 0 | `Any` |
| Abstract type groups | 1 | `Number`, `AbstractString`, `Function` |
| Mid-level abstract | 2 | `Real` |
| More specific abstract | 3 | `Integer`, `AbstractFloat` |
| Most specific abstract | 4 | `Signed`, `Unsigned` |
| Concrete types | 5 | `Int64`, `Float64`, `String`, `Bool` |

### Parametric Type Scoring

Parametric types like `TupleOf`, `VectorOf`, and `MatrixOf` use **element-type-based** scoring, not flat concrete scores.

**TupleOf scoring** (element-wise sum):

| Type | Score Calculation | Result |
|------|-------------------|--------|
| `Tuple{Int64, Int64}` | 5 + 5 | 10 |
| `Tuple{Int64, Any}` | 5 + 0 | 5 |
| `Tuple{Any, Int64}` | 0 + 5 | 5 |
| `Tuple{Any, Any}` | 0 + 0 | 0 |
| `Tuple{Int64, Real}` | 5 + 2 | 7 |
| `Tuple{}` (empty) | (special case) | 5 |

**VectorOf/MatrixOf scoring** (single element type, Issue #2352):

| Type | Element Specificity | Result |
|------|---------------------|--------|
| `Vector{Int64}` | 5 (concrete) | 5 |
| `Vector{Real}` | 2 (abstract) | 2 |
| `Vector{Number}` | 1 (abstract) | 1 |
| `Vector{Any}` | 0 (most general) | 0 |

This ensures correct dispatch ordering:
- `Tuple{Int64, Int64}` (score 10) > `Tuple{Int64, Any}` (score 5) > `Tuple{Any, Any}` (score 0)
- `Vector{Int64}` (score 5) > `Vector{Real}` (score 2) > `Vector{Any}` (score 0)

### Historical Bugs (Issue #2302)

Three scoring approaches were tried for TupleOf:

1. **Flat score (original bug)**: All TupleOf scored 5, making `Tuple{Int64, Int64}` and `Tuple{Any, Any}` equal → `AmbiguousMethod` errors
2. **min() of elements (first fix bug)**: `Tuple{Int64, Any}` scored same as `Tuple{Any, Any}` → incorrect dispatch
3. **sum() of elements (correct)**: Preserves monotonic ordering for all partial specificity cases

### Adding New Parametric Types

When adding a new parametric type variant (e.g., `VectorOf`, `MatrixOf`):

1. **Never group with flat concrete types** in the `specificity()` match arm
2. **Use element-wise sum** for the specificity calculation
3. **Add unit tests** in `types.rs` verifying ordering invariants
4. **Add fixture tests** covering dispatch with varying element types

### Overflow Safety

Using `u8` for specificity scores, the sum approach is safe for tuples up to 51 elements (51 × 5 = 255). Longer tuples would need `u16` or saturating addition.

## Type Conversion Rules

The `compile_expr_as` function in `subset_julia_vm_compile/src/compile/expr/mod.rs` handles type conversions at compile time.

### Required Conversions for New Numeric Types

For any new numeric type `T`, ensure these conversion paths exist:

1. **To Any**: `(T, Any)` - Usually no conversion needed
2. **To wider types**: `(T, F64)`, `(T, I64)` - Use `DynamicToF64` or `DynamicToI64`
3. **To narrower types**: `(I64, T)`, `(F64, T)` - Use appropriate `DynamicToT` instruction

### F32/F16 Conversion Checklist (Issue #1695)

**Required conversions TO Float32/Float16:**
- [ ] `Int64 -> Float32` (via `DynamicToF32`)
- [ ] `Float64 -> Float32` (via `DynamicToF32`)
- [ ] `Int32 -> Float32` (via `DynamicToF32`)
- [ ] `Float32 -> Float32` (identity)
- [ ] All other integer types (`I8`, `I16`, `I128`, `U8`, `U16`, `U32`, `U64`, `U128`)

**Required conversions FROM Float32/Float16:**
- [ ] `Float32 -> Float64` (widening, via `DynamicToF64`)
- [ ] `Float32 -> Int64` (truncation, via `DynamicToI64`)

**Test scenarios:**
1. Struct field assignment: `struct S; x::Float32; end; S(1.0)`
2. Explicit conversion: `Float32(x)` where x is Int64/Float64
3. Function return type coercion
4. Array element conversion
5. Complex{Float32} construction

### Union -> Numeric Type Conversion Completeness (Issue #1771, #1774)

When expressions have Union types (e.g., from if/elseif/else branches), the compiler needs conversion paths to all numeric types. Missing conversion paths cause "Cannot convert Union([...]) to T" errors.

**Required Union conversions in `compile/expr/mod.rs`:**
- [x] `Union -> Bool` (via `DynamicToBool`)
- [x] `Union -> I64` (via `DynamicToI64`)
- [x] `Union -> F64` (via `DynamicToF64`)
- [x] `Union -> F32` (via `DynamicToF32`) - Added in Issue #1771
- [x] `Union -> F16` (via `DynamicToF16`) - Added in Issue #1851

**Code review checklist for type conversions:**
- [ ] When adding type conversion for `Union -> T`, ensure ALL numeric types are covered
- [ ] When modifying transfer functions (tfuncs), verify all valid arities are handled (unary, binary, n-ary)
- [ ] When adding new operator methods in Julia, test with both same-type and mixed-type operands

**Transfer function arity handling:**

Transfer functions like `tfunc_sub` must handle all valid arities. For example, the `-` operator can be:
- Unary negation: `-x` (1 argument) - preserves the operand type
- Binary subtraction: `x - y` (2 arguments) - follows promotion rules

If a transfer function only handles binary operations, unary operations will return `LatticeType::Top`, causing type inference failures for methods like `-x::Float32`.

## Parametric Type Preservation (Issue #2317, #2323)

### Problem: Type Information Loss Through Function Calls

When a function returns a tuple like `(1, 2)`, the VM needs to preserve the parametric type information `Tuple{Int64, Int64}` so that method dispatch works correctly on the returned value.

**Example scenario:**

```julia
function tuple_sum(t::Tuple{Int64, Int64})
    return t[1] + t[2]
end

function make_pair()
    return (1, 2)
end

function wrap_pair()
    return make_pair()
end

# These must all dispatch to tuple_sum correctly:
tuple_sum((1, 2))           # Direct tuple literal
tuple_sum(make_pair())      # Single function call
tuple_sum(wrap_pair())      # Chained function calls
```

### ValueType Conversion Can Lose Type Parameters

The `LatticeType → ValueType` conversion in `compile/bridge.rs` can lose information:

| LatticeType | ValueType | Information Lost |
|-------------|-----------|------------------|
| `TupleOf([Int64, Int64])` | `TupleOf([I64, I64])` | ✓ Preserved |
| `TupleOf([Any, Any])` | `Tuple` | Parametric info lost |
| `Unknown` | `Any` | All type info lost |

### Runtime Type Preservation

To ensure parametric types work through function call chains:

1. **Tuple values carry element types**: The `TupleValue` struct stores actual element `Value` variants, from which parametric types can be recovered at runtime.

2. **Return value type inference**: When compiling a call to a function that returns a tuple, the compiler uses the function's return type (if known) rather than only the call site context.

3. **Dynamic dispatch fallback**: When static type info is insufficient, the VM extracts parametric types from actual values at runtime.

### Fixture Tests

Prevention tests for type preservation:

- `subset_julia_vm/tests/fixtures/tuple/function_return_dispatch.jl` - Single-level function returns (Issue #2317)
- `subset_julia_vm/tests/fixtures/tuple/chained_return_dispatch.jl` - Multi-level chained function returns (Issue #2323)
- `subset_julia_vm/tests/fixtures/tuple/parametric_dispatch.jl` - General parametric tuple dispatch

### Debugging Type Preservation Issues

If parametric dispatch fails on returned values:

1. **Check return type inference**: Verify the function's return type is being inferred as `TupleOf([...])` not just `Tuple`
2. **Check ValueType conversion**: Look for `Tuple` (non-parametric) where `TupleOf([...])` is expected
3. **Check runtime extraction**: Verify `Value::Tuple` elements are being used to reconstruct parametric types

## Parametric Type Handler Checklist (Issue #2304, #2316)

When adding or modifying parametric types (`TupleOf`, `VectorOf`, `MatrixOf`, etc.), **three functions must stay in sync**:

### Required Functions

| Function | Location | Purpose |
|----------|----------|---------|
| `is_subtype_of()` | `types/julia_type/comparison.rs` | Basic subtype checking |
| `is_subtype_of_parametric()` | `types/julia_type/comparison.rs` | Dispatch-time subtype checking with where-clause type variables |
| `extract_type_bindings()` | `types/julia_type/comparison.rs` | Type variable binding extraction after dispatch |

### Checklist for New Parametric Types

When adding a new parametric type variant to `JuliaType`:

- [ ] Add handler to `is_subtype_of()` with correct variance (covariant for Tuple, invariant for Vector)
- [ ] Add handler to `is_subtype_of_parametric()` for where-clause support
- [ ] Add handler to `extract_type_bindings()` for type variable binding
- [ ] Add tests for both concrete dispatch (`Type{Int64, Int64}`) AND type-variable dispatch (`Type{T, T} where T`)
- [ ] Test bounded type variables (`Type{T, T} where T<:Real`)
- [ ] Test mixed concrete/TypeVar patterns (`Type{Int64, T} where T`)

### Variance Rules

| Type | Variance | Example |
|------|----------|---------|
| `TupleOf` | Covariant | `Tuple{Int64}` <: `Tuple{Number}` |
| `VectorOf` | Invariant | `Vector{Int64}` is NOT <: `Vector{Number}` |
| `MatrixOf` | Invariant | Same as VectorOf |

### Prevention Tests

- `subset_julia_vm/tests/fixtures/tuple/where_clause_binding.jl` - Basic where-clause binding (Issue #2304)
- `subset_julia_vm/tests/fixtures/tuple/bounded_where_clause.jl` - Bounded type variables and mixed patterns (Issue #2316)

## Return-Type Inference for `Type{T}→T`, `abs`/`sign`, and `gcd`/`lcm` (Issues #2245, #2383, #8608)

Functions like `typemin(::Type{T})`, `typemax(::Type{T})`, `zero(::Type{T})`,
`abs`/`abs2`/`sign`, and `gcd`/`lcm` return a value whose concrete type depends
on the *value* of a type argument or on the numeric type of an ordinary
argument (e.g. `typemin(Float64)::Float64`, `abs(x::BigInt)::BigInt`,
`gcd(a::BigInt, b::BigInt)::BigInt`).

**History (resolved).** These functions were once handled by a **per-function,
name-keyed return-type override whitelist**: explicit match arms in
`compile/expr/call/dispatch.rs` and `compile/expr/infer/expr_tfuncs.rs` that
overrode `method.return_type` by function name after dispatch. The whitelist
was originally a workaround for the "Parametric Type Dispatch Limitation"
(Issues #2384/#2388) — inside a `where T` body the compiler could not resolve
`T` to a concrete type, so generic dispatch produced an over-wide (`Any` /
`Float64`) return type.

That limitation **no longer reproduces**. The general transfer-function
registry (`compile/tfuncs/`, e.g. `tfunc_abs`, `tfunc_gcd`, `tfunc_typemin`)
now infers these return types from argument types through the normal inference
machinery, matching upstream Julia. The audit tracked by parent Issue #8608
(sub-issues #8616 / #8617 / #8618) toggled each override off via
`SJULIA_DISABLE_RETURN_OVERRIDE=<id>` and confirmed, against upstream `julia`,
that removing it changed **no** observable return type. All seven overrides
(`abs`/`abs2`/`sign`, `typemin`/`typemax`, `gcd`/`lcm`, and the Complex-math
family from Issue #4341, in both the dispatch and infer sites) were therefore
retired, together with the audit harness and its `SJULIA_DISABLE_RETURN_OVERRIDE`
kill-switch.

**Current rule.** Do **not** add a new name-keyed return-type override arm.
When a `f(::Type{T})→T` or type-preserving numeric function infers too wide a
return type, add or fix its transfer function under `compile/tfuncs/` (the
general mechanism) so *every* call site benefits, rather than special-casing
the name in `dispatch.rs` / `expr_tfuncs.rs`.

### Regression Tests

- `subset_julia_vm/tests/fixtures/type_inference/abs_sign_type_preservation_8617.jl`
  — `abs`/`abs2`/`sign` return types across all integer/float widths, BigInt,
  BigFloat, and Complex, including inside a `where T` helper.
- `subset_julia_vm/tests/fixtures/type_inference/typemin_typemax_preservation_8617.jl`
  — `typemin`/`typemax` across all numeric widths + Bool, and inside `where T`.
- `subset_julia_vm/tests/fixtures/type_inference/gcd_complex_return_types_8618.jl`
  — `gcd`/`lcm` BigInt preservation and Complex-math (`sqrt`/`sin`/…) return
  types, guarding the removal of the last three overrides.
- `subset_julia_vm/tests/fixtures/number/type_param_return_types.jl` — original
  `typemin`/`typemax`/`zero`/`one` coverage (Issue #2245).
- `subset_julia_vm/tests/fixtures/bigint/type_preservation.jl` — comprehensive
  BigInt type preservation.

> **Historical note (Issue #2384/#2388).** Earlier revisions of this document
> recommended working around the parametric-dispatch limitation by calling
> intrinsics directly (e.g. `sdiv_int` instead of `div`) or by adding a
> return-type override. Both recommendations are obsolete: general inference
> now resolves these return types, and no per-function return-type overrides
> remain in the compiler.

## Callable Value Dispatch Sites (Issue #2312)

Callable values (`Value::Function`, `Value::Closure`, `Value::ComposedFunction`) require uniform handling at multiple dispatch sites. When adding or modifying callable value support, **ALL THREE variants must be handled together** to avoid partial-implementation bugs like Issue #2298.

### Callable Value Dispatch Site Inventory

| File | Line(s) | Function/Context | All 3 Handled? |
|------|---------|------------------|----------------|
| `vm/builtins_exec.rs` | ~1482-1492 | `compose()` builtin | ✓ Yes (9 pairs) |
| `vm/exec/call_dynamic.rs` | ~2361-2375 | `flatten_composition()` outer call | ✓ Yes |
| `vm/exec/call_dynamic.rs` | ~2521-2522 | `flatten_composition()` inner extraction | Function+Closure only |
| `vm/exec/call_dynamic.rs` | ~2656-2657 | `flatten_composition()` second path | Function+Closure only |
| `vm/exec/call_dynamic.rs` | ~3062-3095 | `flatten_composition()` recursion | ✓ Yes |
| `vm/exec/return_ops.rs` | ~355-356 | `handle_composed_call_return()` | Function+Closure only |
| `vm/value/value_enum.rs` | ~207-209 | `julia_type()` | ✓ Yes |
| `vm/value/value_enum.rs` | ~271-273 | `value_type()` | ✓ Yes |
| `vm/value/value_enum.rs` | ~526-528 | `is_nothing_or_nothing_type()` | ✓ Yes |
| `vm/util.rs` | ~95-97 | `get_type_name()` | ✓ Yes |
| `vm/type_ops/` | ~1323-1325 | `type_name_for_value()` | ✓ Yes |
| `vm/type_ops/` | ~1681-1695 | `deepcopy_value()` | ✓ Yes |
| `vm/formatting.rs` | ~210-219 | `format_value()` | ✓ Yes |
| `vm/formatting.rs` | ~510-519 | `value_to_string()` | ✓ Yes |
| `vm/builtins_reflection/` | ~902 | `nameof()` | Function only (intentional) |
| `vm/builtins_types.rs` | ~1627 | Type function helper | Function only |
| `vm/exec/locals.rs` | ~468-470 | Local variable type check | ✓ Yes |
| `ffi/format.rs` | ~143-152 | FFI formatting | ✓ Yes |
| `bin/sjulia.rs` | ~1039-1060 | REPL result display | ✓ Yes |
| `bin/sjulia.rs` | ~1830-1874 | `format_value_with_vm()` | ✓ Yes |
| `repl.rs` | ~108-154 | REPL variable storage | ✓ Yes |
| `repl.rs` | ~1129-1133 | REPL expression conversion | Function+ComposedFunction only |

### Categories of Callable Value Sites

1. **Complete sites (handle all 3)**: These are correct and should be used as patterns.
2. **Intentionally incomplete**: Some sites intentionally only handle `Function` (e.g., `nameof()` returns function name, closures/composed have different semantics).
3. **Potentially incomplete**: Sites that only handle Function+Closure or Function+ComposedFunction may be missing cases.

### Code Review Checklist for Callable Values

When modifying code that matches on callable values:

- [ ] Check if ALL THREE variants (`Function`, `Closure`, `ComposedFunction`) need handling
- [ ] If only some variants are handled, add a comment explaining why others are excluded
- [ ] When adding a new callable-related builtin, test with closures AND composed functions, not just named functions
- [ ] Search for `Value::Function` and verify `Closure` is handled where needed

### Audit Command

To find potentially incomplete callable value handling:

```bash
# Find match statements that handle Function but may be missing Closure or ComposedFunction
rg -n 'Value::Function' subset_julia_vm_vm/src/vm -g '*.rs' | \
  rg -v 'ComposedFunction|Closure|test|#\['
```

### Historical Bugs

- **Issue #2298**: `compose()` didn't accept closures because `flatten_composition()` only handled `Function` and `ComposedFunction`
- **Issue #1736**: Similar pattern — `Value::Closure` pattern matching was missing in multiple locations

### Design Pattern: Grouping Callable Variants

When pattern matching on callable values, prefer grouping all three variants together:

```rust
// Good: Explicit grouping with comment
match value {
    Value::Function(_) | Value::Closure(_) | Value::ComposedFunction(_) => {
        // Handle all callable values uniformly
    }
    // ...
}

// Good: Handle each variant when they need different behavior
match value {
    Value::Function(fv) => (fv.name.clone(), None),
    Value::Closure(cv) => (cv.name.clone(), Some(cv.captures.clone())),
    Value::ComposedFunction(cf) => {
        // Recursively flatten
    }
}
```

## Two-Layer Type System Risk: ValueType vs JuliaType (Issue #2585)

SubsetJuliaVM has two parallel type representations at runtime: `ValueType` (used by the compiler and VM instructions) and `JuliaType` (used by method dispatch). This duality creates a risk of **type information loss** when converting between them, particularly for TypeVar upper bounds.

### The Risk

When a function parameter has `x::T where T<:Integer`, the compiler must:
1. Parse the upper bound `Integer` from the where clause
2. Store it in `JuliaType` form (e.g., `JuliaType::Integer`)
3. Use it during dispatch to constrain type matching

The risk occurs when `JuliaType` upper bounds are converted to `ValueType` for compile-time inference:

| JuliaType | ValueType | Information Lost? |
|-----------|-----------|-------------------|
| `Integer` | `I64` | Yes — `Integer` is abstract, `I64` is concrete |
| `Real` | `F64` | Yes — `Real` includes Int64, Float32, etc. |
| `Number` | `Any` | Yes — broad collapse |
| `AbstractFloat` | `F64` | Yes — loses Float32, Float16 |

### Fix Applied (PR #2584)

The TypeVar upper bound loss was fixed by preserving the `JuliaType` upper bound through the dispatch pipeline instead of converting to `ValueType` prematurely. The fix ensures:

1. `where {T<:Integer}` correctly constrains `T` to integer subtypes during dispatch
2. `where {T<:Real}` correctly constrains `T` to real number subtypes
3. The upper bound is used in `is_subtype_of_parametric()` for dispatch decisions

### Prevention: Type Conversion Guidelines

When working with type parameter bounds:

- [ ] **Never convert TypeVar bounds to ValueType** for dispatch decisions — use `JuliaType` directly
- [ ] **Preserve abstract type information** — `Integer` is not the same as `Int64`
- [ ] **Test with constrained TypeVars** — verify `f(x::T) where T<:Integer` rejects `Float64` arguments
- [ ] **Check both compile-time and runtime paths** — the fix must work in both the compile-time dispatch (`compile/method_table.rs` / `types/julia_type/comparison.rs`) and `vm/exec/call_dynamic*.rs`

### Fixture Tests

- `subset_julia_vm/tests/fixtures/dispatch/where_context_dispatch.jl` — Tests Integer-bounded div dispatch and Real-bounded addition (Issue #2556)
- `subset_julia_vm/tests/fixtures/types/diagonal_rule.jl` — Tests diagonal rule enforcement at compile time (Issue #2554)

## Parametric Struct Parent: Three Registries (Issue #5614)

A user parametric struct (`struct Circle{T<:Real} <: Shape`) is NOT in
`shared_ctx.struct_defs` — it instantiates lazily and lives in
`shared_ctx.parametric_structs`. Its declared parent must be threaded into **three**
separate subtype data sources that currently disagree:

1. **VM `type_ancestors`** (`vm/mod.rs` `compute_type_ancestors`) — includes parametric
   structs via `parametric_struct_parents`. Consumed by `check_abstract_type_hierarchy`
   and `check_nominal_supertype_chain`. This is the **complete** source: `Circle{Int} <:
   Shape`, bare `Circle <: Shape`, `g(x::Shape)` dispatch, and `typeintersect(Circle{Int},
   Shape)` all work via this source.
2. **CoreType `STRUCT_PARENT_REGISTRY`** (thread-local, `inference_core/type_core.rs`) —
   populated from `struct_defs` + `abstract_types` ONLY → missing parametric user structs
   at runtime. Consumed by `CoreType::is_subtype_of`.
3. **`method_table.struct_parents`** (`compile/method_table.rs`) — also built from
   `struct_defs` ONLY → missing parametric structs. Consumed by
   `needs_struct_parent_fallback` / `struct_is_subtype_of_abstract` for
   `where {T<:UserAbstract}` bound checks.

**Rule**: when a parametric-user-struct ↔ user-abstract-parent relation misbehaves,
identify WHICH source the failing path consults:
- Runtime `<:` on a `where` form → fails source #2 → fix at VM-layer (extract bare
  nominal head, resolve via source #1 `check_abstract_type_hierarchy`).
- Dispatch `f(x::T) where {T<:UserAbstract}` → fails source #3 → seed
  `parametric_structs`' `(base→parent base)` pairs into `struct_parent_map` at
  `compile/mod.rs`. This also feeds the inference registry (#2) safely.
- Prefer reusing source #1 over broadening #2/#3, which perturb inference/dispatch widely.

## `try`/`catch`/`finally` Tail-Position Return-Type Join (Issue #10254)

`try ... catch ... [else ...] [finally ...] end` is a value-producing
expression (see `LOWERING.md` "`try`/`catch`/`finally` as an Expression" for
the full lowering mechanism and the value-semantics rule set). This section
covers the **type-inference half** of that contract: the declared/inferred
return type a caller sees for a try/catch expression must be the type that
actually shows up at runtime, or callers slot-type-mismatch crash (the
original failure mode of Issue #9131) or silently receive the wrong static
type for arithmetic/`typeof()`/`::T`-annotated returns (Issue #10254).

**Rule**: the inferred type of a try/catch expression is the **join** of the
tail-expression types of every branch that can produce the value — the
`try` block's tail, the `catch` block's tail, and the `else` block's tail
when present. `finally`'s own tail type is never part of the join (it never
contributes the value — see Rule 3 in `LOWERING.md`).

This is the same `join_type()` merge used for control-flow local-variable
widening (`LOWERING.md` "Control-Flow Type Tracking", Issue #3044/#3049),
applied here to the tail *value* itself rather than to each individual
local variable name:

- `infer_block_branch` (`compile/abstract_interp/engine/mod.rs`) computes
  and joins the `try_ret`/`catch_ret`/`else_ret` return types for a
  `Stmt::Try` reached during abstract interpretation.
- The `Stmt::Try` arms in `compile/inference.rs` do the same join for the
  narrower per-statement inference pass (also responsible for the
  Issue #9131 env-sharing fix: the catch branch must infer against the
  **pre-try** environment, not a mutated shared reference, or a
  catch-branch assignment silently overwrites the try-branch's inferred
  type for the same variable).
- Because the lowering rewrite (`LOWERING.md`) makes assignment-tailed
  branches (`x = v`, `x += v`, `global x = v`) produce a value exactly the
  same way as an expression-tailed branch, the type-inference join sees a
  uniform shape regardless of whether a branch's tail is a plain
  expression or an assignment — no special-casing was needed in the
  inference layer once the lowering layer treated both uniformly.

**Regression coverage**: `tests/fixtures/exceptions/try_catch_type_inference_9131.jl`
(env-join correctness) and `tests/fixtures/exceptions/try_catch_tail_value_semantics_10254.jl`
(typed post-use of an assign-tail result: `f() + 100`, `typeof(f())`,
string concatenation, and a `function f()::Int` return annotation on a
bare try/catch body) both exercise this join end-to-end against upstream
`julia` parity.

### Extending the Join to Every Assignment Shape, and Its CFG Fast Paths (Issue #10431)

The last bullet above ("no special-casing was needed... once the lowering
layer treated both uniformly") held for `Stmt::Assign`/`Stmt::AddAssign`
specifically, because `infer_block_branch` already had a per-statement arm
for `Stmt::Assign` before Issue #10074/#10254 — the lowering rewrite just
gave it a uniform shape to see. Generalizing the lowering rewrite to
`IndexAssign`/`FieldAssign`/`DictAssign`/(most) tuple destructuring (Issue
#10431, see `LOWERING.md` "Generalizing to Every Assignment Shape") is
**not** free the same way: `infer_block_branch`'s per-statement loop needed
NEW arms for these statement kinds too (mirroring the existing
`Stmt::Assign` arm: `last_stmt_type = Some(self.infer_expr(value, env))`),
or the block's inferred fallthrough/tail type silently stayed `Nothing`
even though the compiled body correctly returns the RHS value — a
declared-type-vs-actual-value mismatch that, unlike a merely-wrong printed
value, **crashes** at the call site once the (correctly) declared type
feeds a type-specific instruction (e.g. `PrintI64NoNewline` fed an actual
`Nothing` from the still-unfixed body).

Three additional wrinkles surfaced fixing this generally, all now covered:

- **CFG fast paths.** `infer_block_with_fixpoint` has two fast paths that
  bypass the general per-statement loop entirely when eligible:
  `try_infer_straightline_cfg_return` (no branches) computes each
  statement's value via `cfg_authoritative_statement_value`, and
  `try_infer_all_return_cfg` (every exit ends in an explicit `return`) via
  `infer_cfg_authoritative_payload_stmt`. Both needed their own matching
  arms — `cfg_authoritative_statement_value` gained one;
  `cfg_authoritative_all_return_stmt_supported`'s existing gate already
  excludes `IndexAssign`/`FieldAssign`/`DictAssign`, so that fast path
  correctly declines and falls through to the general path instead of
  needing a fix.
- **Tuple destructuring's `Stmt::Block(inner) if is_last` arm.** A
  destructuring decomposition is itself a tail-position `Stmt::Block` (see
  `LOWERING.md`), and `infer_block_branch`'s existing nested-block arm
  (added for Issue #10023's `global x = v` shape) naively used the inner
  block's `fallthrough` — the type of its LAST per-target assignment (e.g.
  `b`), not the whole destructured tuple. Fixed by checking
  `destructuring_tail_value` (the same detector the lowering/codegen fix
  uses) and inferring the reconstructed value's type instead when that
  shape is detected.
- **The lazy call-site specializer** (`vm/specialize/*.rs`) computes its
  OWN declared return type independently (it is a separate, on-demand
  compiled representation keyed on concrete call-site argument types, used
  once the abstract-interp return type above is already known) and needed
  the identical `IndexAssign`/`FieldAssign` tail-value fix in its own
  `compile_function_body`/`compile_block_with_implicit_return`
  (`vm/specialize/stmt.rs`) — this is what turned "declared type says I64,
  compiled specialized body still silently returns `Nothing`" into a
  runtime `PrintI64NoNewline` crash, which is how the gap was actually
  found (`Stmt::DictAssign` is not reachable there: this specializer's
  `compile_stmt` has no arm for it at all, so a `d[k] = x` statement fails
  specialization and falls back to the legacy/abstract-interp-typed path
  above, which IS fixed).

The one deliberately unfixed sub-case — an independent literal-tuple RHS
with matching arity, `(a, b) = (1, 2)` exactly (Issue #10444) — is
correspondingly still typed as the type of its last target (`b`'s type),
not `Tuple`, in every one of these inference paths; see `LOWERING.md` for
why a general fix was not attempted in this change.

**Regression coverage**: `tests/fixtures/control_flow/assign_statement_tail_value_10431.jl`,
including typed arithmetic post-use of an indexed-assign tail result and a
negative regression guard proving a genuine multi-statement `begin ... end`
(with and without an `@eval` wrapper sharing one macro-call span) still
infers/returns its last statement's type, not a reconstructed tuple.

## Related Documentation

- `LOWERING.md` - Parameter parsing, type annotations, and CST contract
- `NUMERIC_TYPES.md` - Numeric type parity checklist and intrinsic dispatch
- `BINARY_DISPATCH.md` - Binary operator dispatch paths
- `ARCHITECTURE_OVERVIEW.md` - Overall VM architecture
- `CLAUDE.md` - Top-level contributor guidelines
