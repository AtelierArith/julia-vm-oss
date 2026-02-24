# Let block used in expression

using Test

@testset "Let block used as expression in arithmetic" begin
    value = 1 + (let x = 10; x * 2 end) + 3
    println(value)  # 24 = 1 + 20 + 3
    @test (value) == 24.0
end

true  # Test passed
