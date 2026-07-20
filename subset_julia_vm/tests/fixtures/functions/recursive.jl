# Recursive factorial function: 5! = 120

using Test

function factorial(n)
    if n <= 1
        return 1
    end
    n * factorial(n - 1)
end

@testset "Recursive factorial function" begin
    @test (factorial(5)) == 120.0
end

true  # Test passed
