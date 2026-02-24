# Variable shadowing with let block

using Test

@testset "Let block variable shadowing with scope restoration" begin
    x = 100
    result = let x = 5
        x * 2
    end
    # result should be 10 (from inner x = 5)
    # x should still be 100 (restored after let block)
    println(result)  # 10
    println(x)       # 100
    @test (result + x) == 110.0
end

true  # Test passed
