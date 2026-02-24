# Test foreach function - applies function for side effects

using Test

@testset "foreach - apply function for side effects" begin
    # foreach returns nothing
    result = foreach(x -> x * 2, [1, 2, 3])
    @test result === nothing

    # foreach with range returns nothing
    result2 = foreach(x -> x^2, 1:4)
    @test result2 === nothing

    # foreach with tuple returns nothing
    result3 = foreach(x -> x + 10, (5, 6, 7))
    @test result3 === nothing

    # foreach with empty array returns nothing (use x+0 instead of just x)
    result4 = foreach(x -> x + 0, Int64[])
    @test result4 === nothing
end

true
