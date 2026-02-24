# Test Type{T} - types as first-class values (Issue #993)
# Tests the Type hierarchy: DataType <: Type <: Any

using Test

# Function definitions must be outside @testset

# Function dispatch on Type{T}
function get_zero(::Type{Int64})
    0
end

function get_zero(::Type{Float64})
    0.0
end

@testset "Type{T} as first-class values" begin
    # Test 1: Type{T} dispatch with concrete types
    @test get_zero(Int64) == 0
    @test get_zero(Float64) == 0.0

    # Test 2: typeof on types returns DataType
    @test typeof(Int64) == DataType
    @test typeof(Float64) == DataType

    # Test 3: Type identity with ===
    @test Int64 === Int64
    @test Float64 === Float64

    # Test 4: DataType <: Type hierarchy
    # In Julia, all type objects are instances of Type
    @test DataType <: Type
    @test Type <: Any
end

true
