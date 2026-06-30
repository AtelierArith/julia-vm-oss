using Test

@testset "range adjoint allocation follows Array helper dispatch (Issues #4018, #4572)" begin
    r = range(1.0, step=0.5, length=4)
    a = adjoint(r)

    @test size(a) == (1, 4)
    @test a[1, 1] == 1.0
    @test a[1, 2] == 1.5
    @test a[1, 4] == 2.5

    lin = LinRange(1.0, 3.0, 3)
    lin_row = adjoint(lin)

    @test size(lin_row) == (1, 3)
    @test lin_row[1, 1] == 1.0
    @test lin_row[1, 2] == 2.0
    @test lin_row[1, 3] == 3.0
end

true
