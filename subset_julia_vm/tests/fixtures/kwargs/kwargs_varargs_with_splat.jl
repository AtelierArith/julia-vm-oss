using Test

# Test kwargs varargs when function is called via positional splat (Issue #2269).
# CallWithSplat instruction must also bind kwargs varargs to empty Pairs.

function f(x, y; kwargs...)
    (x + y, length(kwargs))
end

g(x, y; kwargs...) = (x + y, length(kwargs))

@testset "kwargs varargs with positional splat - full form" begin
    args = (1, 2)
    result = f(args...)
    @test result[1] == 3
    @test result[2] == 0  # kwargs should be empty Pairs, not uninitialized
end

@testset "kwargs varargs with positional splat - short form" begin
    args = (10, 20)
    result = g(args...)
    @test result[1] == 30
    @test result[2] == 0  # kwargs should be empty Pairs, not uninitialized
end

@testset "mixed: splat positional + explicit kwargs" begin
    # This uses CallWithKwargsSplat, not CallWithSplat
    args = (1, 2)
    result = f(args...; a=1)
    @test result[1] == 3
    @test result[2] == 1
end

true
