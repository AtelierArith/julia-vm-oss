# Issue #6162: `sjulia` accepted non-Bool operands in `&&` / `||` boolean
# context by coercing `Int64` to `Bool`, while upstream Julia raises
# `TypeError: non-boolean (Int64) used in boolean context`.
#
# Branch (condition) position — `if`/`while`/ternary and `&&`/`||` used as a
# condition — was already strict (PR #6165). This fixture covers the remaining
# *value* position (`x = a && b`, `println(a || b)`, a function whose body is a
# bare `&&`/`||`), which routed through `compile_and_expr`/`compile_or_expr` and
# coerced the left operand via `I64ToBool`.
#
# Out of scope (separate follow-up): value preservation of the final operand,
# e.g. `true && 1` should return `1`, not `true`.

using Test

@testset "value-position && rejects non-Bool left operand (Issue #6162)" begin
    @test_throws TypeError (1 && true)
    @test_throws TypeError (0 && true)
    @test_throws TypeError (2 && false)
end

@testset "value-position || rejects non-Bool left operand (Issue #6162)" begin
    @test_throws TypeError (1 || false)
    @test_throws TypeError (0 || true)
end

@testset "non-Bool left operand via variable still rejected (Issue #6162)" begin
    x = 1
    @test_throws TypeError (x && true)
    @test_throws TypeError (x || false)
end

f_and() = (1 && true)
f_or() = (5 || false)

@testset "function body value-position &&/|| rejects non-Bool (Issue #6162)" begin
    @test_throws TypeError f_and()
    @test_throws TypeError f_or()
end

@testset "valid Bool operands in value position still work (Issue #6162)" begin
    @test (true && false) === false
    @test (true && true) === true
    @test (false && true) === false
    @test (false || true) === true
    @test (true || false) === true
    @test (false || false) === false
end

g(x) = x > 0 && x < 10
h(x) = x < 0 || x > 100

@testset "comparison operands keep working in value position (Issue #6162)" begin
    @test g(5) === true
    @test g(-1) === false
    @test g(50) === false
    @test h(-3) === true
    @test h(200) === true
    @test h(42) === false
end

@testset "chained && / || with Bool operands (Issue #6162)" begin
    @test (true && true && true) === true
    @test (true && false && true) === false
    @test (false || false || true) === true
end

true
