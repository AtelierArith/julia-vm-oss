# Issue #4830 (follow-up to #4795): typeof('a':'e') returned
# `UnitRange{Char}` in sjulia but upstream Julia returns
# `StepRange{Char, Int64}`. The typeof discrepancy caused
# `show(io, ::StepRange-Char-range)` dispatch to route to the
# UnitRange show method, emitting `'a':'e'` instead of upstream's
# `'a':1:'e'`.
#
# Char ranges in upstream Julia are always `StepRange{Char, Int64}`,
# never `UnitRange{Char}`, because `:` over non-numeric types
# defaults to the explicit-step form and Char arithmetic uses Int
# steps (Char + Int = Char, Char - Char = Int).
#
# Fix: added Char-element guards in three places that reported the
# Range typeof / kind: `value_enum.rs::runtime_type`,
# `type_ops/introspection.rs::value_type` (used by `typeof`), and
# `exec/call_dynamic.rs::is_native_range_candidate_mismatch` (used
# by method dispatch). All three now classify Char ranges as
# `StepRange{Char, Int64}` regardless of step value, which makes
# `typeof`, `isa`, and `show` dispatch all agree.

using Test

@testset "typeof('a':'e') is StepRange{Char, Int64} (Issue #4830)" begin
    @test typeof('a':'e') === StepRange{Char, Int64}
end

@testset "typeof('e':-1:'a') is StepRange{Char, Int64} (Issue #4830)" begin
    @test typeof('e':-1:'a') === StepRange{Char, Int64}
end

@testset "isa Char range is StepRange not UnitRange (Issue #4830)" begin
    r = 'a':'e'
    @test isa(r, StepRange)
    @test !isa(r, UnitRange)
    @test isa(r, AbstractRange)
end

@testset "repr('a':'e') routes to StepRange show (Issue #4830)" begin
    @test repr('a':'e') == "'a':1:'e'"
end

@testset "show(io, 'a':'e') routes to StepRange show (Issue #4830)" begin
    buf = IOBuffer()
    show(buf, 'a':'e')
    @test String(take!(buf)) == "'a':1:'e'"
end

@testset "Numeric range typeof regression — UnitRange / StepRange (Issue #4830)" begin
    # Adding the Char guard must not regress numeric range typeof.
    @test typeof(1:5) === UnitRange{Int64}
    @test typeof(1:2:9) === StepRange{Int64, Int64}
end

@testset "Numeric range isa regression (Issue #4830)" begin
    @test isa(1:5, UnitRange)
    @test isa(1:2:9, StepRange)
end

true
