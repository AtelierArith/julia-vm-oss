# Type System Architecture

*Last updated: 2026-06-10*

This document describes the type system used in SubsetJuliaVM, including the relationships between different type representations.

## Type Representations

SubsetJuliaVM uses four distinct type representations at different compilation stages:

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

Located in: `subset_julia_vm/src/compile/lattice/types.rs`

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

Located in: `subset_julia_vm/src/compile/lattice/types.rs`

`ConcreteType` represents specific Julia types inside the lattice. There are currently **49 variants**:

- **Signed integers** (6): `Int8`, `Int16`, `Int32`, `Int64`, `Int128`, `BigInt`
- **Unsigned integers** (5): `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UInt128`
- **Floating point** (4): `Float16`, `Float32`, `Float64`, `BigFloat`
- **Boolean** (1): `Bool`
- **Text** (2): `String`, `Char`
- **Special** (3): `Any`, `Nothing`, `Missing`
- **Abstract numeric** (3): `Number`, `Integer`, `AbstractFloat`
- **Symbolic** (1): `Symbol`
- **Composite/Collection** (9): `Array{element}`, `Tuple{elements}`, `TupleVararg{elements, tail}`, `NamedTuple{fields}`, `Range{element}`, `Dict{key, value}`, `Set{element}`, `Generator{element}`, `Pairs`
- **User-defined** (1): `Struct{name, type_id}`
- **Callable** (3): `Function{name}`, `Closure{name, captures}`, `ComposedFunction{outer, inner}`
- **Type system** (2): `DataType{name}`, `Module{name}`
- **IO** (1): `IO`
- **Metaprogramming** (4): `Expr`, `QuoteNode`, `LineNumberNode`, `GlobalRef`
- **Pattern matching** (2): `Regex`, `RegexMatch`
- **Type unions** (1): `UnionOf(Vec<ConcreteType>)`
- **Enum** (1): `Enum{name}` — for Julia `@enum` types (Issue #2863)

Note: `Dict{key, value}` is parametric in `ConcreteType`, allowing the compile-time lattice to track key/value types for `Dict{K,V}`.

### 2. ValueType (Runtime)

Located in: `subset_julia_vm/src/vm/value/value_enum.rs`

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
    Array, ArrayOf(ArrayElementType),
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

Located in: `subset_julia_vm/src/vm/value/value_enum.rs`

`Value` is the main runtime value enum. Every Julia value at runtime is represented as a `Value` variant. There are currently **52 variants** (verified by compile-time exhaustiveness test `test_all_value_variants_constructed`):

| Category | Variants | Count |
|----------|----------|-------|
| Signed integers | `I8(i8)`, `I16(i16)`, `I32(i32)`, `I64(i64)`, `I128(i128)`, `BigInt(RustBigInt)` | 6 |
| Unsigned integers | `U8(u8)`, `U16(u16)`, `U32(u32)`, `U64(u64)`, `U128(u128)` | 5 |
| Boolean | `Bool(bool)` | 1 |
| Floating point | `F16(f16)`, `F32(f32)`, `F64(f64)`, `BigFloat(RustBigFloat)` | 4 |
| String types | `Str(String)`, `Char(char)` | 2 |
| Singletons | `Nothing`, `Missing`, `Undef`, `SliceAll` | 4 |
| Collections | `NativeArray(ArrayRef)`, `Memory(MemoryRef)`, `MemoryRef(Box<MemoryRefValue>)`, `Range(RangeValue)`, `Tuple(TupleValue)`, `SimpleVector(TupleValue)`, `NamedTuple(NamedTupleValue)`, `Pairs(PairsValue)`, `Dict(DictRef)`, `Set(SetValue)`, `Ref(RefCellRef)`, `Generator(Box<GeneratorValue>)` | 12 |
| Struct types | `Struct(StructInstance)`, `StructRef(usize)` | 2 |
| RNG | `Rng(RngInstance)` | 1 |
| Type/Module | `DataType(JuliaType)`, `RuntimeTypeVar(Box<RuntimeTypeVarValue>)`, `Module(Box<ModuleValue>)` | 3 |
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

Located in: `subset_julia_vm/src/compile/bridge.rs`

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

When adding a new variant to the `ValueType` enum in `subset_julia_vm/src/vm/value/value_enum.rs`, you **MUST** update all match statements throughout the codebase. Failure to do so will cause compilation errors or runtime failures.

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
  - [ ] Update the compile-time fallback in `compile/promotion.rs` only for bootstrapping/cache-miss coverage
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
rg -n "should_use_inline_dynamic_op|inline_dynamic|fast_path|PrimitiveFallbackFirst" subset_julia_vm/src/vm -g '*.rs'

# Find pass-through catch-all arms that need context review
rg -n "other => other" subset_julia_vm/src/vm -g '*.rs'
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

The `compile_expr_as` function in `subset_julia_vm/src/compile/expr/mod.rs` handles type conversions at compile time.

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

## Type{T}→T Return Type Override Pattern (Issue #2245)

Functions like `typemin(::Type{T})`, `typemax(::Type{T})`, `zero(::Type{T})`, and `one(::Type{T})` return a value of type `T`. However, when these functions are implemented in Pure Julia (e.g., `typemin(::Type{Float64}) = -Inf`), the method table may incorrectly infer the return type.

### The Problem

1. The Pure Julia method `typemin(::Type{Float64}) = return -Inf` is compiled
2. The method table stores `return_type = Bool` (or another incorrect type)
3. During `compile_call`, the compiler trusts `method.return_type`
4. With an incorrect return type, the compiler generates wrong type conversion instructions
5. Runtime fails with type errors (e.g., `BoolToI64` applied to a `Float64`)

### The Solution: Return Type Overrides

Add explicit return type overrides in **both** `compile_call` (call.rs) and `infer_expr_type` (infer.rs):

```rust
// In call.rs and infer.rs
"typemin" | "typemax" if args.len() == 1 => {
    let julia_ty = self.infer_julia_type(&args[0]);
    match julia_ty {
        JuliaType::TypeOf(inner) => match *inner {
            JuliaType::Float64 => ValueType::F64,
            JuliaType::Float32 => ValueType::F32,
            JuliaType::Int64 => ValueType::I64,
            // ... all numeric types
            _ => method.return_type.clone(),
        },
        _ => method.return_type.clone(),
    }
}
```

### Functions Requiring Overrides

| Function | Implementation Pattern | Override Status |
|----------|----------------------|-----------------|
| `typemin(::Type{T})` | Returns `T` type's minimum | ✓ Implemented |
| `typemax(::Type{T})` | Returns `T` type's maximum | ✓ Implemented |
| `zero(::Type{T})` | Returns zero of type `T` | Not yet needed |
| `one(::Type{T})` | Returns one of type `T` | Not yet needed |
| `eps(::Type{T})` | Returns epsilon of type `T` | Not yet needed |

### Code Review Checklist

When adding new Pure Julia methods of the form `f(::Type{T}) → T`:

- [ ] Check if the method table infers the correct return type
- [ ] If not, add override in `call.rs` `compile_call` function
- [ ] Add the **same** override in `infer.rs` `infer_expr_type` function
- [ ] Add tests that verify `typeof(f(T)) == T` for all supported types
- [ ] Document the function in the table above

### Prevention Tests

- `subset_julia_vm/tests/fixtures/number/type_param_return_types.jl` - Verifies return types for typemin/typemax/zero/one (Issue #2245)

## Parametric Type Dispatch Limitation (Issue #2384, #2388)

### Known Limitation

Inside parametric functions with `where T`, function dispatch does NOT resolve type parameters at compile time. This means that calling generic functions inside `where T` context may dispatch to unexpected methods:

```julia
function foo(x::T, y::T) where T
    div(x, y)  # Dispatches to generic div(x, y), NOT div(::Int64, ::Int64)
end

# Result: div returns Float64 instead of Int64!
foo(Int64(6), Int64(2))  # => 3.0 (Float64)
```

### Root Cause

The compiler sees `x::T` and `y::T` at compile time, but `T` is an unresolved type parameter. When dispatching `div(x, y)`, the compiler cannot determine that `T = Int64`, so it falls back to the generic `div(x, y)` method which returns `Float64`.

### Affected Patterns

1. **Parametric struct constructors** - Inner constructors with `where T` that call reduction functions
2. **Generic helper functions** - Functions with `where T` that call type-sensitive operations
3. **Any function using** `div`, `mod`, `rem`, `fld`, etc. inside `where T` context

### Workarounds

#### 1. Use Intrinsics Directly

For critical type-preserving operations, use intrinsics instead of function calls:

```julia
# Helper that uses intrinsic directly (no dispatch)
function _safe_div(x::Int64, y::Int64)
    return sdiv_int(x, y)  # Intrinsic, no dispatch
end

# Use in parametric function
function Rational{T}(num::T, den::T) where T
    g = gcd(num, den)
    if g > 1
        # Use helper instead of div(num, g)
        num = _safe_div(Int64(num), Int64(g))
        den = _safe_div(Int64(den), Int64(g))
    end
    return new{T}(num, den)
end
```

#### 2. Add Return Type Overrides

For commonly used functions, add special handling in `compile/expr/call/` (around line 1493) to override the return type based on argument types:

```rust
// In compile/expr/call/
"gcd" | "lcm" if args.len() == 2 => {
    // Preserve BigInt/Int64 types for gcd/lcm
    let has_bigint = args.iter().any(|arg| {
        matches!(self.infer_expr_type(arg), ValueType::BigInt)
    });
    if has_bigint {
        ValueType::BigInt
    } else {
        method.return_type.clone()
    }
}
```

### Functions with Return Type Overrides

| Function | Override Location | Notes |
|----------|-------------------|-------|
| `gcd` | call.rs, infer.rs | Preserves BigInt/Int64 (Issue #2383) |
| `lcm` | call.rs, infer.rs | Preserves BigInt/Int64 (Issue #2383) |
| `abs`, `abs2`, `sign` | call.rs, infer.rs | Preserves BigInt/I128/F32/F16 (Issue #2383) |
| `typemin`, `typemax` | call.rs, infer.rs | Type{T}→T pattern (Issue #2245) |

### Code Review Checklist

When implementing or reviewing parametric functions (`where T`):

- [ ] Does the function call generic methods like `div`, `mod`, `rem`?
- [ ] If yes, is the return type critical for correctness?
- [ ] Consider using an intrinsic helper function instead
- [ ] Or add a return type override in call.rs/infer.rs
- [ ] Test with actual type parameters to verify type preservation

### Audit Command

Find potentially problematic patterns:

```bash
# Find parametric functions that call div/mod/rem
rg -n -C 20 'where T' subset_julia_vm/src/julia -g '*.jl' | \
  rg '(div|mod|rem)\(' | \
  rg -v 'sdiv_int|srem_int'
```

### Long-term Solution

The fundamental fix would be to improve the compiler's type parameter resolution during function specialization:

1. When `Rational{Int64}` constructor is compiled, track that `T = Int64`
2. Use this binding when dispatching internal function calls
3. `div(num::T, g::T)` would then dispatch to `div(::Int64, ::Int64)`

Related code locations:
- `compile/expr/call/` - Method dispatch
- `compile/expr/infer/` - Type inference
- `compile/stmt.rs` - Function specialization

### Fixture Tests

- `subset_julia_vm/tests/fixtures/dispatch/parametric_function_dispatch.jl` - Tests workarounds and known-working patterns
- `subset_julia_vm/tests/fixtures/rational/` - Rational arithmetic tests (affected by this limitation)

## BigInt Type Preservation (Issue #2383, #2386)

BigInt type must be preserved throughout function calls and arithmetic operations. This requires updates in three locations when adding BigInt support for a function.

### The Three Locations

| Location | File | Purpose |
|----------|------|---------|
| Type Inference | `compile/expr/infer/` | Infer return type based on argument types |
| Compile-time Return Type | `compile/expr/call/` | Override method table return type |
| Runtime Operations | `vm/dynamic_ops/` | Handle BigInt operands in dynamic dispatch |

### Example: Adding BigInt Support for `abs`

**1. Type Inference (infer.rs)** - Add to `infer_expr_type` match for `Expr::Call`:

```rust
"abs" | "abs2" | "sign" => {
    if let Some(arg) = args.first() {
        let arg_type = self.infer_expr_type(arg);
        match arg_type {
            ValueType::BigInt => ValueType::BigInt,  // Preserve BigInt
            ValueType::I128 => ValueType::I128,
            ValueType::F32 => ValueType::F32,
            _ => ValueType::F64,
        }
    } else {
        ValueType::F64
    }
}
```

**2. Compile-time Return Type (call.rs)** - Add to return type override section (~line 1493):

```rust
"abs" | "abs2" | "sign" if args.len() == 1 => {
    let arg_type = self.infer_expr_type(&args[0]);
    match arg_type {
        ValueType::BigInt => ValueType::BigInt,
        ValueType::I128 => ValueType::I128,
        ValueType::F32 => ValueType::F32,
        _ => method.return_type.clone(),
    }
}
```

**3. Runtime Operations (`vm/dynamic_ops/`, `vm/exec/binary_both.rs`)** - Add BigInt cases:

```rust
// In dynamic_int_div
(Value::BigInt(x), Value::BigInt(y)) => {
    let zero = num_bigint::BigInt::from(0);
    if *y == zero {
        return Err(VmError::DivisionByZero);
    }
    Ok(Value::BigInt(x / y))
}
```

### Functions with BigInt Preservation

| Function | Behavior | Status |
|----------|----------|--------|
| `abs`, `abs2`, `sign` | Unary, preserves argument type | ✓ Implemented |
| `gcd`, `lcm` | Binary, returns BigInt if any arg is BigInt | ✓ Implemented |
| `+`, `-`, `*` | Binary, promotes to BigInt | ✓ Via dynamic dispatch |
| `÷`, `div` | Integer division | ✓ Implemented |
| `%`, `rem`, `mod` | Remainder/modulo | ✓ Implemented |

### Code Review Checklist

When adding a new function that should preserve BigInt:

- [ ] Add BigInt case to `infer_expr_type` in `infer.rs`
- [ ] Add return type override in `compile_call` in `call.rs`
- [ ] Add BigInt operand handling in the relevant `dynamic_*` path under
      `vm/dynamic_ops/` or `vm/exec/binary_both.rs`
- [ ] Add test cases to `bigint/type_preservation.jl`
- [ ] Verify chained function calls preserve type (e.g., `abs(gcd(a, b))`)

### Fixture Tests

- `subset_julia_vm/tests/fixtures/bigint/type_preservation.jl` - Comprehensive BigInt type preservation tests

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
rg -n 'Value::Function' subset_julia_vm/src/vm -g '*.rs' | \
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

## Related Documentation

- `LOWERING.md` - Parameter parsing, type annotations, and CST contract
- `NUMERIC_TYPES.md` - Numeric type parity checklist and intrinsic dispatch
- `BINARY_DISPATCH.md` - Binary operator dispatch paths
- `ARCHITECTURE_OVERVIEW.md` - Overall VM architecture
- `CLAUDE.md` - Top-level contributor guidelines
