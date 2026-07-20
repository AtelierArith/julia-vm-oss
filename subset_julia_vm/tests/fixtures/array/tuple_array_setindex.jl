# Test setindex! on tuple arrays

using Test

@testset "Tuple array setindex! operation" begin
    arr = [(1, 2.0), (3, 4.0)]

    # Modify element
    arr[1] = (10, 20.0)

    # Access and verify the change through the second element
    t = arr[1]
    @test (t[2]) == 20.0
end

true  # Test passed
