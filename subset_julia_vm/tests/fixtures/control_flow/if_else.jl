# If-else statement

using Test

@testset "If-else statement" begin
    x = 2
    result = 0
    if x > 3
        result = 1
    else
        result = 2
    end
    @test (result) == 2.0
end

true  # Test passed
