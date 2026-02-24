# Basic if statement

using Test

@testset "Basic if statement" begin
    x = 5
    result = 0
    if x > 3
        result = 1
    end
    @test (result) == 1.0
end

true  # Test passed
