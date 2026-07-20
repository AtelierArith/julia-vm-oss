# Basic function definition and call

using Test

function double(x)
    x * 2
end

@testset "Basic function definition and call" begin
    @test (double(5)) == 10.0
end

true  # Test passed
