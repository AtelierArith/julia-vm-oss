# Test nameof function (Issue #493, Issue #5106)
# Tests that nameof returns the name of functions and types as Symbol.
# For types, nameof returns the canonical TypeName symbol (parameters stripped,
# Base display aliases collapsed onto the shared TypeName), matching upstream
# Julia 1.12: `nameof(Vector{Int}) === :Array` because `Vector{T}` is
# `Array{T,1}` whose TypeName is `Array` (Issue #5106).

# Test 1: nameof for builtin functions
@assert nameof(sin) == :sin
@assert nameof(cos) == :cos
@assert nameof(abs) == :abs

# Test 2: user-defined function nameof is covered by
# nameof_user_function_5580.jl. This fixture focuses on TypeName /
# nameof(::Type) consistency for Issue #5106.

# Test 3: nameof for primitive types
@assert nameof(Int64) == :Int64
@assert nameof(Float64) == :Float64
@assert nameof(Bool) == :Bool
@assert nameof(String) == :String

# Test 4: nameof for non-parametric collection types
@assert nameof(Array) == :Array
@assert nameof(Tuple) == :Tuple
@assert nameof(Dict) == :Dict

# Test 5: the Array family shares the `Array` TypeName (Issue #5106).
# Vector/Matrix are display aliases of Array{T,1}/Array{T,2}.
@assert nameof(Vector) == :Array
@assert nameof(Vector{Int64}) == :Array
@assert nameof(Vector{Float64}) == :Array
@assert nameof(Matrix) == :Array
@assert nameof(Matrix{Int64}) == :Array
@assert nameof(Array{Int64,1}) == :Array
@assert nameof(Array{Int64,2}) == :Array

# Test 6: nameof with typeof (typeof([...]) is Vector{Int} === Array{Int,1})
arr = [1, 2, 3]
@assert nameof(typeof(arr)) == :Array

# Test 7: other parametric builtins strip parameters but keep their own name
@assert nameof(Dict{Int64,Int64}) == :Dict
@assert nameof(Set{Int64}) == :Set
@assert nameof(UnitRange) == :UnitRange
@assert nameof(UnitRange{Int64}) == :UnitRange

# Test 8: abstract types
@assert nameof(Number) == :Number
@assert nameof(Real) == :Real
@assert nameof(Integer) == :Integer

# Test 9: nameof for user-defined struct types (non-parametric)
struct NameOfTestPoint
    x::Float64
    y::Float64
end
@assert nameof(NameOfTestPoint) == :NameOfTestPoint

# Test 10: user-defined parametric struct shares its base TypeName across
# instantiations
struct NameOfBox{T} end
@assert nameof(NameOfBox) == :NameOfBox
@assert nameof(NameOfBox{Int64}) == :NameOfBox
@assert nameof(NameOfBox{String}) == :NameOfBox

# Test 11: user abstract type and a parametric subtype
abstract type NameOfAbs{T} end
struct NameOfImpl{T} <: NameOfAbs{T} end
@assert nameof(NameOfAbs) == :NameOfAbs
@assert nameof(NameOfAbs{Int64}) == :NameOfAbs
@assert nameof(NameOfImpl) == :NameOfImpl
@assert nameof(NameOfImpl{Int64}) == :NameOfImpl

# Test 12: Base.typename shares the same canonical TypeName across all
# instantiations and aliases of a type (Issue #5106). Not exported, matching
# upstream.
@assert Base.typename(NameOfBox{Int64}) === Base.typename(NameOfBox)
@assert Base.typename(Vector{Int64}) === Base.typename(Array)
@assert Base.typename(Matrix{Int64}) === Base.typename(Array)

true
