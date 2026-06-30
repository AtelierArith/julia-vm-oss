using Test

@testset "broadcast reads reshaped shared backing storage" begin
    arr = [1.0, 2.0, 3.0, 4.0]
    mat = reshape(arr, 2, 2)
    arr[3] = 10.0

    out = mat .+ 1.0
    @test out[1, 1] == 2.0
    @test out[2, 1] == 3.0
    @test out[1, 2] == 11.0
    @test out[2, 2] == 5.0
end

true
