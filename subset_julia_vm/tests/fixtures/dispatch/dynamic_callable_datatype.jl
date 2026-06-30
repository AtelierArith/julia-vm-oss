using Test
import Base: Float64

struct DynamicCallableWrap
    x::Int64
end

struct DynamicCallablePair
    x::Int64
    y::Int64
end

struct DynamicCallableFloat64Input3910
    x::Int64
end

function call_one_arg_type(T, x)
    return T(x)
end

function call_two_arg_type(T, x, y)
    return T(x, y)
end

function call_one_arg_function(f, x)
    return f(x)
end

function call_two_arg_function(f, x, y)
    return f(x, y)
end

dynamic_inc(x::Int64) = x + 1
dynamic_add(x::Int64, y::Int64) = x + y
Float64(x::DynamicCallableFloat64Input3910) = 3910.0

@testset "Any-typed callable values use runtime callable dispatch" begin
    w = call_one_arg_type(DynamicCallableWrap, 41)
    @test w.x == 41

    p = call_two_arg_type(DynamicCallablePair, 20, 22)
    @test p.x == 20
    @test p.y == 22

    @test call_one_arg_type(Float64, 7) == 7.0
    @test call_one_arg_type(Float64, DynamicCallableFloat64Input3910(1)) == 3910.0
    @test call_one_arg_function(dynamic_inc, 41) == 42
    @test call_two_arg_function(dynamic_add, 20, 22) == 42
end

true
