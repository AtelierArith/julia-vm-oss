# Tests for bounded where-clause type variable binding from tuples (Issue #2316)
# Tests type variable bounds (where T<:Real) and mixed concrete/TypeVar patterns

using Test

# Bounded type variable: T<:Real constraint
function bounded_sum(t::Tuple{T, T}) where {T<:Real}
    return t[1] + t[2]
end

# Mixed concrete and TypeVar: first element is concrete Int64
function mixed_first_int(t::Tuple{Int64, T}) where T
    return t[2]
end

# Mixed concrete and TypeVar: second element is concrete String
function mixed_second_string(t::Tuple{T, String}) where T
    return t[1]
end

# Nested bounded type: Tuple{T, Tuple{T, T}} where T<:Number
function nested_sum(t::Tuple{T, Tuple{T, T}}) where {T<:Number}
    return t[1] + t[2][1] + t[2][2]
end

@testset "Bounded where-clause tuple binding (Issue #2316)" begin
    @testset "T<:Real bound" begin
        # T binds to Int64 (Int64 <: Real)
        @test bounded_sum((1, 2)) == 3
        @test bounded_sum((10, 20)) == 30

        # T binds to Float64 (Float64 <: Real)
        @test bounded_sum((1.5, 2.5)) == 4.0
        @test bounded_sum((0.1, 0.2)) ≈ 0.3 atol=1e-10
    end

    @testset "Mixed concrete and TypeVar - Int64 first" begin
        # T binds to String
        @test mixed_first_int((1, "hello")) == "hello"

        # T binds to Float64
        @test mixed_first_int((42, 3.14)) == 3.14

        # T binds to Int64
        @test mixed_first_int((1, 2)) == 2
    end

    @testset "Mixed concrete and TypeVar - String second" begin
        # T binds to Int64
        @test mixed_second_string((42, "world")) == 42

        # T binds to Float64
        @test mixed_second_string((3.14, "pi")) == 3.14

        # T binds to Bool
        @test mixed_second_string((true, "flag")) == true
    end

    @testset "Nested bounded tuple" begin
        # T binds to Int64 (Int64 <: Number)
        @test nested_sum((1, (2, 3))) == 6
        @test nested_sum((10, (20, 30))) == 60

        # T binds to Float64 (Float64 <: Number)
        @test nested_sum((1.0, (2.0, 3.0))) == 6.0
    end
end

# Test method selection with competing bounded methods
function process_bounded(t::Tuple{T, T}) where {T<:Integer}
    return "integer"
end

function process_bounded(t::Tuple{T, T}) where {T<:AbstractFloat}
    return "float"
end

@testset "Bounded where-clause method selection" begin
    # Should select Integer version for Int64 pair
    @test process_bounded((1, 2)) == "integer"

    # Should select AbstractFloat version for Float64 pair
    @test process_bounded((1.0, 2.0)) == "float"
end

true
