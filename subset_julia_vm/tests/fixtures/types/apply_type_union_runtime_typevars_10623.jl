# Core.apply_type(Union, members...) builds a Union from its member type-args,
# accepting fresh runtime `TypeVar`s as members and deduplicating them by
# object IDENTITY (upstream `jl_type_union` / `jl_egal`): two DISTINCT
# `TypeVar(:F)` build `Union{F, F}` (not `F`), order-insensitively.
# Issue #10623.

using Test

@testset "Core.apply_type(Union, runtime TypeVars) (Issue #10623)" begin
    f1 = TypeVar(:F)
    f2 = TypeVar(:F)

    # Two DISTINCT same-named TypeVars stay as two members, order-insensitively.
    u1 = Core.apply_type(Union, f1, f2)
    u2 = Core.apply_type(Union, f2, f1)
    @test string(u1) == "Union{F, F}"
    @test string(u2) == "Union{F, F}"
    @test u1 == u2

    # A runtime variable holding `Union` is the same applicable family.
    U = Union
    @test string(Core.apply_type(U, f1, f2)) == "Union{F, F}"
    @test Core.apply_type(U, f1, f2) == u1

    # Zero members is the bottom type; a single member collapses to it.
    @test Core.apply_type(Union) === Union{}

    # A free TypeVar member mixes with concrete members without collapsing.
    @test string(Core.apply_type(Union, f1, Int)) == "Union{Int64, F}"
    @test Core.apply_type(Union, f1, Int) == Core.apply_type(Union, Int, f1)

    # `Any` absorbs a free TypeVar (its upper bound is `Any`).
    @test Core.apply_type(Union, Any, f1) === Any

    # Concrete members keep the ordinary Union canonicalization (flatten / dedup
    # / subtype-absorb / sort), unchanged and order-insensitive.
    @test Core.apply_type(Union, Int, Float64) == Union{Int, Float64}
    @test Core.apply_type(Union, Float64, Int) == Union{Int, Float64}
    @test Core.apply_type(Union, Int, Int) === Int
    @test Core.apply_type(Union, Int, Integer) === Integer
end

true
