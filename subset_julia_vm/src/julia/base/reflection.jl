# Reflection functions for introspection
#
# Based on julia/base/runtime_internals.jl
# These functions wrap internal VM builtins (_fieldnames, _fieldtypes)

"""
    fieldnames(T::Type)

Get a tuple with the names (as Symbols) of the fields of a composite DataType `T`.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldnames(Point)  # (:x, :y)
```
"""
function fieldnames(T::Type)
    _fieldnames(T)
end

"""
    fieldname(T::Type, i::Integer) -> Symbol

Get the name (as a Symbol) of the i-th field of composite DataType `T`.
Fields are numbered starting from 1.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldname(Point, 1)  # :x
fieldname(Point, 2)  # :y
```
"""
function fieldname(T::Type, i::Integer)
    # Match upstream Base.fieldname(t::DataType, i::Integer)
    # (julia/base/runtime_internals.jl): abstract types and out-of-range /
    # non-positive indices throw `ArgumentError` with specific messages.
    if isabstracttype(T)
        throw(ArgumentError("type does not have definite field names"))
    end
    names = fieldnames(T)
    n_fields = length(names)
    if i > n_fields
        field_label = n_fields == 1 ? "field" : "fields"
        throw(ArgumentError("Cannot access field $(i) since type $(T) only has $(n_fields) $(field_label)."))
    end
    if i < 1
        throw(ArgumentError("Field numbers must be positive integers. $(i) is invalid."))
    end
    # Convert to Symbol if it's a String (VM returns strings)
    n = names[i]
    isa(n, Symbol) ? n : Symbol(n)
end

"""
    fieldindex(T::Type, name::Symbol) -> Int
    fieldindex(T::Type, name::Symbol, err::Bool) -> Int

Get the index of a named field. If `err` is true (the default), throws an error
if the field does not exist. If `err` is false, returns 0 for non-existent fields.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldindex(Point, :x)  # 1
fieldindex(Point, :y)  # 2
fieldindex(Point, :z, false)  # 0 (field doesn't exist)
```
"""
function fieldindex(T::Type, name::Symbol, err::Bool)
    fnames = fieldnames(T)
    name_str = string(name)
    for i in 1:length(fnames)
        if string(fnames[i]) == name_str
            return i
        end
    end
    if err
        throw(ArgumentError("type $(T) has no field named $(name)"))
    else
        return 0
    end
end

function fieldindex(T::Type, name::Symbol)
    # Default: err=true
    fnames = fieldnames(T)
    name_str = string(name)
    for i in 1:length(fnames)
        if string(fnames[i]) == name_str
            return i
        end
    end
    throw(ArgumentError("type $(T) has no field named $(name)"))
end

"""
    fieldoffset(T::Type, i::Integer) -> UInt64
    fieldoffset(T::Type, name::Symbol) -> UInt64

The byte offset of a field of a type relative to its start.
"""
function fieldoffset(T::Type, i::Integer)
    _fieldoffset(T, i)
end

function fieldoffset(T::Type, name::Symbol)
    fieldoffset(T, fieldindex(T, name))
end

"""
    fieldtypes(T::Type)

The declared types of all fields in a composite DataType `T` as a tuple.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldtypes(Point)  # (Float64, Float64)
```
"""
function fieldtypes(T::Type)
    _fieldtypes(T)
end

"""
    fieldtype(T::Type, i::Integer) -> Type

Get the declared type of the i-th field of composite DataType `T`.
Fields are numbered starting from 1.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldtype(Point, 1)  # Float64
fieldtype(Point, 2)  # Float64
```
"""
function fieldtype(T::Type, i::Integer)
    types = fieldtypes(T)
    result = if i < 1 || i > length(types)
        # Upstream reports the type object itself as the bounds container, e.g.
        # `BoundsError: attempt to access DataType at index [3]` (Issue #5099).
        throw(BoundsError(T, i))
    else
        types[i]
    end
    result
end

"""
    fieldtype(T::Type, name::Symbol) -> Type

Get the declared type of a field by name in composite DataType `T`.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldtype(Point, :x)  # Float64
fieldtype(Point, :y)  # Float64
```
"""
function fieldtype(T::Type, name::Symbol)
    idx = fieldindex(T, name)
    fieldtype(T, idx)
end

"""
    nfields(x)

Get the number of fields in the given object.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
nfields(p)  # 2
```
"""
function nfields(x)
    length(fieldnames(typeof(x)))
end

"""
    fieldcount(T::Type)

Get the number of fields that instances of the given type would have.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

fieldcount(Point)  # 2
```
"""
function fieldcount(T::Type)
    length(fieldnames(T))
end

"""
    isabstracttype(T::DataType) -> Bool

Test whether `T` is an abstract type, i.e., declared with `abstract type`.

# Examples
```julia
isabstracttype(Number)    # true
isabstracttype(Int64)     # false
```
"""
function isabstracttype(T::Type)
    _isabstracttype(T)
end

"""
    isconcretetype(T::DataType) -> Bool

Test whether `T` is a concrete type, meaning it can have instances.

# Examples
```julia
isconcretetype(Int64)     # true
isconcretetype(Number)    # false
```
"""
function isconcretetype(T::Type)
    _isconcretetype(T)
end

"""
    isprimitivetype(T::DataType) -> Bool

Test whether `T` is a primitive type with a fixed number of bits and no fields.

# Examples
```julia
isprimitivetype(Int64)    # true
isprimitivetype(String)   # false
```
"""
function isprimitivetype(T::Type)
    _isprimitivetype(T)
end

"""
    isstructtype(T::DataType) -> Bool

Test whether `T` is a struct type (not primitive and not abstract).

# Examples
```julia
isstructtype(String)      # true
isstructtype(Int64)       # false (primitive)
isstructtype(Number)      # false (abstract)
```
"""
function isstructtype(T::Type)
    _isstructtype(T)
end

"""
    ismutabletype(T::DataType) -> Bool

Test whether `T` is a mutable type (mutable struct, Array, Dict).

# Examples
```julia
ismutabletype(Array)      # true
ismutabletype(Int64)      # false
```
"""
function ismutabletype(T::Type)
    _ismutabletype(T)
end

# Reflection predicates migrated from Rust builtins to pure-Julia public
# wrappers (Issue #6738). They are derived from the VM-metadata primitives that
# stay in Rust: `isbitstype` (type-flag query) / `ismutabletype` (over the
# `_ismutabletype` flag intrinsic) and `_fieldnames`. Matches upstream's
# structure (isbits(x) = isbitstype(typeof(x)) etc.). `ismutable` via
# `ismutabletype` also fixes the prior String divergence (the old Rust
# `ismutable` returned false for String; upstream is true). `hasfield` takes an
# unconstrained `name` (a `::Symbol` annotation made symbol literals hit a
# QuoteNode→Symbol conversion error in the base-function routing).
isbits(x) = isbitstype(typeof(x))
ismutable(x) = ismutabletype(typeof(x))
hasfield(T::Type, name) = name in _fieldnames(T)

function supertype(T::Type)
    _supertype(T)
end

function supertypes(T::Type)
    chain = DataType[]
    current = T
    while true
        push!(chain, current)
        current === Any && break
        current = supertype(current)
    end
    _supertypes_tuple(chain)
end

function _supertypes_tuple(chain)
    n = length(chain)
    if n == 0
        return ()
    elseif n == 1
        return (chain[1],)
    elseif n == 2
        return (chain[1], chain[2])
    elseif n == 3
        return (chain[1], chain[2], chain[3])
    elseif n == 4
        return (chain[1], chain[2], chain[3], chain[4])
    elseif n == 5
        return (chain[1], chain[2], chain[3], chain[4], chain[5])
    elseif n == 6
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6])
    elseif n == 7
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7])
    elseif n == 8
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8])
    elseif n == 9
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9])
    elseif n == 10
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10])
    elseif n == 11
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10], chain[11])
    elseif n == 12
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10], chain[11], chain[12])
    elseif n == 13
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10], chain[11], chain[12], chain[13])
    elseif n == 14
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10], chain[11], chain[12], chain[13], chain[14])
    elseif n == 15
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10], chain[11], chain[12], chain[13], chain[14], chain[15])
    elseif n == 16
        return (chain[1], chain[2], chain[3], chain[4], chain[5], chain[6], chain[7], chain[8], chain[9], chain[10], chain[11], chain[12], chain[13], chain[14], chain[15], chain[16])
    else
        error("supertypes: hierarchy too deep")
    end
end

function typeintersect(a::Type, b::Type)
    _typeintersect(a, b)
end

function _typejoin_tuple_from_args(args)
    n = length(args)
    if n == 0
        return Tuple{}
    elseif n == 1
        return Tuple{args[1]}
    elseif n == 2
        return Tuple{args[1], args[2]}
    elseif n == 3
        return Tuple{args[1], args[2], args[3]}
    elseif n == 4
        return Tuple{args[1], args[2], args[3], args[4]}
    elseif n == 5
        return Tuple{args[1], args[2], args[3], args[4], args[5]}
    elseif n == 6
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6]}
    elseif n == 7
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7]}
    elseif n == 8
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8]}
    elseif n == 9
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9]}
    elseif n == 10
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10]}
    elseif n == 11
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10], args[11]}
    elseif n == 12
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10], args[11], args[12]}
    elseif n == 13
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10], args[11], args[12], args[13]}
    elseif n == 14
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10], args[11], args[12], args[13], args[14]}
    elseif n == 15
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10], args[11], args[12], args[13], args[14], args[15]}
    elseif n == 16
        return Tuple{args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8], args[9], args[10], args[11], args[12], args[13], args[14], args[15], args[16]}
    else
        return Core.apply_type(Tuple, args...)
    end
end

function _typejoin_range(params, first_idx)
    joined = params[first_idx]
    for i in (first_idx + 1):length(params)
        joined = typejoin(joined, params[i])
    end
    return joined
end

"""
    typejoin(A::Type, B::Type) -> Type

Compute the smallest type that both `A` and `B` are subtypes of.
This walks up both supertype chains to find the first common ancestor.

# Examples
```julia
typejoin(Int64, Float64)   # Number
typejoin(Int64, Int64)     # Int64
typejoin(Int64, String)    # Any
typejoin(Bool, UInt8)      # Integer
```
"""
function typejoin(a::Type, b::Type)
    a === b && return a

    # `Union{}` (Bottom) is the identity element: it is a subtype of every type,
    # so `typejoin(T, Union{}) === T` and `typejoin(Union{}, T) === T`. Without
    # this guard Bottom has no supertype chain reaching a common ancestor and the
    # walk below falls through to `Any` (Issue #5556). This identity is what makes
    # `promote_typejoin(Int, Nothing) === Union{Nothing, Int64}` hold (Issue #5113).
    a === Union{} && return b
    b === Union{} && return a

    # Two Tuple types: join elementwise over their parameters and rebuild a
    # Tuple type from the joined element types (Issue #5112). Unequal fixed
    # lengths widen the longer tail to `Vararg{tail_join}`, matching upstream's
    # prefix-preserving tuple join shape (Issue #8425).
    a_is_tuple = a <: Tuple
    b_is_tuple = b <: Tuple
    if a_is_tuple || b_is_tuple
        if !(a_is_tuple && b_is_tuple)
            return Any
        end
        ap = a.parameters
        bp = b.parameters
        joined = Any[]
        common = min(length(ap), length(bp))
        for i in 1:common
            push!(joined, typejoin(ap[i], bp[i]))
        end
        if length(ap) != length(bp)
            tail_params = length(ap) > length(bp) ? ap : bp
            tail = _typejoin_range(tail_params, common + 1)
            push!(joined, Vararg{tail})
        end
        return _typejoin_tuple_from_args(joined)
    end

    if typename(a) === :Array && typename(b) === :Array
        ap = a.parameters
        bp = b.parameters
        if length(ap) >= 2 && length(bp) >= 2
            a_el = ap[1]
            b_el = bp[1]
            a_rank = ap[2]
            b_rank = bp[2]
            if a_rank === b_rank
                a_el === b_el && return a
                return Core.apply_type(a)
            end
            a_el === b_el && return Core.apply_type(Array, a_el)
            return Array
        end
    end

    # Same-name parametric types (e.g. `Box{Int}` and `Box{Float64}`): join
    # their type parameters elementwise. If every joined parameter matches both
    # inputs the instantiation is preserved; otherwise the parameters differ and
    # we widen to the base type, matching `typejoin(Box{Int}, Box{Float64})`
    # collapsing to `Box` (Issue #5112).
    if typename(a) === typename(b)
        ap = a.parameters
        bp = b.parameters
        if length(ap) == length(bp) && length(ap) > 0
            joined = Any[]
            same = true
            for i in 1:length(ap)
                if ap[i] isa Type && bp[i] isa Type
                    ji = typejoin(ap[i], bp[i])
                elseif ap[i] === bp[i]
                    ji = ap[i]
                else
                    same = false
                    continue
                end
                if !(ji === ap[i] && ji === bp[i])
                    same = false
                end
                push!(joined, ji)
            end
            if same
                return a
            end
            # Parameters diverged: widen to the bare base type (the wrapper).
            return Core.apply_type(a)
        end
    end

    # Build supertype chain for a
    chain_a = DataType[]
    current = a
    while true
        push!(chain_a, current)
        current === Any && break
        current = supertype(current)
    end
    # Walk b's chain and find first match in a's chain
    current = b
    while true
        for t in chain_a
            t === current && return current
        end
        current === Any && break
        current = supertype(current)
    end
    return Any
end

"""
    typename(T::Type) -> Core.TypeName-equivalent name symbol

Internal: resolve a (potentially `UnionAll`-wrapped, parametric, or aliased)
type to its canonical `TypeName` symbol (Issue #5106).

Unlike upstream `Base.typename`, which returns a `Core.TypeName` object,
SubsetJuliaVM models the `TypeName` solely by its canonical base-name symbol.
All instantiations of a type share that symbol, so

    typename(Foo{Int}) === typename(Foo)
    typename(Vector{Int}) === typename(Array) === :Array

`Base.typename` is *not* exported, mirroring upstream.
"""
function typename(T::Type)
    _typename(T)
end

"""
    nameof(t::Type) -> Symbol
    nameof(f::Function) -> Symbol

Get the name of a type or function as a Symbol.

For a (potentially `UnionAll`-wrapped) type, returns the canonical `TypeName`
symbol without type parameters. Base display aliases collapse onto the shared
underlying `TypeName`, so `nameof(Vector)`, `nameof(Vector{Int})` and
`nameof(Matrix)` all return `:Array` — matching upstream Julia (Issue #5106).

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

nameof(Point)         # :Point
nameof(Int64)         # :Int64
nameof(Vector{Int64}) # :Array
nameof(Dict)          # :Dict
nameof(sin)           # :sin
```
"""
function nameof(t::Type)
    typename(t)
end

function nameof(f::Function)
    _function_name(f)
end

# Reflection data structure for method introspection
# Simplified version of Julia's Base.Method type

"""
    Method

Represents a method definition for a generic function.
Contains the method name, signature (tuple of parameter types), and argument count.
"""
struct Method
    name::Symbol         # Function name as symbol
    sig::Tuple           # Parameter types as tuple of DataType
    nargs::Int32         # Argument count including the function object (Issue #4989)
    return_type::Type    # Inferred or declared return type snapshot
    # Source-location reflection fields (Issue #5125). Upstream `Method` exposes
    # `.module::Module`, `.file::Symbol`, and `.line::Int32`; `methods(f)`
    # listings and `show(::Method)` read these to render
    # `name(args) @ Module file:line`. SubsetJuliaVM models the defining module
    # as `Main` for top-level user definitions and recovers `line` from the
    # source map of the matched method's entry instruction (`file` falls back to
    # a representative symbol when the original path is unavailable).
    #
    # NOTE: `module` is a reserved keyword the parser cannot accept as a field
    # name, so this field is *declared* as `mod` here and renamed to `module` in
    # the VM struct-definition table at startup (see `normalize_method_struct_def`
    # in src/vm/mod.rs). User code accesses it as `m.module`, matching upstream;
    # the placeholder name is never user-visible.
    mod::Module
    file::Symbol
    line::Int32
    # Representative retained constant-propagation metadata (Issue #4978):
    # 0 = default, 1 = Base.@constprop :aggressive, 2 = Base.@constprop :none.
    constprop::UInt8
    # Representative retained inline metadata, mirrored into CodeInfo.inlining
    # (Issues #4977/#4980): 0 = default, 1 = @inline / @propagate_inbounds,
    # 2 = @noinline.
    inlining::UInt8
    # Representative `@nospecialize` bitmask over explicit positional
    # parameters; statement-position `@nospecialize a b` sets the matching
    # bits, a trailing `@specialize` clears them (Issue #4984).
    nospecialize::Int32
    # True when the matched method is varargs (mirrors `CodeInfo.isva`,
    # Issue #4983).
    isva::Bool
    # Representative `Base.@propagate_inbounds` metadata, mirrored into
    # `CodeInfo.propagate_inbounds` (Issue #4979).
    propagate_inbounds::Bool
    # Representative `Base.@nospecializeinfer` metadata, mirrored into
    # `CodeInfo.nospecializeinfer` (Issue #4979).
    nospecializeinfer::Bool
    # Representative `Base.@assume_effects` purity bitmask, mirrored into
    # `CodeInfo.purity` (Issue #4983).
    purity::UInt16
end

# Representative CodeInfo-like record for reflection APIs.
#
# Official Julia returns `Core.CodeInfo` from `code_lowered` and the first field
# of each `code_typed` pair. sjulia does not yet materialize full compiler IR
# here, but returning a structured record is more faithful than the older
# `nothing` placeholder and gives users the matched Method plus return snapshot.
struct CodeInfo
    method::Method
    rettype::Type
    inferred::Bool
    code::Any
    # Representative retained inline metadata (Issues #4977/#4980):
    # 0 = default, 1 = @inline / @propagate_inbounds, 2 = @noinline.
    # Stored untyped so the UInt8 value carried from Method.inlining keeps its
    # runtime type (sjulia does not yet coerce annotated struct fields).
    inlining
    # Representative retained constant-propagation metadata (Issue #4981):
    # 0 = default, 1 = aggressive, 2 = none.
    constprop
    # Argument count including the function object. Upstream `Core.CodeInfo`
    # uses `UInt64` (Issues #4989/#4983).
    nargs::UInt64
    # Representative `Base.@assume_effects` purity bitmask
    # (encode_effects_override value); 0 = default (Issue #4983).
    purity::UInt16
    # Representative inlining cost heuristic. Upstream reports `UInt16(65535)`
    # for lowered IR, `UInt16(10)` for ordinary inlineable typed methods, and
    # `UInt16(65535)` for `@noinline` typed methods (Issues #4982/#4983).
    inlining_cost::UInt16
    # Whether the IR performs a foreigncall. Representative methods report
    # `false` (Issue #4983).
    has_fcall::Bool
    # Whether the IR references an image global. Representative no-global
    # methods report `false` (Issue #4983).
    has_image_globalref::Bool
    # Whether the matched method is varargs (Issue #4983).
    isva::Bool
    # Representative `Base.@propagate_inbounds` metadata (Issue #4979).
    propagate_inbounds::Bool
    # Representative `Base.@nospecializeinfer` metadata (Issue #4979).
    nospecializeinfer::Bool
end

function _representative_codeinfo_code(m::Method, rettype, inferred::Bool)
    code = Any[]
    push!(code, Expr(:call, inferred ? Symbol("invoke") : m.name, m.name))
    push!(code, Expr(:return, rettype))
    code
end

# Base.show method for Method
#
# Renders a method the upstream way (Issue #5125):
#
#     name(::T1, ::T2) @ Module file:line
#
# The signature portion lists the explicit positional parameter types, and the
# trailing ` @ Module file:line` reports the defining module and source
# location. `methods(f)` listings, `println(m)`, `string(m)`, and `repr(m)` all
# flow through this method.
function Base.show(io::IO, m::Method)
    print(io, m.name)
    print(io, "(")
    # m.nargs includes the function object (Issue #4989), so the number of
    # explicit positional arguments is m.nargs - 1.
    nparams = m.nargs - 1
    for i in 1:nparams
        if i > 1
            print(io, ", ")
        end
        if i <= length(m.sig)
            print(io, "::", m.sig[i])
        else
            print(io, "::Any")
        end
    end
    print(io, ")")
    # Source-location suffix: ` @ Module file:line` (Issue #5125).
    print(io, " @ ", m.module, " ", m.file, ":", m.line)
end

function methods(f)
    _methods_by_ftype(f)
end

function methods(f, types)
    _methods_by_ftype(f, types)
end

# `return_types` accepts upstream's `world` / `interp` keyword arguments (plus a
# `kwargs...` collector for forward-compatibility). SubsetJuliaVM has a single
# inference snapshot, so `world` is accepted but does not change the result;
# rejecting it would be wrong now that unknown keyword arguments error
# (Issue #5121). The `kwargs...` collector also keeps these methods accepting
# any other keyword upstream may add without raising.
function return_types(f; world=nothing, interp=nothing, kwargs...)
    ms = methods(f)
    rts = Any[]
    # Use indexed traversal: `ms` can be inferred as Any in Base reflection
    # callers, and `for m in ms` would route through iterate(::Any) instead of
    # the native-array iterator (Issue #5584).
    i = 1
    while i <= length(ms)
        push!(rts, ms[i].return_type)
        i += 1
    end
    rts
end

function return_types(f, types; world=nothing, interp=nothing, kwargs...)
    _return_types_by_ftype(f, types)
end

function infer_return_type(f)
    rts = return_types(f)
    if length(rts) == 0
        return Union{}
    end
    if length(rts) <= 4
        return _type_union(rts...)
    end
    rt = rts[1]
    for i in 2:length(rts)
        rt = typejoin(rt, rts[i])
    end
    rt
end

function infer_return_type(f, types)
    rts = return_types(f, types)
    if length(rts) == 0
        return Union{}
    end
    if length(rts) <= 4
        return _type_union(rts...)
    end
    rt = rts[1]
    for i in 2:length(rts)
        rt = typejoin(rt, rts[i])
    end
    rt
end

# Representative effect-inference surface for `Base.infer_effects` /
# `Base.infer_exception_type` (Issue #4274).
#
# Upstream Julia carries a full `Compiler.Effects` record (return type, exception
# type, effects, and call metadata) through interprocedural inference. sjulia
# does not yet run that engine from the pure-Julia reflection surface, so this
# `Effects` mirror reproduces the upstream field layout, semantics, and custom
# `show` exactly while seeding values from the conservatively *proven* result
# for the matched methods. Simple total user methods (and many builtin
# arithmetic calls) infer to the all-true representative, matching upstream.
#
# Field encoding mirrors `julia/Compiler/src/effects.jl`:
#   * UInt8 bitfields use ALWAYS_TRUE (0x00) / ALWAYS_FALSE (0x01) / refinement
#     (0x02) states.
#   * Bool fields use true / false directly.
struct Effects
    consistent::UInt8
    effect_free::UInt8
    nothrow::Bool
    terminates::Bool
    notaskstate::Bool
    inaccessiblememonly::UInt8
    noub::UInt8
    nonoverlayed::UInt8
    nortcall::Bool
end

# `EFFECTS_TOTAL`: every property proven (mirrors upstream
# `julia/Compiler/src/effects.jl` `EFFECTS_TOTAL`).
_effects_total() = Effects(0x00, 0x00, true, true, true, 0x00, 0x00, 0x00, true)

# Representative `Effects` records used by the per-signature classification table
# below (Issues #4972, #4957, #4991). Each record reproduces the exact field
# layout upstream Julia 1.12 infers for the matched signature; UInt8 bitfields
# use 0x00 = ALWAYS_TRUE (`+`), 0x01 = ALWAYS_FALSE (`!`), 0x02 = refinement
# (`?`), and Bool fields use true (`+`) / false (`!`).

# Array allocation helpers `fill` / `zeros`: `(!c,+e,!n,!t,+s,+m,!u,+o,+r)`.
_effects_array_alloc() = Effects(0x01, 0x00, false, false, true, 0x00, 0x01, 0x00, true)
# Shape helpers `reshape` / `vec`: `(?c,+e,!n,+t,+s,?m,+u,+o,+r)`.
_effects_reshape() = Effects(0x02, 0x00, false, true, true, 0x02, 0x00, 0x00, true)
# In-place `fill!`: `(!c,?e,!n,!t,+s,?m,!u,+o,+r)`.
_effects_fill_bang() = Effects(0x01, 0x02, false, false, true, 0x02, 0x01, 0x00, true)
# `insert!`: `(!c,!e,!n,+t,+s,!m,+u,+o,!r)`.
_effects_insert() = Effects(0x01, 0x01, false, true, true, 0x01, 0x00, 0x00, false)
# Fully imprecise (`splice!`, `which`, `methods`): `(!c,!e,!n,!t,!s,!m,!u,!o,!r)`.
_effects_all_false() = Effects(0x01, 0x01, false, false, false, 0x01, 0x01, 0x01, false)
# `applicable`: `(!c,!e,!n,+t,+s,!m,+u,+o,+r)`.
_effects_applicable() = Effects(0x01, 0x01, false, true, true, 0x01, 0x00, 0x00, true)
# Consistent-but-throwing (`fieldoffset`, `Int64`/`Bool` constructors):
# `(+c,+e,!n,+t,+s,+m,+u,+o,+r)`.
_effects_consistent_throws() = Effects(0x00, 0x00, false, true, true, 0x00, 0x00, 0x00, true)

# `EFFECTS_UNKNOWN` (mirrors upstream `julia/Compiler/src/effects.jl`
# `EFFECTS_UNKNOWN`): inference proved nothing except that the call is not an
# overlayed method, i.e. `(!c,!e,!n,!t,!s,!m,!u,+o,!r)`. This is the *honest*
# result upstream reports for the many public string / parse / search helpers
# that have no `@assume_effects` annotation and whose bodies inference cannot
# refine — distinct from sjulia's accidental proven-total fallback, and distinct
# from `_effects_all_false()` (which also lowers `nonoverlayed`, used by
# `which`/`methods`/`splice!`). Issues #4968 / #4969 / #4971.
_effects_unknown() = Effects(0x01, 0x01, false, false, false, 0x01, 0x01, 0x00, false)
# `lstrip(::AbstractString)`: effect-free + noub but otherwise imprecise:
# `(!c,+e,!n,!t,!s,!m,+u,+o,+r)` (Issue #4968).
_effects_lstrip_string() = Effects(0x01, 0x00, false, false, false, 0x01, 0x00, 0x00, true)
# `repeat(::AbstractString, ::Integer)`: consistent + effect-free, terminates,
# but throwing and not task-state / inaccessible-mem proven:
# `(+c,+e,!n,+t,!s,!m,+u,+o,+r)` (Issue #4968).
_effects_repeat_string() = Effects(0x00, 0x00, false, true, false, 0x01, 0x00, 0x00, true)
# `string(::Char)`: total except `notaskstate` / `inaccessiblememonly` are not
# proven: `(+c,+e,+n,+t,!s,!m,+u,+o,+r)` (Issue #4969).
_effects_string_char() = Effects(0x00, 0x00, true, true, false, 0x01, 0x00, 0x00, true)
# `thisind`/`nextind`/`prevind`(::AbstractString, ::Integer) index helpers:
# `(!c,+e,!n,+t,!s,!m,+u,+o,+r)` (Issue #4971).
_effects_strindex() = Effects(0x01, 0x00, false, true, false, 0x01, 0x00, 0x00, true)
# `eachindex(::AbstractVector)`: consistent-if-inaccessiblememonly (`?c`, 0x04),
# effect-free, nothrow, terminates, inaccessiblemem-or-argmemonly (`?m`, 0x02):
# `(?c,+e,+n,+t,+s,?m,+u,+o,+r)` (Issue #4974).
_effects_eachindex_vector() = Effects(0x04, 0x00, true, true, true, 0x02, 0x00, 0x00, true)
# `collect(::AbstractRange)` (e.g. `OneTo`): not consistent / not nothrow / may
# not terminate, but effect-free and inaccessible-mem proven, allocates so `!u`:
# `(!c,+e,!n,!t,+s,+m,!u,+o,+r)` (Issue #4974).
_effects_collect_range() = Effects(0x01, 0x00, false, false, true, 0x00, 0x01, 0x00, true)
# `getindex(::Pair, ::Integer)`: total except `nothrow` (an out-of-range pair
# index throws): `(+c,+e,!n,+t,+s,+m,+u,+o,+r)` — same shape as
# `_effects_consistent_throws()` but kept named for the pair-helper site
# (Issue #4974).
_effects_pair_getindex() = _effects_consistent_throws()

# Resolve the canonical name used to key the classification table. `nameof`
# already handles both ordinary function values and DataType callables
# (constructors such as `Int64`), keying the latter by their type name. Routing
# constructors through this name-based lookup is what lets `infer_effects` /
# `infer_exception_type` classify them without taking the `methods(::DataType, …)`
# path, which has no matching dispatch in the subset (Issue #4991).
_effect_class_name(f) = nameof(f)

# --- Core builtin effect categories (Issue #4274) -------------------------
#
# Upstream `julia/Compiler/src/tfuncs.jl` does NOT classify Core builtins with
# ad-hoc per-name `Effects` records. Instead `builtin_effects` *composes* the
# record from the builtin's membership in a handful of semantic category sets
# (`_PURE_BUILTINS`, `_CONSISTENT_BUILTINS`, `_EFFECT_FREE_BUILTINS`,
# `_INACCESSIBLEMEM_BUILTINS`, `_ARGMEM_BUILTINS`) plus a per-call `nothrow`
# decision. This block mirrors that composition for the subset of Core builtins
# sjulia can actually reflect over as first-class function values, so their
# effect/exception metadata is grounded in builtin *semantics* rather than the
# accidental proven-total fallback (the gap #4274 calls out, e.g. `fieldtype`
# tainting `nothrow`).
#
# Category symbols mirror the upstream constant lists in tfuncs.jl. Only the
# reflectable subset is listed; unlisted builtins fall through to the existing
# name-based tables / proven-total default, so this strictly refines behavior.

# Mirrors upstream `_PURE_BUILTINS` (∩ reflectable subset).
_is_pure_builtin(name::Symbol) =
    name === :tuple || name === :typeof || name === :nfields

# Mirrors upstream `_CONSISTENT_BUILTINS` (∩ reflectable subset). Pure builtins
# are a subset of the consistent builtins upstream, so they are included here.
_is_consistent_builtin(name::Symbol) =
    _is_pure_builtin(name) || name === :isa || name === :fieldtype ||
    name === :typeassert || name === :sizeof || name === :ifelse

# Mirrors upstream `_EFFECT_FREE_BUILTINS` (∩ reflectable subset). Note `isa`,
# `fieldtype`, `typeassert`, `sizeof`, `ifelse` are effect-free upstream; the
# pure builtins are effect-free as well.
_is_effect_free_builtin(name::Symbol) =
    _is_pure_builtin(name) || name === :isa || name === :fieldtype ||
    name === :typeassert || name === :sizeof || name === :ifelse

# Mirrors upstream `_INACCESSIBLEMEM_BUILTINS` (∩ reflectable subset): builtins
# that touch no externally accessible mutable memory (ALWAYS_TRUE / `+m`).
_is_inaccessiblemem_builtin(name::Symbol) =
    name === :tuple || name === :typeof || name === :nfields || name === :isa ||
    name === :fieldtype || name === :typeassert || name === :sizeof ||
    name === :ifelse

# Whether sjulia reflects this name as a Core builtin handled by the category
# composition. Constructors and the documented public-helper tables are
# intentionally excluded so they keep their existing dedicated classification.
_is_known_core_builtin(name::Symbol) =
    _is_consistent_builtin(name) || _is_inaccessiblemem_builtin(name)

# Per-call `nothrow` decision for the reflectable Core builtins, mirroring the
# relevant arms of upstream `builtin_nothrow`. `tuple` never throws. `typeof`,
# `nfields`, `===`, `isa`, `typeassert`, `sizeof`, and `ifelse` are nothrow for
# the well-typed concrete signatures sjulia reflects over. `fieldtype` may throw
# (e.g. an out-of-range field index), so it taints `nothrow` exactly as upstream.
function _core_builtin_nothrow(name::Symbol, types)
    if name === :fieldtype
        return false
    end
    true
end

# Compose the `Effects` record for a reflectable Core builtin from its category
# memberships, mirroring upstream `builtin_effects`. UInt8 bitfields use 0x00 =
# ALWAYS_TRUE (`+`) / 0x01 = ALWAYS_FALSE (`!`); Bool fields use true/false.
function _classify_builtin_effects(name::Symbol, types)
    consistent = _is_consistent_builtin(name) ? 0x00 : 0x01
    effect_free = _is_effect_free_builtin(name) ? 0x00 : 0x01
    nothrow = _core_builtin_nothrow(name, types)
    inaccessiblememonly = _is_inaccessiblemem_builtin(name) ? 0x00 : 0x01
    # The reflectable subset performs no undefined behavior, uses no overlay
    # tables, accesses no task state, makes no runtime call, and terminates.
    Effects(consistent, effect_free, nothrow, true, true, inaccessiblememonly,
        0x00, 0x00, true)
end

# Inferred exception type for a reflectable Core builtin, mirroring upstream
# `builtin_exct`: non-intrinsic builtins surface `Any` whenever they may throw,
# and `Union{}` (no exception) when proven `nothrow`.
function _classify_builtin_exception_type(name::Symbol, types)
    _core_builtin_nothrow(name, types) ? Union{} : Any
end

# Number of positional argument slots described by a `types` signature tuple
# type. Returns -1 when the parameter count cannot be determined.
function _effect_sig_nparams(types)
    if isa(types, Type)
        ps = types.parameters
        return length(ps)
    end
    -1
end

# --- Per-signature public helper classification (Issues #4968/#4969/#4971/#4974)
#
# Upstream Julia infers these public Base helpers *interprocedurally*: most string
# / parse / search helpers have no `@assume_effects` annotation, so inference
# refines nothing and reports `EFFECTS_UNKNOWN`, while a handful expose more
# precise records (e.g. `repeat(::String,::Int)`, `string(::Char)`, the
# `*ind` index helpers) and the tuple/range helpers resolve to total or a small
# named record. sjulia cannot yet run that engine from the reflection surface, so
# this block reproduces the exact upstream record for the *representative*
# signatures each issue calls out, keyed by name AND argument type so it never
# intercepts a same-named overload with different effects (e.g. `count` /
# `findfirst` / `repeat` / `replace` / `getindex` over non-string/non-pair args,
# or `first`/`last`/`length` over Vectors — all of which differ upstream and must
# keep falling through). Values verified field-for-field against Julia 1.12.6.
#
# Returns `nothing` when the signature is not one of the classified
# representatives, so callers fall through to the existing tables / proven-total
# default and nothing regresses.

# True when `t` is a usable concrete-ish argument type (not a free TypeVar etc.).
_helper_is_type(t) = isa(t, Type)

# Whether the first positional argument of `types` is a subtype of `super`.
function _helper_arg1_subtype(types, super)
    ts = _signature_param_types(types)
    length(ts) >= 1 && _helper_is_type(ts[1]) && ts[1] <: super
end

# Whether every positional argument type in `ts` is a concrete-ish `Integer`
# subtype (`Bool` is an `Integer` upstream, so it is included). Used to gate the
# bitwise / shift integer-op classification below so non-integer overloads keep
# falling through (Issue #4274).
function _helper_all_integer_args(ts)
    isempty(ts) && return false
    for t in ts
        (_helper_is_type(t) && t <: Integer) || return false
    end
    true
end

# Fixed-width *signed* integers (`Int8`…`Int128`): `gcd` can overflow at
# `abs(typemin)` (`OverflowError`). Excludes unsigned types (no negation
# overflow) and `BigInt` (arbitrary precision), matching upstream
# `Base.infer_exception_type(gcd, Tuple{T,T})` (Issue #6272).
_is_signed_machine_int(t) = _helper_is_type(t) && t <: Signed && t !== BigInt
# Fixed-width integers (signed or unsigned): `lcm`'s product can overflow and
# its `÷ gcd` step can divide by zero. Excludes `BigInt` and `Bool` (Issue #6272).
_is_machine_int(t) = _helper_is_type(t) && t <: Integer && t !== BigInt && t !== Bool

# Effects for the representative string / parse / search / tuple / range helpers.
# Returns `nothing` unless name + signature matches a classified representative.
function _classify_helper_effects(name::Symbol, types)
    ts = _signature_param_types(types)
    n = length(ts)
    # --- #4968 string transformation helpers (single AbstractString arg) ---
    if (name === :uppercase || name === :lowercase || name === :titlecase ||
        name === :strip || name === :rstrip || name === :chomp || name === :chop ||
        name === :split) && n >= 1 && _helper_is_type(ts[1]) && ts[1] <: AbstractString
        return _effects_unknown()
    elseif name === :lstrip && n >= 1 && _helper_is_type(ts[1]) && ts[1] <: AbstractString
        return _effects_lstrip_string()
    elseif name === :join && n >= 1 && _helper_is_type(ts[1]) && ts[1] <: AbstractVector
        return _effects_unknown()
    elseif name === :repeat && n == 2 && _helper_is_type(ts[1]) && ts[1] <: AbstractString &&
           _helper_is_type(ts[2]) && ts[2] <: Integer
        return _effects_repeat_string()
        # --- #4969 parse / string-conversion helpers ---
    elseif (name === :parse || name === :tryparse) && n >= 2 && _helper_is_type(ts[2]) &&
           ts[2] <: AbstractString
        return _effects_unknown()
    elseif (name === :bitstring || name === :unescape_string || name === :repr) && n == 1
        return _effects_unknown()
    elseif name === :string && n == 1 && _helper_is_type(ts[1]) && ts[1] === Char
        return _effects_string_char()
        # --- #4971 string search / index helpers ---
    elseif (name === :findfirst || name === :findlast) && n == 2 &&
           _helper_is_type(ts[2]) && ts[2] <: AbstractString
        return _effects_unknown()
    elseif (name === :findnext || name === :findprev) && n == 3 &&
           _helper_is_type(ts[1]) && ts[1] <: AbstractString &&
           _helper_is_type(ts[2]) && ts[2] <: AbstractString
        return _effects_unknown()
    elseif name === :count && n == 2 && _helper_is_type(ts[1]) && ts[1] <: AbstractString &&
           _helper_is_type(ts[2]) && ts[2] <: AbstractString
        return _effects_unknown()
    elseif (name === :thisind || name === :nextind || name === :prevind) && n == 2 &&
           _helper_is_type(ts[1]) && ts[1] <: AbstractString &&
           _helper_is_type(ts[2]) && ts[2] <: Integer
        # `thisind`/`nextind` are `(!c,+e,!n,+t,!s,!m,+u,+o,+r)`; `prevind` is
        # fully imprecise upstream.
        return name === :prevind ? _effects_unknown() : _effects_strindex()
    elseif name === :replace && n == 2 && _helper_is_type(ts[1]) && ts[1] <: AbstractString
        return _effects_unknown()
        # --- #4974 tuple / pair / range helpers ---
    elseif (name === :first || name === :last || name === :length ||
            name === :isempty || name === :reverse || name === :only) &&
           n == 1 && _helper_is_type(ts[1]) && ts[1] <: Tuple
        return _effects_total()
    elseif name === :getindex && n == 2 && _helper_is_type(ts[1]) && ts[1] <: Pair
        return _effects_pair_getindex()
    elseif name === :eachindex && n == 1 && _helper_is_type(ts[1]) && ts[1] <: AbstractVector
        return _effects_eachindex_vector()
    elseif name === :collect && n == 1 && _helper_is_type(ts[1]) && ts[1] <: AbstractRange
        return _effects_collect_range()
        # --- #4274 bitwise / shift integer operations ---
        # Upstream infers `EFFECTS_TOTAL` / `Union{}` for the bit-manipulation
        # helpers over `Integer` arguments: they wrap `Base.*_int` /
        # `Core.Intrinsics` shift intrinsics, perform no memory access, never
        # throw, and are consistent + effect-free. Keyed by name + integer arg
        # types so non-integer overloads (e.g. `&`/`|`/`xor`/`~` on `Bool`-array
        # / `Missing`, `<<`/`>>` on `BitVector`) keep falling through. The named
        # bit-count helpers (`count_ones`, `leading_zeros`, `bitrotate`, …) are
        # not yet reflectable as first-class function values in the subset
        # (Issue #5333: bare identifier raises `UndefVarError`), so only the
        # binary/unary operators and `xor` are exercised today; those names are
        # included here so the classification is ready when the function-value
        # surface is extended.
    elseif (name === :count_ones || name === :count_zeros ||
            name === :leading_zeros || name === :trailing_zeros ||
            name === :leading_ones || name === :trailing_ones) &&
           n == 1 && _helper_all_integer_args(ts)
        return _effects_total()
    elseif (name === :xor || name === :| || name === :&) &&
           n == 2 && _helper_all_integer_args(ts)
        return _effects_total()
    elseif name === :~ && n == 1 && _helper_all_integer_args(ts)
        return _effects_total()
    elseif (name === :<< || name === :>> || name === :>>>) &&
           n == 2 && _helper_all_integer_args(ts)
        return _effects_total()
    elseif name === :bitrotate && n == 2 && _helper_all_integer_args(ts)
        return _effects_total()
    end
    nothing
end

# Inferred exception type for the representative helpers above. The
# string/parse/search helpers all surface `Any`; the tuple helpers surface
# `Union{}`; `getindex(::Pair,::Integer)` and `collect(::AbstractRange)` surface
# `Any`; `string(::Char)` surfaces `Union{}`.
function _classify_helper_exception_type(name::Symbol, types)
    ef = _classify_helper_effects(name, types)
    ef === nothing && return nothing
    # A helper proven `nothrow` surfaces no exception (`Union{}`), exactly like
    # upstream `Base.infer_exception_type` (e.g. `string(::Char)`, the total
    # tuple helpers, and `eachindex(::AbstractVector)`). Any helper that may throw
    # surfaces `Any` here, since sjulia does not yet track per-helper exception
    # unions for these public functions.
    ef.nothrow ? Union{} : Any
end

# Per-signature `Effects` classification (Issues #4972 / #4957 / #4991). Returns
# `nothing` when the call is not in the representative table so callers fall back
# to the proven-total representative.
function _classify_effects(name::Symbol, types)
    # Core builtins are classified by category composition first (Issue #4274),
    # so their effects are grounded in builtin semantics rather than the
    # accidental proven-total fallback.
    if _is_known_core_builtin(name)
        return _classify_builtin_effects(name, types)
    elseif name === :fill || name === :zeros
        return _effects_array_alloc()
    elseif name === :reshape || name === :vec
        return _effects_reshape()
    elseif name === :fill!
        return _effects_fill_bang()
    elseif name === :insert!
        return _effects_insert()
    elseif name === :splice! || name === :which || name === :methods
        return _effects_all_false()
    elseif name === :applicable
        return _effects_applicable()
    elseif name === :fieldoffset
        return _effects_consistent_throws()
    elseif name === :typejoin || name === :typeintersect
        return _effects_total()
    elseif name === :Int64 || name === :Bool
        return _effects_consistent_throws()
    elseif name === :Float64
        return _effects_total()
    end
    # Public string / parse / search / tuple / range helpers (Issues
    # #4968/#4969/#4971/#4974) are classified by name + signature; returns
    # `nothing` for non-matching overloads so they fall through unchanged.
    _classify_helper_effects(name, types)
end

# Per-signature inferred exception type classification (Issues #4972 / #4957 /
# #4991). Returns the inferred exception `Type`, or `nothing` when the call is
# not in the representative table.
function _classify_exception_type(name::Symbol, types)
    # Core builtins are classified by category composition first (Issue #4274):
    # `Union{}` when proven nothrow, `Any` when the builtin may throw, mirroring
    # upstream `builtin_exct`.
    if _is_known_core_builtin(name)
        return _classify_builtin_exception_type(name, types)
    elseif name === :fill || name === :zeros || name === :insert! || name === :splice! ||
       name === :applicable || name === :which || name === :methods
        return Any
    elseif name === :reshape
        return Union{DimensionMismatch,ArgumentError}
    elseif name === :vec
        return DimensionMismatch
    elseif name === :fill!
        return BoundsError
    elseif name === :fieldoffset
        # The `Symbol` index form has no matching method upstream, so the
        # inferred exception type is `MethodError`; the integer index form has
        # no inferred exception (`Any`).
        n = _effect_sig_nparams(types)
        if n >= 2 && types.parameters[2] === Symbol
            return MethodError
        end
        return Any
    elseif name === :typejoin || name === :typeintersect
        return Union{}
    elseif name === :Int64 || name === :Bool
        return InexactError
    elseif name === :Float64
        return Union{}
    end
    # Public helper exception types (Issues #4968/#4969/#4971/#4974); returns
    # `nothing` for non-matching overloads.
    _classify_helper_exception_type(name, types)
end

# Render the single effect-bit letter, matching `effectbits_letter` in
# `julia/Compiler/src/ssair/show.jl`. UInt8 bitfields print `+` for ALWAYS_TRUE
# (0x00), `!` for ALWAYS_FALSE (0x01), and `?` for any refinement state; Bool
# fields print `+` for true and `!` for false. Branch on the runtime value
# rather than dispatching on the field type so the helper stays a single
# untyped method (sjulia does not yet narrow struct-field types at the call
# site for static dispatch).
function _effectbits_letter(value, suffix)
    if isa(value, Bool)
        prefix = value ? '+' : '!'
    else
        prefix = value == 0 ? '+' : (value == 1 ? '!' : '?')
    end
    string(prefix, suffix)
end

function Base.show(io::IO, e::Effects)
    print(io, "(")
    print(io, _effectbits_letter(e.consistent, 'c'))
    print(io, ",")
    print(io, _effectbits_letter(e.effect_free, 'e'))
    print(io, ",")
    print(io, _effectbits_letter(e.nothrow, 'n'))
    print(io, ",")
    print(io, _effectbits_letter(e.terminates, 't'))
    print(io, ",")
    print(io, _effectbits_letter(e.notaskstate, 's'))
    print(io, ",")
    print(io, _effectbits_letter(e.inaccessiblememonly, 'm'))
    print(io, ",")
    print(io, _effectbits_letter(e.noub, 'u'))
    print(io, ",")
    print(io, _effectbits_letter(e.nonoverlayed, 'o'))
    print(io, ",")
    print(io, _effectbits_letter(e.nortcall, 'r'))
    print(io, ")")
end

# `EFFECTS_TOTAL` with only `nothrow` lowered to false. Throwing-but-otherwise
# total helpers (e.g. `sin`, `divrem`) report this representative upstream.
_effects_total_throws() = Effects(0x00, 0x00, false, true, true, 0x00, 0x00, 0x00, true)

# Extract the positional parameter types from a `Tuple` type signature, returning
# an empty vector for the bare `Tuple` (zero-argument filter) or any value we
# cannot introspect. Keeps the per-signature classifier below total.
function _signature_param_types(types)
    params = try
        types.parameters
    catch
        return Any[]
    end
    result = Any[]
    for p in params
        push!(result, p)
    end
    result
end

# Per-signature inferred exception type for the documented throwing helpers
# (Issue #4970). Returns `nothing` when the signature is not specially classified
# so callers fall back to the proven-total `Union{}` default. Values mirror
# upstream Julia 1.12 `Base.infer_exception_type`.
function _classified_exception_type(f, types)
    name = try
        nameof(f)
    catch
        return nothing
    end
    ts = _signature_param_types(types)
    if (name === :sin || name === :cos || name === :sqrt) &&
       length(ts) == 1 && ts[1] === Float64
        # Trig/sqrt over Float64 throw `DomainError` (e.g. sqrt of a negative,
        # sin/cos of a non-finite); otherwise total.
        return DomainError
    elseif (name === :log1p || name === :log) && length(ts) == 1 && ts[1] === Float64
        return Union{DomainError,InexactError}
    elseif name === :divrem && length(ts) == 2 && ts[1] === Int64 && ts[2] === Int64
        return DivideError
    elseif (name === :div || name === :rem || name === :mod || name === :fld ||
            name === :cld) && length(ts) == 2 && _helper_all_integer_args(ts)
        # Integer `div`/`rem`/`mod`/`fld`/`cld` throw `DivideError` (division by
        # zero, or `typemin ÷ -1` overflow). Keyed by all-`Integer` argument
        # types so the float overloads (which never throw `DivideError`) and
        # mixed int/float overloads keep falling through to the proven-total
        # representative, matching upstream (Issue #4274).
        return DivideError
    elseif name === :gcd && length(ts) == 2 &&
           _is_signed_machine_int(ts[1]) && _is_signed_machine_int(ts[2])
        # `gcd` over fixed-width signed integers can overflow at `abs(typemin)`;
        # unsigned and `BigInt` args never do, matching upstream (Issue #6272).
        return OverflowError
    elseif name === :lcm && length(ts) == 2 &&
           _is_machine_int(ts[1]) && _is_machine_int(ts[2])
        # `lcm` over any fixed-width integer can overflow in the product, and the
        # `÷ gcd` step can divide by zero (Issue #6272).
        return Union{DivideError,OverflowError}
    elseif (name === :gcd || name === :lcm) && length(ts) == 2 &&
           _helper_is_type(ts[1]) && ts[1] <: Integer &&
           _helper_is_type(ts[2]) && ts[2] <: Integer &&
           (ts[1] === BigInt || ts[2] === BigInt)
        # `gcd`/`lcm` with a `BigInt` argument delegate to GMP via `ccall`, which
        # the inferrer cannot prove `nothrow`; upstream reports `Any` — whether
        # the other argument is `BigInt` or a fixed-width integer that promotes
        # to `BigInt` (Issue #6284).
        return Any
    end
    return nothing
end

"""
    Base.infer_effects(f, types) -> Effects

Return a representative `Effects` record for calling `f` with argument types
`types` (a `Tuple` type). Mirrors upstream `Base.infer_effects` for the subset
of calls sjulia can reflect: simple total methods report the all-true
representative; documented throwing helpers (Issue #4970) report the same
representative with `nothrow` lowered to false. Part of the CallMeta-style
inference surface (Issue #4274).
"""
function infer_effects(f, types)
    # Consult the per-signature classification table first (Issues #4972 / #4957
    # / #4991). This must precede `methods(f, types)` so that DataType callables
    # (e.g. constructors like `Int64`) are classified by name without taking the
    # `methods(::DataType, …)` path, which has no matching dispatch (#4991).
    classified = _classify_effects(_effect_class_name(f), types)
    if classified !== nothing
        return classified
    end
    # Otherwise drive off the matched methods so unknown callables surface a
    # method error exactly as the rest of the reflection surface does. The
    # proven-total representative matches upstream for simple pure user and
    # arithmetic methods (#4274).
    ms = methods(f, types)
    if _classified_exception_type(f, types) !== nothing
        return _effects_total_throws()
    end
    # Interprocedural composition (Issue #5600): when a user function's body can
    # throw (its composed exception type is non-empty), its `nothrow` effect is
    # cleared — mirroring upstream `Effects` propagation from throwing callees.
    if _compose_exception_type(f, types) !== nothing
        return _effects_total_throws()
    end
    _effects_total()
end

function infer_effects(f)
    ms = methods(f)
    _effects_total()
end

"""
    Base.infer_exception_type(f, types) -> Type

Return a representative inferred exception type for calling `f` with argument
types `types`. Simple total methods report `Union{}` (no exception); documented
throwing helpers report their inferred exception type (Issue #4970), matching
upstream Julia. Part of the CallMeta-style inference surface (Issue #4274).
"""
function infer_exception_type(f, types)
    # Consult the per-signature classification table first (Issues #4972 / #4957
    # / #4991), before `methods(f, types)`, so DataType callables are classified
    # without the unsupported `methods(::DataType, …)` path (#4991).
    classified = _classify_exception_type(_effect_class_name(f), types)
    if classified !== nothing
        return classified
    end
    ms = methods(f, types)
    classified = _classified_exception_type(f, types)
    if classified !== nothing
        return classified
    end
    # Interprocedural composition (Issue #5600): a user function's exception type
    # is the union of the exceptions thrown by the operations and callees in its
    # body. `_compose_exception_type` walks the matched method body, so e.g.
    # `f(x) = sqrt(x)` reports `DomainError` rather than the name-table miss
    # `Union{}`. Returns `nothing` when nothing is proven to throw.
    composed = _compose_exception_type(f, types)
    if composed !== nothing
        return composed
    end
    Union{}
end

function infer_exception_type(f)
    ms = methods(f)
    Union{}
end

function code_lowered(f)
    code_lowered(f, Tuple)
end

function _normalize_code_debuginfo(debuginfo)
    if debuginfo === :default
        return :source
    end
    if debuginfo !== :source && debuginfo !== :none
        throw(ArgumentError("'debuginfo' must be either :source or :none"))
    end
    debuginfo
end

function code_lowered(f, types; generated=true, debuginfo=:default, kwargs...)
    if kwargs !== nothing && length(kwargs) != 0
        error("code_lowered does not support these keyword arguments")
    end
    _normalize_code_debuginfo(debuginfo)
    ms = methods(f, types)
    result = Any[]
    # Use indexed traversal for native method arrays; see Issue #5584.
    i = 1
    while i <= length(ms)
        m = ms[i]
        push!(
            result,
            CodeInfo(
                m,
                Any,
                false,
                _representative_codeinfo_code(m, Any, false),
                m.inlining,
                m.constprop,
                UInt64(m.nargs),
                m.purity,
                # Lowered IR uses the sentinel max inlining cost (Issue #4982).
                UInt16(65535),
                false,
                false,
                m.isva,
                m.propagate_inbounds,
                m.nospecializeinfer,
            ),
        )
        i += 1
    end
    result
end

function code_typed(f)
    code_typed(f, Tuple)
end

function code_typed(f, types; optimize=true, debuginfo=:default, world=nothing, interp=nothing, kwargs...)
    if kwargs !== nothing && length(kwargs) != 0
        error("code_typed does not support these keyword arguments")
    end
    if interp !== nothing
        error("Expected AbstractInterpreter")
    end
    _normalize_code_debuginfo(debuginfo)
    ms = methods(f, types)
    result = Any[]
    # Use indexed traversal for native method arrays; see Issue #5584.
    i = 1
    while i <= length(ms)
        m = ms[i]
        # Typed IR reports a representative inlining cost: ordinary methods
        # inline (UInt16(10)) while `@noinline` (inlining == 2) retains the
        # sentinel max cost (Issues #4982/#4983).
        typed_inlining_cost = m.inlining == 0x02 ? UInt16(65535) : UInt16(10)
        push!(
            result,
            Pair(
                CodeInfo(
                    m,
                    m.return_type,
                    true,
                    _representative_codeinfo_code(m, m.return_type, true),
                    m.inlining,
                    m.constprop,
                    UInt64(m.nargs),
                    m.purity,
                    typed_inlining_cost,
                    false,
                    false,
                    m.isva,
                    m.propagate_inbounds,
                    m.nospecializeinfer,
                ),
                m.return_type,
            ),
        )
        i += 1
    end
    result
end

function which(f, types)
    ms = methods(f, types)
    if length(ms) == 0
        error("no matching method")
    end
    ms[1]
end

"""
    applicable(f, args...) -> Bool

Determine whether the given generic function `f` has a method applicable to
the concrete types of the supplied arguments `args`. Mirrors upstream Julia's
`applicable(f, args...)`, which is equivalent to `hasmethod` against the tuple
type of the runtime argument values (Issue #4957).

# Examples
```julia
applicable(sin, 1.0)   # true
applicable(+, 1, "a")  # false
```
"""
function applicable(f, args...)
    hasmethod(f, typeof(args))
end

"""
    hasproperty(x, s::Symbol)

Return a boolean indicating whether the object `x` has `s` as one of its own properties.

!!! compat "Julia 1.2"
     This function requires at least Julia 1.2.

See also: [`propertynames`](@ref), [`hasfield`](@ref).

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
hasproperty(p, :x)  # true
hasproperty(p, :z)  # false
```
"""
function hasproperty(x, s::Symbol)
    # Match upstream: route through `propertynames` so a custom `propertynames`
    # overload is honored (Issue #5101). `propertynames(x)` defaults to
    # `fieldnames(typeof(x))`.
    s in propertynames(x)
end

"""
    isdefined(m::Module, s::Symbol)
    isdefined(object, s::Symbol)

Tests whether a global variable or object field is defined. The arguments can be
a module and a symbol or a composite object and field name (as a symbol).

See also [`@isdefined`](@ref).

# Examples
```julia
julia> isdefined(Base, :sum)
true

julia> isdefined(Base, :NonExistentMethod)
false

julia> struct Point; x::Int; y::Int; end

julia> p = Point(1, 2);

julia> isdefined(p, :x)
true

julia> isdefined(p, :z)
false
```
"""
function isdefined(m::Module, s::Symbol)
    return _isdefined_module_binding(m, s)
end

function isdefined(x, s::Symbol)
    hasfield(typeof(x), s)
end

"""
    getproperty(x, s::Symbol)

Get the value of property `s` from object `x`.
By default, this delegates to `getfield(x, s)`.

Types can override this function to customize property access behavior.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
getproperty(p, :x)  # 1.0
p.x                 # equivalent to getproperty(p, :x)
```
"""
function getproperty(T::Type, f::Symbol)
    if f === :parameters
        return _type_parameters(T)
    end
    getfield(T, f)
end

function getproperty(x, f::Symbol)
    getfield(x, f)
end

"""
    setproperty!(x, s::Symbol, v)

Set the value of property `s` in object `x` to `v`.
By default, this delegates to `setfield!(x, s, convert(fieldtype(typeof(x), s), v))`.

Types can override this function to customize property assignment behavior.

# Examples
```julia
mutable struct MutablePoint
    x::Float64
    y::Float64
end

p = MutablePoint(1.0, 2.0)
setproperty!(p, :x, 3.0)  # sets p.x to 3.0
p.x = 4.0                 # equivalent to setproperty!(p, :x, 4.0)
```
"""
function setproperty!(x, f::Symbol, v)
    ty = fieldtype(typeof(x), f)
    val = isa(v, ty) ? v : convert(ty, v)
    setfield!(x, f, val)
end

"""
    propertynames(x)

Get a tuple of the names (as Symbols) of the properties of object `x`.
By default, this returns `fieldnames(typeof(x))`.

Types can override this function to customize which properties are exposed.

See also: [`hasproperty`](@ref), [`hasfield`](@ref), [`fieldnames`](@ref).

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

p = Point(1.0, 2.0)
propertynames(p)  # (:x, :y)
```
"""
function propertynames(x)
    fieldnames(typeof(x))
end
