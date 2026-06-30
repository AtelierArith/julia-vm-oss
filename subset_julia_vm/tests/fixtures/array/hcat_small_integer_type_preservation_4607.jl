using Test

@testset "hcat preserves small integer element types (#4018, #4607)" begin
    a16 = zeros(Int16, 2)
    b16 = zeros(Int16, 2)
    c16 = zeros(Int16, 2)
    a16[1] = 1
    a16[2] = 2
    b16[1] = 3
    b16[2] = 4
    c16[1] = 5
    c16[2] = 6
    r16 = hcat(a16, b16)
    @test typeof(r16) == Matrix{Int16}
    @test eltype(r16) == Int16
    @test typeof(r16[1, 1]) == Int16
    @test r16[1, 2] == Int16(3)
    r16v = hcat(a16, b16, c16)
    @test typeof(r16v) == Matrix{Int16}
    @test eltype(r16v) == Int16
    @test r16v[2, 3] == Int16(6)

    a32 = zeros(Int32, 2)
    b32 = zeros(Int32, 2)
    a32[1] = 1
    a32[2] = 2
    b32[1] = 3
    b32[2] = 4
    r32 = hcat(a32, b32)
    @test typeof(r32) == Matrix{Int32}
    @test eltype(r32) == Int32
    @test typeof(r32[1, 1]) == Int32
    @test r32[2, 2] == Int32(4)

    au8 = zeros(UInt8, 2)
    bu8 = zeros(UInt8, 2)
    au8[1] = 1
    au8[2] = 2
    bu8[1] = 3
    bu8[2] = 4
    ru8 = hcat(au8, bu8)
    @test typeof(ru8) == Matrix{UInt8}
    @test eltype(ru8) == UInt8
    @test typeof(ru8[1, 1]) == UInt8
    @test ru8[1, 2] == UInt8(3)

    au16 = zeros(UInt16, 2)
    bu16 = zeros(UInt16, 2)
    au16[1] = 1
    au16[2] = 2
    bu16[1] = 3
    bu16[2] = 4
    ru16 = hcat(au16, bu16)
    @test typeof(ru16) == Matrix{UInt16}
    @test eltype(ru16) == UInt16
    @test typeof(ru16[1, 1]) == UInt16
    @test ru16[2, 2] == UInt16(4)

    au32 = zeros(UInt32, 2)
    bu32 = zeros(UInt32, 2)
    au32[1] = 1
    au32[2] = 2
    bu32[1] = 3
    bu32[2] = 4
    ru32 = hcat(au32, bu32)
    @test typeof(ru32) == Matrix{UInt32}
    @test eltype(ru32) == UInt32
    @test typeof(ru32[1, 1]) == UInt32
    @test ru32[1, 2] == UInt32(3)

    au64 = zeros(UInt64, 2)
    bu64 = zeros(UInt64, 2)
    au64[1] = 1
    au64[2] = 2
    bu64[1] = 3
    bu64[2] = 4
    ru64 = hcat(au64, bu64)
    @test typeof(ru64) == Matrix{UInt64}
    @test eltype(ru64) == UInt64
    @test typeof(ru64[1, 1]) == UInt64
    @test ru64[2, 2] == UInt64(4)
end

true
