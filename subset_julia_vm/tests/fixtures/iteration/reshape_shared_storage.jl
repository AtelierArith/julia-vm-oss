using Test

@testset "iteration reads reshaped shared backing storage" begin
    arr = [1, 2, 3, 4]
    mat = reshape(arr, 2, 2)
    arr[3] = 10

    total = 0
    for x in mat
        total = total + x
    end
    @test total == 17
end

true
