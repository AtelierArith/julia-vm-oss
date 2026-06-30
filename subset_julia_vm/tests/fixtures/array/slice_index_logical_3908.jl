using Test

@testset "slice index arrays read logical reshaped elements (Issue #3908)" begin
    data = [10, 20, 30, 40]

    idx = [1, 4, 3, 4]
    reshaped_idx = reshape(idx, 4)
    selected = data[reshaped_idx]

    @test selected == [10, 40, 30, 40]
    @test typeof(selected) == Vector{Int64}

    mask = [true, false, true, false]
    reshaped_mask = reshape(mask, 4)
    mask[2] = true
    masked = data[reshaped_mask]

    @test masked == [10, 20, 30]
    @test typeof(masked) == Vector{Int64}
end

true
