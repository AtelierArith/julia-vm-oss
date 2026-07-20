using Test

@testset "Generator getindex is unsupported (Issue #9457)" begin
    @test_throws MethodError (x^2 for x in 1:4)[2]
    @test_throws MethodError (x for x in 1:5 if x > 2)[1]
end

@testset "Generator consumers use iterate after getindex removal" begin
    @test first(x^2 for x in 2:4) == 4
    @test any(x > 3 for x in 1:4) === true
    @test any(x > 10 for x in 1:4) === false
    @test all(x > 0 for x in 1:4) === true
    @test all(x > 2 for x in 1:4) === false
    @test join((x for x in 1:3), ",") == "1,2,3"
    @test join((x for x in 1:3), ", ", " and ") == "1, 2 and 3"
    @test join((x for x in 1:5 if x > 2), ", ") == "3, 4, 5"
    @test Tuple(x for x in 1:3) == (1, 2, 3)
    @test [2y for y in (x^2 for x in 1:3)] == [2, 8, 18]
end

true
