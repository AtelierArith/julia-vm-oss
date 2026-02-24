# Test ismutable function

using Test

@testset "ismutable - check if value is mutable" begin

    # Mutable values
    arr = [1, 2, 3]
    @assert ismutable(arr)

    # Primitives are immutable
    @assert !ismutable(42)
    @assert !ismutable(3.14)

    @test (true)
end

true  # Test passed
