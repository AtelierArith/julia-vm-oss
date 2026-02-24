using Test

# Test kwargs varargs with no keyword arguments passed (Issue #2247).
# Previously, kwargs received Nothing instead of empty Pairs/NamedTuple.

function f(; kwargs...)
    length(kwargs)
end

g(; kwargs...) = length(kwargs)

@testset "kwargs varargs empty - full form" begin
    @test f() == 0
    @test f(a=1) == 1
    @test f(a=1, b=2) == 2
end

@testset "kwargs varargs empty - short form" begin
    @test g() == 0
    @test g(x=10) == 1
end

true
