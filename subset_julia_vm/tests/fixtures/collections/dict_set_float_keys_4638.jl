using Test

@testset "Dict and Set floating-point keys (#4638)" begin
    d32 = Dict(Float32(1) => Int8(2), Float32(3) => Int8(4))
    @test typeof(d32) === Dict{Float32, Int8}
    @test keytype(d32) === Float32
    @test valtype(d32) === Int8
    @test d32[Float32(3)] == Int8(4)
    @test haskey(d32, Float32(1))
    @test haskey(d32, 1.0)
    @test haskey(d32, 1)

    d64 = Dict(1.0 => 2)
    @test typeof(d64) === Dict{Float64, Int64}
    @test keytype(d64) === Float64
    @test valtype(d64) === Int64
    @test d64[1.0] == 2
    @test haskey(d64, 1)

    s32 = Set(Float32[1, 2])
    @test length(s32) == 2
    @test Float32(1) in s32
    @test 1.0 in s32

    s64 = Set(Float64[-0.0])
    @test -0.0 in s64
    @test !(0.0 in s64)
end

true
