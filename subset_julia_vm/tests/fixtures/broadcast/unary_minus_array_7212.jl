using Test

@testset "Unary minus on arrays (Issue #7212)" begin
    xs = [1.0, 2.0, 3.0]
    @test -xs == [-1.0, -2.0, -3.0]
    @test -((xs .- 1.0) .^ 2) == [-0.0, -1.0, -4.0]

    ys = [1, 2, 3]
    @test -ys == [-1, -2, -3]
    @test Base.:-(ys) == [-1, -2, -3]
end

true
