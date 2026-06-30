# Issue #6539: where-clause bounds must be enforced on every evaluation
# channel that an expression inlined into `@test` (or a callable value bound
# to a variable) can take, matching upstream Julia:
#
# 1. The callable-value channel (`f = abs; f(x)` via
#    resolve_callable_value_candidates) ignored `where` bounds, so
#    `abs(h::Holder{T}) where {T<:Real}` matched `Holder{String}`.
# 2. `abs(hs[2]) == "..."` inlined into `@test` (or any expression position)
#    constant-folded to `false` at compile time: return-type inference
#    assumed `abs(::Any)::Float64`, and the String-vs-non-String equality
#    shortcut folded the comparison.
# 3. `(a == b) == "..."` with a user `==` returning a non-Bool mis-folded the
#    same way (equality result inference assumed Bool unconditionally).
#
# Verified against julia 1.12 (all tests pass upstream).

using Test

import Base: abs
import Base: ==

struct Holder6539{T}
    v::T
end

function abs(h::Holder6539{T}) where {T<:Real}
    return "holder-real"
end
abs(h::Holder6539) = "holder-any"

hs6539 = Any[Holder6539(3), Holder6539("s")]

@testset "inline @test call enforces where bound (Issue #6539)" begin
    # Variable-bound control (CallDynamic channel, fixed by #6536/#6543).
    r = abs(hs6539[2])
    @test r == "holder-any"
    # Inline form: previously constant-folded to `false` at compile time.
    @test abs(hs6539[2]) == "holder-any"
    @test abs(hs6539[1]) == "holder-real"
    # Inline comparison outside @test as well.
    @test (abs(hs6539[2]) == "holder-any") == true
end

@testset "callable-value channel enforces where bound (Issue #6539)" begin
    f = abs
    # `f(...)` routes through resolve_callable_value_candidates: the bounded
    # holder-real method must be rejected for Holder6539{String}.
    @test f(hs6539[2]) == "holder-any"
    @test f(hs6539[1]) == "holder-real"
end

struct Box6539{T}
    v::T
end

==(a::Box6539, b::Box6539) = "box-any"

bb6539 = Any[Box6539(1), Box6539(2)]

@testset "nested comparison with user non-Bool == (Issue #6539)" begin
    # Variable-bound control.
    r = bb6539[1] == bb6539[2]
    @test r == "box-any"
    # Inline nested form: previously constant-folded to `false` because the
    # inner `==` was unconditionally inferred Bool.
    @test (bb6539[1] == bb6539[2]) == "box-any"
end

true
