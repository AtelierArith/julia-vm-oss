# Return type declaration `function f()::T` inserts `convert(T, ret)` on every
# return path and the implicit final-expression return (Issue #5147).
#
# Mirrors upstream julia-syntax.scm `method-lambda-expr` / `convert-for-type-decl`:
# the function body's return value(s) are wrapped so the result is `convert`-ed
# to the declared type T (and a non-convertible value throws like upstream).
#
# Verified against upstream Julia 1.12.6 (13 passed, 0 failed).

using Test

# Short form: f()::Int = 2.0  ->  2 :: Int64
f1()::Int64 = 2.0

# Short form with an argument: f(x)::Float64 = x  ->  Float64
f2(x)::Float64 = x

# Full form with an explicit `return`: 3 (Int) converted to Float64
function g1()::Float64
    return 3
end

# Full form, implicit final-expression return: 2.0 (Float64) converted to Int64
function g2()::Int64
    1.0 + 1.0
end

# Full form with multiple `return` sites: every path is convert-wrapped to Int64
function g3(x)::Int64
    if x > 0
        return x
    else
        return -x
    end
end

# Non-convertible value (String -> Int) must throw, matching upstream.
h()::Int64 = "x"

# Non-integral float (2.5 -> Int) must throw InexactError, matching upstream.
k()::Int64 = 2.5

@testset "return type convert insertion (Issue #5147)" begin
    @test f1() == 2
    @test typeof(f1()) === Int64
    @test f2(5) == 5.0
    @test typeof(f2(5)) === Float64
    @test g1() == 3.0
    @test typeof(g1()) === Float64
    @test g2() == 2
    @test typeof(g2()) === Int64
    @test g3(2.0) == 2
    @test typeof(g3(2.0)) === Int64
    @test g3(-3.0) == 3
    @test_throws Exception h()
    @test_throws Exception k()
end

true  # Test passed
