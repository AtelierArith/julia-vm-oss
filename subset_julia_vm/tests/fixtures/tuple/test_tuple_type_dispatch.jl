# Test Tuple type pattern matching for dispatch (Issue #2524)

using Test

# Functions dispatching on Tuple type patterns
f(::Tuple{}) = "empty"
f(::Tuple{Any}) = "one"
f(::Tuple{Any, Any}) = "two"
f(t::Tuple) = "many"

@testset "Tuple type dispatch" begin
    # Test 1: Empty tuple
    @test f(()) == "empty"

    # Test 2: One-element tuple
    @test f((1,)) == "one"
    @test f(("hello",)) == "one"

    # Test 3: Two-element tuple
    @test f((1, 2)) == "two"
    @test f((1, "a")) == "two"

    # Test 4: Three or more elements → fallback
    @test f((1, 2, 3)) == "many"
    @test f((1, 2, 3, 4)) == "many"
end

true
