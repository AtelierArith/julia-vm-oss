using Test

f10618(x) = 1

@testset "empty filtered generator sum distinguishes mapped bodies (Issue #10618)" begin
    @test sum(x for x in [1, 2, 3] if x >= 4) == 0
    @test_throws ArgumentError sum(f10618(x) for x in [1, 2, 3] if x >= 4)
    @test_throws ArgumentError sum((x + 0) for x in [1, 2, 3] if x >= 4)
    @test sum(f10618(x) for x in [1, 2, 3] if x >= 4; init=10) == 10
    @test sum(f10618(x) for x in [1, 2, 3] if x >= 3) == 1
end

true
