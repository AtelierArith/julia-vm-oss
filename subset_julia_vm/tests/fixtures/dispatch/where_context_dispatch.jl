# Test type inference for function dispatch inside where T context
# Issue #2556: TypeVar upper bounds should be used for compile-time dispatch

using Test

# Integer constraint - div should use integer division
function safe_div(x::T, y::T) where {T<:Integer}
    return div(x, y)
end

function safe_rem(x::T, y::T) where {T<:Integer}
    return rem(x, y)
end

function safe_mod(x::T, y::T) where {T<:Integer}
    return mod(x, y)
end

function safe_gcd(x::T, y::T) where {T<:Integer}
    return gcd(x, y)
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
        # Issue #5398: a `where {T<:Integer}` method calling `div` must
        # runtime-dispatch to the concrete integer method rather than
        # statically binding the generic `floor(x / y)` fallback (Float64).
        # Concrete integer types must be preserved end-to-end.
        @test safe_div(Int32(10), Int32(3)) == 3
        @test typeof(safe_div(Int32(10), Int32(3))) == Int32
        @test safe_div(big(10), big(3)) == 3
        @test typeof(safe_div(big(10), big(3))) == BigInt
    end

    @testset "Integer-bounded builtin dispatch matrix" begin
        @test safe_rem(Int32(10), Int32(3)) == Int32(1)
        @test typeof(safe_rem(Int32(10), Int32(3))) == Int32
        @test safe_rem(big(10), big(3)) == 1
        @test typeof(safe_rem(big(10), big(3))) == BigInt

        @test safe_mod(Int32(10), Int32(3)) == Int32(1)
        @test typeof(safe_mod(Int32(10), Int32(3))) == Int32
        @test safe_mod(big(10), big(3)) == 1
        @test typeof(safe_mod(big(10), big(3))) == BigInt

        @test safe_gcd(Int32(10), Int32(3)) == Int32(1)
        @test typeof(safe_gcd(Int32(10), Int32(3))) == Int32
        @test safe_gcd(big(10), big(3)) == 1
        @test typeof(safe_gcd(big(10), big(3))) == BigInt
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
