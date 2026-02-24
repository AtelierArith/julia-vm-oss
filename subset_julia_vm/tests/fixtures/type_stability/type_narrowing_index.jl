# Type narrowing tests for index access in conditional branches
# Issue #1740: Verify type inference correctly narrows index access types
#
# This tests the feature implemented in Issue #1641:
# - Index access type narrowing with isa()
# - Index access type narrowing with nothing checks

using Test

# Helper: tuple index with isa() narrowing
function process_tuple_isa(t::Tuple{Union{Int64, String}, Int64})
    if isa(t[1], Int64)
        # In this branch, t[1] should be narrowed to Int64
        return t[1] * 2
    else
        # In this branch, t[1] is String
        return 0
    end
end

# Helper: tuple index with nothing check
function process_tuple_nothing(t::Tuple{Union{Int64, Nothing}, String})
    if t[1] !== nothing
        # In this branch, t[1] should be narrowed to Int64
        return t[1] + 100
    end
    return -1
end

# Helper: array-like index access (using tuple for simplicity)
function process_pair_isa(pair::Tuple{Any, Any})
    if isa(pair[1], Float64)
        return pair[1] + 0.5
    end
    return 0.0
end

# Helper: second element narrowing
function process_second_element(t::Tuple{Int64, Union{String, Nothing}})
    if t[2] !== nothing
        # t[2] should be narrowed to String
        return length(t[2])
    end
    return 0
end

@testset "Index Access Type Narrowing" begin
    @testset "tuple isa narrowing" begin
        t1 = (42, 10)
        t2 = ("hello", 10)

        @test process_tuple_isa(t1) == 84
        @test process_tuple_isa(t2) == 0
    end

    @testset "tuple !== nothing narrowing" begin
        t1 = (50, "test")
        t2 = (nothing, "test")

        @test process_tuple_nothing(t1) == 150
        @test process_tuple_nothing(t2) == -1
    end

    @testset "any tuple isa narrowing" begin
        p1 = (3.14, "pi")
        p2 = ("not a float", 123)

        @test process_pair_isa(p1) == 3.64
        @test process_pair_isa(p2) == 0.0
    end

    @testset "second element narrowing" begin
        t1 = (1, "hello")
        t2 = (2, nothing)

        @test process_second_element(t1) == 5
        @test process_second_element(t2) == 0
    end
end

true
