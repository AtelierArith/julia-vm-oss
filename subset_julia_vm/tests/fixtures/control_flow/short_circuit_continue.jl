# Test short-circuit && continue pattern

using Test

@testset "Short-circuit && continue pattern" begin
    result = 0
    for i in 1:10
        i % 2 == 0 && continue
        result += i
    end
    @test (result) == 25.0
end

true  # Test passed
