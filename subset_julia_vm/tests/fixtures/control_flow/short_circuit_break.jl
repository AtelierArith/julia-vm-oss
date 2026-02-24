# Test short-circuit && break pattern

using Test

@testset "Short-circuit && break pattern" begin
    result = 0
    for i in 1:10
        i > 5 && break
        result += i
    end
    @test (result) == 15.0
end

true  # Test passed
