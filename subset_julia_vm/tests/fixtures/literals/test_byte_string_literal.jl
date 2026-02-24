# Test byte string literal b"..."

using Test

@testset "Byte string literals" begin
    # Basic byte string
    v = b"hello"
    @test length(v) == 5
    @test v[1] == 104  # 'h'
    @test v[2] == 101  # 'e'
    @test v[3] == 108  # 'l'
    @test v[4] == 108  # 'l'
    @test v[5] == 111  # 'o'

    # Single character
    v2 = b"A"
    @test length(v2) == 1
    @test v2[1] == 65

    # Numbers as ASCII
    v3 = b"123"
    @test v3[1] == 49  # '1'
    @test v3[2] == 50  # '2'
    @test v3[3] == 51  # '3'

    # Empty byte string
    v4 = b""
    @test length(v4) == 0
end

true
