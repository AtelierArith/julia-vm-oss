# Issue #6782: defining a user `promote_rule` method must NOT break `promote_type`
# dispatch for unrelated (numeric) pairs.
#
# Root cause was a blanket "crosses base/user boundary" fence in the runtime
# metadata dispatcher (`Vm::find_best_method_index_from_candidates`): once a user
# adds any `promote_rule` method, the candidate set for `promote_rule` mixes
# Base-origin and user-origin methods, and the fence disabled the metadata scorer
# entirely. The metadata scorer is the only channel that resolves `where`-bounded
# `Type{T}` parametric methods (e.g. `promote_rule(::Type{Bool}, ::Type{T}) where
# {T<:Number}` and `promote_rule(::Type{Complex{T}}, ::Type{S})`), so those base
# methods fell through to the generic `Union{}` rule and `promote_type` widened to
# the `typejoin` (Integer / Number). Concrete-typed base methods (e.g.
# `promote_rule(::Type{Int16}, ::Type{Int8})`) still resolved via the typed-core
# string resolver, which is why only the parametric pairs broke.
#
# The fence is redundant: the function body already applies the per-candidate
# native-array wrapper boundary exclusion (Issue #6202) and the Issue #5926 origin
# dominance fence inside the dominance pre-check, so the metadata scorer is safe
# for mixed sets. Values verified against upstream julia.

using Test

import Base: promote_rule

struct Meter6782 end
struct Foot6782 end

promote_rule(::Type{Meter6782}, ::Type{Foot6782}) = Meter6782

@testset "user promote_rule extension still works (Issue #6782)" begin
    @test promote_type(Meter6782, Foot6782) === Meter6782
    @test promote_type(Foot6782, Meter6782) === Meter6782
end

@testset "user promote_rule does not corrupt numeric promote_type (Issue #6782)" begin
    # parametric `where`-bounded base methods (the ones the fence broke)
    @test promote_type(Bool, Int64) === Int64
    @test promote_type(Int64, Bool) === Int64
    @test promote_type(Bool, Float64) === Float64
    @test promote_type(Complex{Float64}, Int64) === Complex{Float64}
    @test promote_type(Int64, Complex{Float64}) === Complex{Float64}
    # concrete base methods (resolved by the string resolver; should stay correct)
    @test promote_type(Int8, Int16) === Int16
    @test promote_type(Int16, Int8) === Int16
    @test promote_type(Float32, Float64) === Float64
    @test promote_type(Int64, Float64) === Float64
    # rational (parametric `where`)
    @test promote_type(Rational{Int64}, Int64) === Rational{Int64}
end

# Final expression doubles as the harness regression check (the fixture runner
# compares this value to `expected = true`; sjulia's `@testset` does not throw on
# failure, so the conjunction below is what actually fails when the bug is present).
promote_type(Meter6782, Foot6782) === Meter6782 &&
    promote_type(Foot6782, Meter6782) === Meter6782 &&
    promote_type(Bool, Int64) === Int64 &&
    promote_type(Int64, Bool) === Int64 &&
    promote_type(Bool, Float64) === Float64 &&
    promote_type(Complex{Float64}, Int64) === Complex{Float64} &&
    promote_type(Int64, Complex{Float64}) === Complex{Float64} &&
    promote_type(Int8, Int16) === Int16 &&
    promote_type(Int16, Int8) === Int16 &&
    promote_type(Float32, Float64) === Float64 &&
    promote_type(Int64, Float64) === Float64 &&
    promote_type(Rational{Int64}, Int64) === Rational{Int64}
