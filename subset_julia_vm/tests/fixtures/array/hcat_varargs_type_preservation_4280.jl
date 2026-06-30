using Test

@testset "hcat 4+ typed vectors preserves element type (Issue #4280)" begin
    mi = hcat([1], [2], [3], [4])
    @test typeof(mi) == Matrix{Int64}
    @test size(mi) == (1, 4)
    @test mi[1, 4] == 4

    mi5 = hcat([1], [2], [3], [4], [5])
    @test typeof(mi5) == Matrix{Int64}
    @test size(mi5) == (1, 5)
    @test mi5[1, 5] == 5

    mi8 = hcat(Int8[1], Int8[2], Int8[3], Int8[4])
    @test typeof(mi8) == Matrix{Int8}
    @test eltype(mi8) == Int8
    @test mi8[1, 4] == Int8(4)

    mf = hcat([1.0], [2.0], [3.0], [4.0])
    @test typeof(mf) == Matrix{Float64}

    mf32 = hcat(Float32[1], Float32[2], Float32[3], Float32[4])
    @test typeof(mf32) == Matrix{Float32}
    @test eltype(mf32) == Float32
    @test mf32[1, 4] == Float32(4)

    mb = hcat([true], [false], [true], [false])
    @test typeof(mb) == Matrix{Bool}

    promoted = hcat([1], [2.0], [3], [4.0])
    @test typeof(promoted) == Matrix{Float64}
    @test eltype(promoted) == Float64
    @test promoted[1, 1] == 1.0
    @test promoted[1, 4] == 4.0

    promoted2 = hcat([1], [2.0])
    @test typeof(promoted2) == Matrix{Float64}
    @test eltype(promoted2) == Float64
    @test promoted2[1, 1] == 1.0

    mi8_2 = hcat(Int8[1], Int8[2])
    @test typeof(mi8_2) == Matrix{Int8}

    mixed = hcat([1], [2], [3], [4], ["x"])
    @test typeof(mixed) == Matrix{Any}
end

true
