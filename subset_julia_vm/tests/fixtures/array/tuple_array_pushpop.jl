# Test push!/pop! on tuple arrays

using Test

@testset "Tuple array push! operation" begin
    arr = [(1, 2.0)]

    # Push a tuple
    push!(arr, (3, 4.0))

    # Verify length after push
    @test (length(arr)) == 2
end

true  # Test passed
