# Issue #6278 (follow-up to #6162): value-position `&&` / `||` must preserve the
# value of the final operand, not coerce it to `Bool`.
#
# Julia semantics:
#   a && b  ==  a ? b : false   (returns b as-is when a is true)
#   a || b  ==  a ? true : b     (returns b as-is when a is false)
#
# Before the fix, sjulia compiled the right operand via
# `compile_expr_as(right, Bool)`, so `true && 1` returned `true` (and
# `true && "x"` raised a *compile* error "Cannot convert Str to Bool").

using Test

@testset "&& returns the right operand's value when left is true (Issue #6278)" begin
    @test (true && 1) === 1
    @test (true && 2.5) === 2.5
    @test (true && "x") == "x"
    @test (true && 'c') === 'c'
end

@testset "&& returns false when left is false (Issue #6278)" begin
    @test (false && 1) === false
    @test (false && "x") === false
end

@testset "|| returns the right operand's value when left is false (Issue #6278)" begin
    @test (false || 2) === 2
    @test (false || "y") == "y"
    @test (false || 3.5) === 3.5
end

@testset "|| returns true when left is true (Issue #6278)" begin
    @test (true || 1) === true
    @test (true || "z") === true
end

@testset "chained &&/|| preserve the final operand (Issue #6278)" begin
    @test (true && true && 5) === 5
    @test (false || false || 7) === 7
    @test (true && false && 9) === false
    @test (true && true && "end") == "end"
end

@testset "preserved value flows into arithmetic (Issue #6278)" begin
    @test (true && 1) + 10 == 11
    @test (false || 2) * 3 == 6
end

b = true
c = false
@testset "preserved value via variables (Issue #6278)" begin
    @test (b && 99) === 99
    @test (c || 100) === 100
end

f_and() = true && 42
f_or() = false || "fallback"

@testset "function body returns preserved operand (Issue #6278)" begin
    @test f_and() === 42
    @test f_or() == "fallback"
end

true
