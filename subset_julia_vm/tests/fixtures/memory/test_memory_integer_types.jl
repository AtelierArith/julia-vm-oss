using Test

@testset "Memory{T} with small integer types" begin
    # Int32
    m32 = Memory{Int32}(3)
    m32[1] = Int32(100)
    m32[2] = Int32(200)
    m32[3] = Int32(300)
    @test m32[1] == 100
    @test m32[2] == 200
    @test m32[3] == 300
    @test length(m32) == 3

    # Int8 (range -128 to 127)
    m8 = Memory{Int8}(4)
    m8[1] = Int8(1)
    m8[2] = Int8(127)
    m8[3] = Int8(-1)
    m8[4] = Int8(-128)
    @test m8[1] == 1
    @test m8[2] == 127
    @test m8[3] == -1
    @test m8[4] == -128

    # UInt8
    mu8 = Memory{UInt8}(3)
    mu8[1] = UInt8(0)
    mu8[2] = UInt8(128)
    mu8[3] = UInt8(255)
    @test mu8[1] == 0
    @test mu8[2] == 128
    @test mu8[3] == 255

    # Float32
    mf32 = Memory{Float32}(3)
    mf32[1] = Float32(1.5)
    mf32[2] = Float32(2.5)
    mf32[3] = Float32(3.5)
    @test mf32[1] == 1.5f0
    @test mf32[2] == 2.5f0
    @test mf32[3] == 3.5f0

    # fill! with typed values
    fill!(m32, Int32(0))
    @test m32[1] == 0
    @test m32[2] == 0
    @test m32[3] == 0
end

true
