# Runtime Type Objects

SubsetJuliaVM currently stores type values compactly as `Value::DataType(JuliaType)`.
Issue #3896 starts separating that value projection from a registry-backed runtime
object handle.

## Upstream References

- `julia/base/boot.jl`: bootstrap names and constructors for `DataType`, `TypeVar`, and `UnionAll`
- `julia/base/essentials.jl`: `unwrap_unionall`, `rewrap_unionall`, and TypeVar-renaming helpers
- `julia/base/reflection.jl`: Julia-level reflection APIs over `DataType` and `UnionAll`
- `julia/src/jltypes.c`: builtin `DataType`, `TypeVar`, `UnionAll`, type caches, and identity-sensitive TypeVar creation
- `julia/src/datatype.c`: datatype allocation, field layout, concreteness, mutability, and isbits flags
- `julia/src/subtype.c`: UnionAll environments, bounds, subtype, and intersection behavior
- `julia/src/builtins.c`: Core reflection builtins including `TypeVar`, `UnionAll`, `fieldtypes`, and `typeintersect`

## sjulia Model

`RuntimeTypeRegistry` owns the runtime read model for supported type objects.
Each `RuntimeTypeHandle` has:

- a `RuntimeTypeObjectKind`: `DataType`, `UnionAll`, or `TypeVar`
- a `RuntimeTypeIdentity`: stable identity key derived from the object kind and shared `CoreType`
- a `JuliaType` projection used by existing VM values and Pure Julia reflection

`CoreType` remains the semantic representation for subtype, intersection, type
join, and dispatch-cache work. Runtime handles carry object-kind and identity
metadata so reflection builtins do not need to re-derive those facts locally.

## Supported

- `DataType.parameters` for concrete parametric types such as `Vector{Int64}`,
  `Dict{Symbol, Int64}`, tuples, nested parameters, and supported user structs.
- `UnionAll.var` and `UnionAll.body` projections for built-in parametric
  schemas such as `Vector`, `Matrix`, `Dict`, and `Set`.
- `TypeVar.name`, `TypeVar.lb`, and `TypeVar.ub`.
- Registry-backed `typeof(::Type)` kind projection for supported type objects:
  `DataType`, `UnionAll`, and `TypeVar`.
- Fresh `TypeVar(:T)` constructor results have identity separate from their
  name/bounds projection for `===`, `isequal`, and `objectid`.
- Builtin AST-like datatype metadata for `Expr`, `QuoteNode`,
  `LineNumberNode`, and `GlobalRef`.
- `objectid(::Type)` now hashes the registry-backed type identity instead of
  manually hashing `JuliaType.name()`.
- `isa(x, DataType)`, `isa(x, UnionAll)`, `isa(x, TypeVar)`, and
  `isa(x, Type)` agree with `typeof(x)` for type-object values (Issue
  #3909). The `Isa` builtin routes `Value::DataType(jt)` through the
  registry's kind projection so `isa(Vector, UnionAll)` and
  `isa(Vector{Int64}, DataType)` are both true, matching upstream Julia.
- `Base.unwrap_unionall(t)` iterates through `UnionAll` bodies via
  `isa(t, UnionAll); t = t.body`. Dynamic field access (`Instr::GetFieldByName`)
  on `Value::DataType` mirrors the static `ValueType::DataType` path so the
  loop body executes correctly when `t` is not statically narrowed.
- `UnionAll(var, body)` constructs a `JuliaType::UnionAll` from a `TypeVar`
  and a body (Issue #4694). Matches upstream `jl_type_unionall`'s smart-wrap
  semantics: when the body does not reference the bound variable, the body
  is returned unchanged. This enables `Base.rewrap_unionall` and round-trips
  with `Base.unwrap_unionall`.

## Unsupported Or Approximate

- Heap-allocated `DataType` and `UnionAll` objects are not yet separate
  `Value` variants. Their public value projection is still
  `Value::DataType(JuliaType)`.
- Built-in schema TypeVars projected from `Vector.var`, `Dict.var`, and
  `.parameters` remain registry-projected semantic values rather than heap
  objects. Fresh identity is currently implemented for explicit `TypeVar(...)`
  constructor results.
- Full datatype layout identity and type-cache canonicalization are not modeled
  yet.
- `UnionAll` environment-sensitive operations such as full diagonal widening
  remain limited to the currently supported reflection and subtype surface.
