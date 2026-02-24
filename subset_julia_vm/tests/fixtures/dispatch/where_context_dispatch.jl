# Test type inference for function dispatch inside where T context
# Issue #2556: TypeVar upper bounds should be used for compile-time dispatch

using Test

# Integer constraint - div should use integer division
function safe_div(x::T, y::T) where {T<:Integer}
    return div(x, y)
end

# Real constraint - arithmetic should work
function safe_add(x::T, y::T) where {T<:Real}
    return x + y
end

# Unconstrained TypeVar - runtime dispatch
function identity_op(x::T) where T
    return x
end

@testset "Where context dispatch (Issue #2556)" begin
    @testset "Integer-bounded div dispatch" begin
        @test safe_div(10, 3) == 3
        @test safe_div(7, 2) == 3
        @test typeof(safe_div(10, 3)) == Int64
    end

    @testset "Real-bounded addition" begin
        @test safe_add(1.5, 2.5) == 4.0
        @test safe_add(1, 2) == 3
    end

    @testset "Unconstrained TypeVar" begin
        @test identity_op(42) == 42
        @test identity_op("hello") == "hello"
    end
end

true
