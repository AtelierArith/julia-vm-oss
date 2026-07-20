# eval-redefined functions must update calls through Function values.

using Test

q9742(x) = x + 100

@testset "eval redefined Function value dispatch (Issue #9742)" begin
    @test map(q9742, [1, 2, 3]) == [101, 102, 103]

    eval(:(q9742(x) = x + 1))

    @test map(q9742, [1, 2, 3]) == [2, 3, 4]
    @test q9742.(1:3) == [2, 3, 4]
    @test q9742(1) == 2
end

true  # Test passed
