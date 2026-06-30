# Runtime introspection functions
# Corresponds to julia/base/runtime_internals.jl

# Note: isexported and ispublic are implemented as builtin functions
# and recognized by the compiler. No Julia wrapper needed - they compile
# directly to CallBuiltin instructions.

# SubsetJuliaVM has no world-age separated method tables yet, so most
# representative invokelatest/invoke_in_world surfaces are equivalent to
# ordinary calls in the single-world runtime. World age zero is still older
# than all user-defined methods and mirrors upstream's MethodError boundary.
# Full world-age method-table semantics are tracked by Issues #4271/#4285.
get_world_counter() = UInt64(1)
tls_world_age() = get_world_counter()
invokelatest(f, args...; kwargs...) = f(args...; kwargs...)
function invoke_in_world(world, f, args...; kwargs...)
    if world == UInt64(0)
        throw(MethodError(f, args))
    end
    f(args...; kwargs...)
end

"""
    Base.issingletontype(T)

Determine whether type `T` has exactly one possible instance; for example, a
struct type with no fields except other singleton values.
If `T` is not a concrete type, then return `false`.
"""
function issingletontype(@nospecialize(t))
    # Upstream (julia/base/runtime_internals.jl) tests
    #   isa(t, DataType) && isdefined(t, :instance) &&
    #       datatype_layoutsize(t) == 0 && datatype_pointerfree(t)
    # SubsetJuliaVM exposes no `:instance` field or layout intrinsics, so the
    # equivalent condition is expressed through the public reflection
    # predicates: a singleton is an immutable, concrete struct type with no
    # fields. The mutability check excludes empty `mutable struct`s (whose
    # instances are distinct) and variable-size types such as `String`/`Symbol`,
    # which upstream classifies as mutable and therefore non-singleton.
    isconcretetype(t) && isstructtype(t) && !ismutabletype(t) && fieldcount(t) == 0
end

"""
    Base.datatype_alignment(dt::DataType) -> Int

Memory allocation minimum alignment for instances of this type.
Can be called on any `isconcretetype`, although for Memory it will
give the alignment of the elements, not the whole object.
"""
datatype_alignment(T::Type) = _datatype_alignment(T)

# Round `x` up to the next multiple of `sz` (a power of two). Mirrors the
# upstream `LLT_ALIGN` macro in `julia/base/runtime_internals.jl`.
LLT_ALIGN(x, sz) = (x + sz - 1) & -sz

"""
    Base.allocatedinline(T::Type) -> Bool

Return whether instances of type `T` are stored inline (unboxed) when held as a
struct field or array element, rather than being referenced through a pointer.

Mirrors upstream `Base.allocatedinline` (`julia/base/array.jl`). For the subset
SubsetJuliaVM supports this reduces to: concrete, immutable `DataType`s — which
covers all `isbits` types as well as immutable structs that carry boxed fields
(e.g. a `String` field). Mutable structs, abstract types, and variable-size
builtins such as `String`/`Symbol` are not stored inline.
"""
allocatedinline(T::Type) = _allocatedinline(T)

# amount of total space taken by T when stored in a container
"""
    Base.aligned_sizeof(T::Type) -> Int

The total space, in bytes, taken by an instance of `T` when stored inside a
container (struct field or array element), including any trailing padding needed
to satisfy `T`'s alignment. When `T` is not stored inline this is the pointer
width.

Mirrors upstream `aligned_sizeof` (`julia/base/runtime_internals.jl`).
"""
function aligned_sizeof(T::Type)
    if allocatedinline(T)
        al = datatype_alignment(T)
        return LLT_ALIGN(sizeof(T), al)
    end
    # Upstream returns `Core.sizeof(Ptr{Cvoid})`; `Cvoid === Nothing`, and the
    # VM has no `Cvoid` alias, so the pointer-width is taken from `Ptr{Nothing}`.
    return sizeof(Ptr{Nothing})
end
