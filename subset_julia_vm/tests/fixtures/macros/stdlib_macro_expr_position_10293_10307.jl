# stdlib `Test` macros (@test/@testset/@test_throws/@test_broken) used in
# EXPRESSION/VALUE position (assignment RHS, binary-operator operand,
# function-call argument) previously failed to lower with a misleading
# "@<name> macro requires `using Test`" error, even with `using Test` present
# (Issue #10293, #10307). The expression-position macro dispatcher never
# looked up the stdlib-macro registry at all -- only the statement-position
# dispatcher did. This fixture exercises each covered position/macro; every
# `@testset` here is intentionally top-level (not nested in another
# `@testset`) so its "N passed, M failed" summary is directly comparable to
# upstream without depending on nested-testset count aggregation (a separate,
# unrelated gap tracked by Issue #10338).
#
# Issue #10496 completes the value contract: the macro calls below return the
# recorded Test.Result/TestSet-shaped values instead of `nothing`.
using Test

identity_fn(x) = x

# Assignment RHS, space-call spelling: `r = @test true` (Issue #10307 MWE).
@testset "assignment rhs space-call" begin
    r = @test true
    @test r isa Test.Pass
end

# Assignment RHS, adjacent-paren spelling: `r = @test(...)` (Issue #10293 MWE).
@testset "assignment rhs paren-call" begin
    r = @test(2 + 2 == 4)
    @test typeof(r) == Test.Pass
end

# Binary-operator operand: `@test(x) isa T` (Issue #10293's concrete example
# of the adjacent-paren spelling as an `isa` operand).
@testset "isa operand paren-call" begin
    is_int = @test(1 == 1) isa Int
    @test !is_int
end

# Function-call argument position.
@testset "function argument position" begin
    v = identity_fn(@test true)
    @test v isa Test.Pass
end

# @test_throws in expression position (assignment RHS).
@testset "test_throws expression position" begin
    r = @test_throws ErrorException error("boom")
    @test r isa Test.Pass
end

# @test_broken in expression position (assignment RHS).
@testset "test_broken expression position" begin
    r = @test_broken 1 == 2
    @test r isa Test.Broken
end

# @testset itself in expression position (assignment RHS). Deliberately
# top-level (not nested in an outer @testset) -- see note above.
y = @testset "testset expression position" begin
    @test true
end
@test y isa Test.DefaultTestSet

true
