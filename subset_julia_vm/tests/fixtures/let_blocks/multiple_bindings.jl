# Multiple bindings in let block

using Test

@testset "Let block with multiple variable bindings" begin
    result = let x = 1, y = 2, z = 3
        x + y + z
    end
    println(result)  # 6
    @test (result) == 6.0
end

true  # Test passed
