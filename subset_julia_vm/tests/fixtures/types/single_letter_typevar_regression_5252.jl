# Regression guard for Issue #5252: the fix that makes a *declared* single-letter
# type name a DataType must NOT break legitimate single-letter TYPE VARIABLES in
# `where` clauses and parametric type parameters.

using Test

struct Box{T}
    val::T
end

f(x::T) where T = T
h(x::A, y::B) where {A, B} = (A, B)
k(x::S) where S <: Real = S
gettype(::Type{T}) where T = T

@testset "single-letter where/parametric type variables still work (Issue #5252)" begin
    # where T binds the argument's type
    @assert f(3) === Int64
    @assert f("hi") === String
    @assert f(2.5) === Float64

    # Parametric struct with single-letter type parameter
    b = Box(42)
    @assert typeof(b) === Box{Int64}
    @assert b.val == 42
    bs = Box("hello")
    @assert typeof(bs) === Box{String}
    @assert bs.val == "hello"

    # Multi-parameter where {A, B}
    @assert h(1, "x") === (Int64, String)

    # where with an upper bound
    @assert k(3.0) === Float64
    @assert k(7) === Int64

    # A bare type variable referenced as a value inside a where body
    @assert gettype(Int) === Int64
    @assert gettype(Float64) === Float64

    # Vector{T}-style element typing is unaffected
    v = [1, 2, 3]
    @assert eltype(v) === Int64
    @assert typeof(v) === Vector{Int64}

    @test (true)
end

true
