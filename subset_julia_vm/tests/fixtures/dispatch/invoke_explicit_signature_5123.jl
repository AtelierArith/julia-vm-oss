# Issue #5123: `invoke(f, Tuple{ArgTypes...}, args...)` calls the method matching
# the GIVEN signature type tuple, NOT necessarily the most specific applicable
# method (the upstream `Core.invoke` / `jl_f_invoke` behavior described in
# julia/doc/src/manual/methods.md "Invoking a method on a more general signature").
#
# `f(3)` picks the most specific `f(::Int)`, while `invoke(f, Tuple{Integer}, 3)`
# explicitly selects the more general `f(::Integer)` method. This fixture locks in
# parity with upstream Julia for the representative `invoke` surfaces.

using Test

# Single-argument specificity bypass.
f(::Integer) = :gen
f(::Int) = :spec

# Two-argument signature.
g(x::Number, y::Number) = :number
g(x::Int, y::Int) = :int

# Return value flows into arithmetic.
add(x::Number, y::Number) = x + y + 100
add(x::Int, y::Int) = x + y

# Vararg signature via Tuple{Vararg{T}}.
vf(xs::Int...) = sum(xs)

# Parametric method selected through invoke.
p(x::T) where {T<:Number} = (:param, T)
p(x::Int) = :spec_int

# Three-argument mixed signature.
m(a, b::Integer, c) = (:gen3, a, b, c)
m(a, b::Int, c) = (:spec3, a, b, c)

@testset "invoke selects the explicitly named signature (Issue #5123)" begin
    # Normal dispatch picks the most specific method ...
    @test f(3) == :spec
    # ... but invoke dispatches to the named (more general) signature.
    @test invoke(f, Tuple{Integer}, 3) == :gen

    @test g(1, 2) == :int
    @test invoke(g, Tuple{Number,Number}, 1, 2) == :number

    @test invoke(add, Tuple{Number,Number}, 2, 3) == 105

    @test invoke(vf, Tuple{Vararg{Int}}, 1, 2, 3) == 6

    @test p(5) == :spec_int
    @test invoke(p, Tuple{Number}, 5) == (:param, Int64)

    @test invoke(m, Tuple{Any,Integer,Any}, 1, 2, 3) == (:gen3, 1, 2, 3)
end

# `Core.invoke` / `Base.invoke` reach the same dispatch path.
@testset "Base.invoke / Core.invoke parity (Issue #5123)" begin
    @test Base.invoke(f, Tuple{Integer}, 7) == :gen
    @test Core.invoke(f, Tuple{Integer}, 9) == :gen
end

# A function alias and a Tuple-type alias both keep the explicit-signature path.
q(::Integer) = :qgen
q(::Int) = :qspec

@testset "invoke via function alias and signature alias (Issue #5123)" begin
    h = q
    @test invoke(h, Tuple{Integer}, 4) == :qgen

    sig = Tuple{Integer}
    @test invoke(q, sig, 4) == :qgen
end

# Keyword arguments are bound after the explicit-signature method is selected.
kw(x::Number; scale = 1) = x * scale + 1000
kw(x::Int; scale = 1) = x * scale

@testset "invoke preserves keyword arguments (Issue #5123)" begin
    @test kw(3; scale = 2) == 6
    @test invoke(kw, Tuple{Number}, 3; scale = 2) == 1006
end

true
