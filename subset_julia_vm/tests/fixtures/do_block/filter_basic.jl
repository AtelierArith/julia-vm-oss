# Do block with filter - returns filtered array

using Test

@testset "Do block with filter - returns filtered array length" begin
    arr = [1.0, 2.0, 3.0, 4.0, 5.0]
    result = filter(arr) do x
        x > 2.5
    end
    @test (length(result)) == 3
end

true  # Test passed
