using Test

# Tests for integer type limits
@testset "integer type limits" begin
    # Int32 limits
    @test typemax(Int32) == 2147483647
    @test typemin(Int32) == -2147483648

    # Int16 limits
    @test typemax(Int16) == 32767
    @test typemin(Int16) == -32768

    # Int8 limits
    @test typemax(Int8) == 127
    @test typemin(Int8) == -128

    # Verify typemax > typemin for signed types
    @test typemax(Int64) > typemin(Int64)
    @test typemax(Int32) > typemin(Int32)
    @test typemax(Int16) > typemin(Int16)
    @test typemax(Int8) > typemin(Int8)
end

true
