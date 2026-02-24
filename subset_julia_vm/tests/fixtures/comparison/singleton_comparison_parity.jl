# Singleton comparison parity tests (Issue #1754)
# Verifies that equality (==) and identity (===) operators produce
# the same results for singleton values (nothing, DataType, Symbol, Char).
# Prevention test to avoid regression of Issue #1747.

using Test

# --- Functions for type narrowing parity tests ---

# Type narrowing with !== nothing
function narrow_neq(x::Union{Int64, Nothing})
    if x !== nothing
        return x + 1
    end
    return 0
end

# Type narrowing with != nothing
function narrow_ne(x::Union{Int64, Nothing})
    if x != nothing
        return x + 1
    end
    return 0
end

# Type narrowing with === nothing (else branch)
function narrow_egal(x::Union{Int64, Nothing})
    if x === nothing
        return 0
    else
        return x + 1
    end
end

# Type narrowing with == nothing (else branch)
function narrow_eq(x::Union{Int64, Nothing})
    if x == nothing
        return 0
    else
        return x + 1
    end
end

@testset "Singleton comparison parity (Issue #1754)" begin
    @testset "nothing comparisons" begin
        # nothing == nothing vs nothing === nothing
        @test (nothing == nothing) === (nothing === nothing)
        @test (nothing != nothing) === (nothing !== nothing)

        # Int64 vs nothing
        @test (1 == nothing) === (1 === nothing)
        @test (1 != nothing) === (1 !== nothing)

        # nothing vs Int64
        @test (nothing == 1) === (nothing === 1)
        @test (nothing != 1) === (nothing !== 1)
    end

    @testset "Type narrowing parity" begin
        # !== vs != for nothing narrowing
        @test narrow_neq(5) == narrow_ne(5)
        @test narrow_neq(nothing) == narrow_ne(nothing)

        # === vs == for nothing narrowing (else branch)
        @test narrow_egal(5) == narrow_eq(5)
        @test narrow_egal(nothing) == narrow_eq(nothing)

        # All four should produce same results
        @test narrow_neq(10) == 11
        @test narrow_ne(10) == 11
        @test narrow_egal(10) == 11
        @test narrow_eq(10) == 11

        @test narrow_neq(nothing) == 0
        @test narrow_ne(nothing) == 0
        @test narrow_egal(nothing) == 0
        @test narrow_eq(nothing) == 0
    end

    @testset "DataType comparisons" begin
        @test (typeof(1) == Int64) === (typeof(1) === Int64)
        @test (typeof(1) != Float64) === (typeof(1) !== Float64)
        @test (typeof(1.0) == Float64) === (typeof(1.0) === Float64)
    end

    @testset "Symbol comparisons" begin
        @test (:foo == :foo) === (:foo === :foo)
        @test (:foo != :bar) === (:foo !== :bar)
        @test (:foo == :bar) === (:foo === :bar)
    end

    @testset "Char comparisons" begin
        @test ('a' == 'a') === ('a' === 'a')
        @test ('a' != 'b') === ('a' !== 'b')
        @test ('a' == 'b') === ('a' === 'b')
    end
end

true
