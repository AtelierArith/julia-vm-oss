# Tests for VectorOf specificity-based dispatch (Issue #2352)
# Vector{Int64} should be preferred over Vector{Any}

using Test

# Overlapping methods: specific vs general vector
function process_vec(v::Vector{Any})
    return "any"
end

function process_vec(v::Vector{Int64})
    return "int64"
end

function process_vec(v::Vector{Float64})
    return "float64"
end

@testset "VectorOf specificity dispatch (Issue #2352)" begin
    # Specific method should be selected over general
    @test process_vec([1, 2, 3]) == "int64"
    @test process_vec([1.0, 2.0, 3.0]) == "float64"

    # String array should match Any (no specific Vector{String} method)
    @test process_vec(["a", "b", "c"]) == "any"
end

# Test with abstract type hierarchy
function process_num(v::Vector{Any})
    return "any"
end

function process_num(v::Vector{Number})
    return "number"
end

function process_num(v::Vector{Real})
    return "real"
end

function process_num(v::Vector{Int64})
    return "int64"
end

@testset "VectorOf abstract type specificity" begin
    # Most specific concrete type should be selected
    @test process_num([1, 2, 3]) == "int64"
end

# Test that Vector dispatch doesn't interfere with non-Vector
function mixed_dispatch(x::Any)
    return "any_value"
end

function mixed_dispatch(v::Vector{Int64})
    return "vector_int64"
end

@testset "Vector vs non-Vector dispatch" begin
    # Vector should match Vector{Int64}
    @test mixed_dispatch([1, 2, 3]) == "vector_int64"

    # Non-vector should match Any
    @test mixed_dispatch(42) == "any_value"
    @test mixed_dispatch("hello") == "any_value"
    @test mixed_dispatch((1, 2)) == "any_value"
end

true
