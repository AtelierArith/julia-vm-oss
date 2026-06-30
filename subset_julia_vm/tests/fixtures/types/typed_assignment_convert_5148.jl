# Typed variable declarations `x::T = v` must `convert(T, v)` on assignment
# (Issue #5148). A bare `x::T` in expression position is a type assertion.

using Test

# `x::Float64 = 3` converts the Int literal 3 to the Float64 value 3.0.
function decl_int_to_float()
    x::Float64 = 3
    return (x, typeof(x))
end

# Converting a non-integral Float64 to Int must throw InexactError, exactly
# like `convert(Int, 3.7)`.
function decl_inexact()
    x::Int = 3.7
    return x
end

# `x::Float64 = x + y` converts the (Int) sum through Float64.
function decl_convert_expr()
    a = 2
    b = 3
    s::Float64 = a + b
    return (s, typeof(s))
end

# A bare `x::T` used as an expression is a type assertion: it returns the
# value when the runtime type matches.
function assertion_match()
    x = 5.0
    return x::Float64
end

# ... and throws a TypeError when the runtime type does not match.
function assertion_mismatch()
    x = 5
    return x::Float64
end

# `global g::T = v` enforces the declared type on the (correctly typed) value.
global g5148::Int = 5

@testset "typed assignment convert (Issue 5148)" begin
    @test decl_int_to_float() == (3.0, Float64)
    @test_throws InexactError decl_inexact()
    @test decl_convert_expr() == (5.0, Float64)
    @test assertion_match() == 5.0
    @test_throws TypeError assertion_mismatch()
    @test g5148 == 5
    @test typeof(g5148) == Int
end

true
