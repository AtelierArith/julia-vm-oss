# Test const type aliases in dispatch (Issue #2527)
# const Alias = ExistingType{Param} should work in type annotations and dispatch

using Test

# Parametric struct definition
struct Wrapper{T}
    value::T
end

# Type aliases for parametric types
const IntWrapper = Wrapper{Int64}
const FloatWrapper = Wrapper{Float64}

# Functions using type aliases for dispatch
f(::IntWrapper) = "int wrapper"
f(::FloatWrapper) = "float wrapper"

# Type alias as value (for comparisons)
g(x) = typeof(x) == IntWrapper

@testset "Const type alias dispatch" begin
    # Test 1: Type alias equality
    @test IntWrapper === Wrapper{Int64}
    @test FloatWrapper === Wrapper{Float64}

    # Test 2: Dispatch using type aliases
    @test f(Wrapper{Int64}(42)) == "int wrapper"
    @test f(Wrapper{Float64}(3.14)) == "float wrapper"

    # Test 3: Type alias in typeof comparison
    @test g(Wrapper{Int64}(10)) == true
    @test g(Wrapper{Float64}(1.0)) == false
end

true
