# Issue #6536 / #6502 slice 2: the runtime dynamic dispatch channel
# (CallDynamicBinaryBoth / CallDynamicOrBuiltin for Any-typed operands) must
# enforce where-clause bounds on parametric struct params and cross-slot
# typevar binding consistency, matching upstream Julia.
#
# Operator methods use the function form (the assignment form drops the bound
# in lowering — Issue #6537).

using Test

import Base: *
import Base: ==
import Base: abs

abstract type Animal6536 end
struct Dog6536 <: Animal6536 end

struct Wrap6536{T}
    x::T
end

function *(a::Wrap6536{T}, b::Wrap6536{T}) where {T<:Real}
    return "wrap-real"
end
*(a::Wrap6536{T}, b::Wrap6536{S}) where {T,S} = "wrap-generic"

@testset "where bound enforced through Any-typed binary dispatch" begin
    ws = Any[Wrap6536(1), Wrap6536(2)]
    @test (ws[1] * ws[2]) == "wrap-real"
    wf = Any[Wrap6536("a"), Wrap6536("b")]
    @test (wf[1] * wf[2]) == "wrap-generic"
end

# NOTE: dispatch results below are bound to variables before the `@test`,
# because expressions inlined into `@test` are evaluated through the
# callable-value / test-macro channel, which does not yet enforce where
# bounds (pre-existing, independent of the runtime dispatch channel under
# test here).

struct Box6536{T}
    v::T
end

function ==(a::Box6536{T}, b::Box6536{T}) where {T<:Animal6536}
    return "box-animal"
end
==(a::Box6536, b::Box6536) = "box-any"

@testset "user-abstract bound resolves through hierarchy and stays specific" begin
    ba = Any[Box6536(Dog6536()), Box6536(Dog6536())]
    ra = ba[1] == ba[2]
    @test ra == "box-animal"
    bb = Any[Box6536(1), Box6536(2)]
    rb = bb[1] == bb[2]
    @test rb == "box-any"
end

struct Holder6536{T}
    v::T
end

function *(a::Holder6536{T}, b::Holder6536{T}) where {T}
    return "holder-diag"
end
*(a::Holder6536, b::Holder6536) = "holder-pair"

@testset "cross-slot typevar binding consistency" begin
    p = Any[Holder6536(1), Holder6536(2)]
    @test (p[1] * p[2]) == "holder-diag"
    q = Any[Holder6536(1), Holder6536("x")]
    @test (q[1] * q[2]) == "holder-pair"
end

function abs(h::Holder6536{T}) where {T<:Real}
    return "holder-real"
end
abs(h::Holder6536) = "holder-any"

@testset "unary builtin-fallback channel enforces bounds" begin
    hs = Any[Holder6536(3), Holder6536("s")]
    h1 = abs(hs[1])
    @test h1 == "holder-real"
    h2 = abs(hs[2])
    @test h2 == "holder-any"
    # builtin fallback stays intact
    a2 = abs(-2)
    @test a2 == 2
end

true
