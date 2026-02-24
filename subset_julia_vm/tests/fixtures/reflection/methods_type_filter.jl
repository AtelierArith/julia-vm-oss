# Test methods(f, types) - type-filtered method lookup
# Issue #3257: methods(f, [Type1, Type2]) was not supported
using Test

function foo(x::Int64)
    x + 1
end

function foo(x::Float64)
    x * 2.0
end

function foo(x)
    x
end

function bar(x::Int64, y::Int64)
    x + y
end

function bar(x::Float64, y::Float64)
    x + y
end

@testset "methods(f, types) - type-filtered lookup" begin
    # methods(foo, [Int64]) should return exactly the Int64 method
    ms_int = methods(foo, [Int64])
    @test length(ms_int) == 1

    # methods(foo, [Float64]) should return exactly the Float64 method
    ms_float = methods(foo, [Float64])
    @test length(ms_float) == 1

    # methods(foo) without filter should return all 3 methods
    ms_all = methods(foo)
    @test length(ms_all) == 3

    # methods(bar, [Int64, Int64]) should return only the Int64+Int64 method
    ms_bar_int = methods(bar, [Int64, Int64])
    @test length(ms_bar_int) == 1

    # methods(bar, [Float64, Float64]) should return only Float64+Float64 method
    ms_bar_float = methods(bar, [Float64, Float64])
    @test length(ms_bar_float) == 1

    # methods(bar, [Int64, Float64]) should return nothing (no matching method)
    ms_bar_mixed = methods(bar, [Int64, Float64])
    @test length(ms_bar_mixed) == 0
end

true
