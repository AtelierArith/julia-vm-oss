using Test

@testset "Range size methods and 2D broadcast" begin
    # LinRange size
    lr = range(-2.0, 1.0; length=3)
    @test size(lr) == (3,)
    @test length(lr) == 3

    # StepRangeLen size
    sr = range(0.0; step=0.5, length=5)
    @test size(sr) == (5,)
    @test length(sr) == 5

    # 2D broadcast with ranges using transpose (Issue #2689)
    xs = range(-2.0, 1.0; length=3)
    ys = range(1.2, -1.2; length=2)
    C = xs' .+ im .* ys
    @test length(C) == 6
    @test size(C) == (2, 3)

    # Verify individual elements
    @test C[1, 1] == -2.0 + 1.2im
    @test C[2, 1] == -2.0 - 1.2im
    @test C[1, 3] == 1.0 + 1.2im
    @test C[2, 3] == 1.0 - 1.2im
end

true
