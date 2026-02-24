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
    names = fieldnames(T)
    result = if i < 1 || i > length(names)
        throw(BoundsError(names, i))
    else
        # Convert to Symbol if it's a String (VM returns strings)
        n = names[i]
        isa(n, Symbol) ? n : Symbol(n)
    end
    result
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
        # Compare as strings since fieldnames returns Strings
        if fnames[i] == name_str
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
        # Compare as strings since fieldnames returns Strings
        if fnames[i] == name_str
            return i
        end
    end
    throw(ArgumentError("type $(T) has no field named $(name)"))
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
        throw(BoundsError(types, i))
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
    (T === Bool || T === Int8 || T === Int16 || T === Int32 || T === Int64 || T === Int128 || T === UInt8 || T === UInt16 || T === UInt32 || T === UInt64 || T === UInt128 || T === Float16 || T === Float32 || T === Float64 || T === Char)
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
    !isprimitivetype(T) && !isabstracttype(T)
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
    nameof(t::Type) -> Symbol
    nameof(f::Function) -> Symbol

Get the name of a type or function as a Symbol.
For parametric types, returns the base type name without parameters.

# Examples
```julia
struct Point
    x::Float64
    y::Float64
end

nameof(Point)  # :Point
nameof(Int64)  # :Int64
nameof(Vector{Int64})  # :Vector
nameof(sin)  # :sin
```
"""
function nameof(t::Type)
    s = string(t)
    # Strip type parameters if present (e.g., "Vector{Int64}" -> "Vector")
    idx = findfirst("{", s)
    result = if idx !== nothing
        Symbol(s[1:first(idx)-1])
    else
        Symbol(s)
    end
    result
end

function nameof(f::Function)
    # Get function name from string representation
    # Julia functions display as "function name" or just "name"
    s = string(f)
    # The function string format in SubsetJuliaVM is "function name"
    result = if startswith(s, "function ")
        Symbol(s[10:end])
    else
        Symbol(s)
    end
    result
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
    nargs::Int64         # Number of positional arguments
end

# Base.show method for Method
function Base.show(io::IO, m::Method)
    print(io, m.name)
    print(io, "(")
    for i in 1:m.nargs
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
end

"""
    hasproperty(x, s::Symbol)

Return a boolean indicating whether the object `x` has `s` as one of its own properties.

!!! compat "Julia 1.2"
     This function requires at least Julia 1.2.

See also: [`hasfield`](@ref).

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
