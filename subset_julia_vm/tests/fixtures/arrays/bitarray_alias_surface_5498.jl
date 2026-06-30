using Test

@testset "BitArray alias surface (Issue #5498)" begin
    @test BitVector === BitArray{1}
    @test BitMatrix === BitArray{2}
    @test BitVector <: AbstractVector{Bool}
    @test BitMatrix <: AbstractMatrix{Bool}

    @test typeof(falses(3)) === BitVector
    @test typeof(trues(3)) === BitVector
    @test typeof(falses(2, 2)) === BitMatrix
    @test typeof(trues(2, 2)) === BitMatrix
    @test typeof(trues()) === BitArray{0}
    @test typeof(falses(2, 1, 2)) === BitArray{3}
    @test typeof(trues(2, 1, 1, 1)) === BitArray{4}

    @test falses(3) == Bool[false, false, false]
    @test trues(2, 2) == reshape(Bool[true, true, true, true], 2, 2)
    @test size(trues()) == ()
    @test size(falses(2, 1, 2)) == (2, 1, 2)

    @test typeof(copy(falses(3))) === BitVector
    @test typeof(copy(falses(2, 2))) === BitMatrix
    @test typeof(similar(falses(3))) === BitVector
    @test typeof(similar(falses(2, 2))) === BitMatrix
    @test typeof(similar(falses(3), Bool)) === BitVector
    @test typeof(similar(falses(3), Bool, 2)) === BitVector
    @test typeof(similar(falses(2, 2), Bool, (2, 1, 1))) === BitArray{3}
    @test typeof(similar(falses(3), Int64)) === Vector{Int64}

    @test typeof([1, 2, 3] .== 2) === BitVector
    @test typeof(reshape([1, 2, 3, 4], 2, 2) .== 2) === BitMatrix
    @test typeof(reshape([0, 1, 0, 2], 2, 1, 2) .== 0) === BitArray{3}
    @test typeof(iszero.([0, 1, 0])) === BitVector
end

true
