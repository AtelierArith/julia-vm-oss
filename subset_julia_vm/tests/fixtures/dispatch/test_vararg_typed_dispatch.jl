# Test Vararg{T} and Vararg{T,N} dispatch (Issue #2525)
using Test

# Basic Vararg{T} — equivalent to args::T...
function sum_ints(args::Vararg{Int64})
    s = 0
    for x in args
        s = s + x
    end
    s
end

# Vararg{T,N} — fixed count varargs
function pair(a::Vararg{Int64, 2})
    a[1] + a[2]
end

# Dispatch between specific-count and any-count varargs
function vfunc(x::Vararg{Int64, 1})
    "one"
end

function vfunc(x::Int64, y::Int64)
    "two"
end

@testset "Vararg{T} and Vararg{T,N} dispatch" begin
    # Vararg{Int64} collects any number of Int64 args
    @test sum_ints(1, 2, 3) == 6
    @test sum_ints(10, 20) == 30
    @test sum_ints(5) == 5

    # Vararg{Int64, 2} requires exactly 2 Int64 args
    @test pair(3, 4) == 7
    @test pair(10, 20) == 30

    # vfunc(x::Vararg{Int64, 1}) matches 1 arg, vfunc(x, y) matches 2 args
    @test vfunc(42) == "one"
    @test vfunc(1, 2) == "two"
end

true
