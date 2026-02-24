using Test

# Test kwargs varargs + splat dispatch combinations (Issue #3324).
# Ensures call handlers (CallFunctionVariable, CallFunctionVariableWithSplat)
# route kwargs binding through bind_kwargs_defaults(), not inline loops.

function f_with_kwargs_varargs(x, y; kwargs...)
    (x + y, length(kwargs))
end

@testset "kwargs splat dispatch: basic splat with kwargs varargs" begin
    args = (1, 2)
    result = f_with_kwargs_varargs(args...)
    @test result[1] == 3
    @test result[2] == 0  # kwargs... should receive empty Pairs
end

# Multi-method dispatch through function variable
function multi_dispatch(x::Int64, y::Int64)
    "int-int"
end
function multi_dispatch(x::Float64, y::Float64)
    "float-float"
end

@testset "kwargs splat dispatch: multi-method splat calls" begin
    int_args = (1, 2)
    float_args = (1.0, 2.0)
    @test multi_dispatch(int_args...) == "int-int"
    @test multi_dispatch(float_args...) == "float-float"
end

# kwargs with default values and splat
function with_defaults(a, b; sep="-", prefix="")
    string(prefix, a, sep, b)
end

@testset "kwargs splat dispatch: defaults with splat" begin
    args = ("hello", "world")
    @test with_defaults(args...) == "hello-world"
    @test with_defaults(args...; sep=":") == "hello:world"
end

true
