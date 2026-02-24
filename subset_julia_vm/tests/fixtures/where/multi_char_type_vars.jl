# Test multi-character type variable names in where clauses (Issue #2273)
# Verifies that T1, T2, S1 etc. are recognized as type variables, not struct names.

using Test

# Three type parameters with multi-char names
function triple_type(x::T1, y::T2, z::T3) where {T1, T2, T3}
    (x, y, z)
end

# Mixed single and multi-char type variable names
function mixed_vars(x::T, y::S1) where {T, S1}
    (x, y)
end

# Multi-char type vars with upper bounds
function bounded_multi(x::T1, y::T2) where {T1<:Number, T2<:Number}
    x + y
end

@testset "Multi-char type vars: T1, T2, T3" begin
    result = triple_type(1, 2.0, "hello")
    @test result[1] == 1
    @test result[2] == 2.0
    @test result[3] == "hello"
end

@testset "Mixed single and multi-char type vars" begin
    result = mixed_vars(42, 3.14)
    @test result[1] == 42
    @test result[2] == 3.14
end

@testset "Bounded multi-char type vars" begin
    @test bounded_multi(1, 2) == 3
    @test bounded_multi(1.0, 2.0) == 3.0
    @test bounded_multi(1, 2.0) == 3.0
end

true
