# Test Type{T} pattern matching in dispatch (Issue #465)

using Test

# Function definitions must be outside @testset

# Test 1: Function dispatch on Type{T}
function create_default(::Type{Int64})
    0
end

function create_default(::Type{Float64})
    0.0
end

# Test 2: Type{T} as return value in subtype checks
function is_numeric_type(::Type{Int64})
    true
end

function is_numeric_type(::Type{Float64})
    true
end

function is_numeric_type(::Type{T}) where T
    false
end

@testset "Type{T} pattern matching" begin
    # Test 1: Basic Type{T} dispatch
    @test create_default(Int64) == 0
    @test create_default(Float64) == 0.0

    # Test 2: Type{T} dispatch with where clause fallback
    @test is_numeric_type(Int64) == true
    @test is_numeric_type(Float64) == true
    @test is_numeric_type(String) == false

    # Test 3: typeof returns Type
    @test typeof(Int64) <: Type
    @test typeof(Float64) <: Type
end

true
