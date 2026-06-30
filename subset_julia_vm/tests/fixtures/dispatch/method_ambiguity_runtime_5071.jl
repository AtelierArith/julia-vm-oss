# Issue #5071: an ambiguous method call must raise a *catchable* runtime
# MethodError (matching upstream Julia), not abort compilation.
#
# Upstream Julia raises `MethodError: f(::Int64, ::Int64) is ambiguous`
# at runtime, which is catchable via try/catch and `@test_throws MethodError`.
# Previously sjulia raised a hard `CompileError::Dispatch(AmbiguousMethod{..})`
# that exited the process (exit code 1) and was NOT catchable.

using Test

f(x::Int, y::Number) = "Int,Number"
f(x::Number, y::Int) = "Number,Int"

@testset "ambiguous dispatch throws catchable MethodError" begin
    # The ambiguous call must throw a catchable MethodError at runtime.
    @test_throws MethodError f(1, 2)

    # And it must be catchable via try/catch (process does NOT abort).
    caught = false
    try
        f(1, 2)
    catch e
        caught = true
    end
    @test caught
end

# Adding a most-specific resolver method makes the call unambiguous again;
# this already worked and must keep working.
g(x::Int, y::Number) = "Int,Number"
g(x::Number, y::Int) = "Number,Int"
g(x::Int, y::Int) = "Int,Int"

@testset "resolver method disambiguates" begin
    @test g(1, 2) == "Int,Int"
    # Non-ambiguous calls still pick the unique best method.
    @test g(1, 2.0) == "Int,Number"
    @test g(1.0, 2) == "Number,Int"
end

true
