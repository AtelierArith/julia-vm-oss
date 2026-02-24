# Do block with reduce - two parameters (acc, val)

using Test

@testset "Do block with reduce - two parameters (acc, val)" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = reduce(arr) do acc, val
        acc + val
    end
    @test (result) == 15.0
end

true  # Test passed
