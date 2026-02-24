# Nested let blocks

using Test

@testset "Nested let blocks" begin
    result = let x = 1
        let y = 2
            x + y
        end
    end
    println(result)  # 3
    @test (result) == 3.0
end

true  # Test passed
