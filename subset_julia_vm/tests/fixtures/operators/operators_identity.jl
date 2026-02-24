# Test identity operators === and !==

using Test

@testset "identity operators === and !==" begin

    # Identical values
    @assert 1 === 1
    @assert 3.14 === 3.14
    @assert "hello" === "hello"

    # Non-identical values (different types)
    @assert !(1 === 1.0)
    @assert !(1 === true)

    # Non-identity operator (!==)
    @assert 1 !== 1.0
    @assert 1 !== 2

    # Same value, same type
    @assert !(1 !== 1)
    @assert !("test" !== "test")

    @test (true)
end

true  # Test passed
