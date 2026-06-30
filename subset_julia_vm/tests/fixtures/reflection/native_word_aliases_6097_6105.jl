using Test

@testset "native word aliases (Issues #6097, #6105)" begin
    native_int = Sys.WORD_SIZE == 32 ? Int32 : Int64
    native_uint = Sys.WORD_SIZE == 32 ? UInt32 : UInt64

    @test Int === native_int
    @test UInt === native_uint

    @test typeof(Int(7)) === native_int
    @test typeof(UInt(7)) === native_uint

    @test Vector{Int} === Vector{native_int}
    @test Vector{UInt} === Vector{native_uint}
    @test Tuple{Int, UInt} === Tuple{native_int, native_uint}
end

true
