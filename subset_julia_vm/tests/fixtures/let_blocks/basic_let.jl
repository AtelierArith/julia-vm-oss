# Basic let block

using Test

@testset "Basic let block with single binding" begin
    result = let x = 10
        x + 5
    end
    println(result)  # 15
    @test (result) == 15.0
end

true  # Test passed
