using Test

function literal_destructure_tail_10444()
    (a, b) = (1, 2)
end

function duplicate_target_tail_10444()
    (a, a) = (1, 2)
end

@testset "literal destructuring tail value (Issue #10444)" begin
    @test literal_destructure_tail_10444() == (1, 2)
    @test duplicate_target_tail_10444() == (1, 2)

    (x, y) = (Int8(3), 4.5)
    @test x isa Int8
    @test y isa Float64
    @test x == 3
    @test y == 4.5
end

true
