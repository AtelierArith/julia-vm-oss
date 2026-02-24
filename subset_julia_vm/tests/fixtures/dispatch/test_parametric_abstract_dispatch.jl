# Test parametric abstract type dispatch (Issue #2523)
# abstract type Container{T} end should preserve type params at runtime

using Test

# Parametric abstract type hierarchy
abstract type Container{T} end

# Non-parametric structs with parametric abstract parent
struct IntBox <: Container{Int64}
    value::Int64
end

struct FloatBox <: Container{Float64}
    value::Float64
end

# Dispatch on parametric abstract type
describe(::Container{Int64}) = "int container"
describe(::Container{Float64}) = "float container"

# Dispatch on base abstract type (no params)
is_container(::Container) = true

@testset "Parametric abstract type dispatch" begin
    b1 = IntBox(42)
    b2 = FloatBox(3.14)

    # Test 1: Struct construction works
    @test b1.value == 42
    @test b2.value == 3.14

    # Test 2: Dispatch on parametric abstract types
    @test describe(b1) == "int container"
    @test describe(b2) == "float container"

    # Test 3: Dispatch on base abstract type
    @test is_container(b1) == true
    @test is_container(b2) == true

    # Test 4: Subtype relationships
    @test IntBox <: Container{Int64}
    @test FloatBox <: Container{Float64}
    @test IntBox <: Container
    @test FloatBox <: Container
end

true
