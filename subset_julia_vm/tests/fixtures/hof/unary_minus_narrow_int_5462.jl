using Test

@testset "HOF unary minus narrow integers (Issue #5462)" begin
    i32s = Int32[-3, 0, 4]
    neg_i32s = map(-, i32s)
    @test neg_i32s == Int32[3, 0, -4]
    @test typeof(neg_i32s) == Vector{Int32}

    u32s = UInt32[3, 0, 4]
    neg_u32s = map(-, u32s)
    @test neg_u32s == UInt32[-UInt32(3), UInt32(0), -UInt32(4)]
    @test typeof(neg_u32s) == Vector{UInt32}

    @test -Int8(-3) == Int8(3)
    @test typeof(-Int8(-3)) == Int8
    @test -UInt8(3) == UInt8(253)
    @test typeof(-UInt8(3)) == UInt8
end

true
