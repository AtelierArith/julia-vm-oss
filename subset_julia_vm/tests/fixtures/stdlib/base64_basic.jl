# Test Base64 encoding and decoding (Issue #1846)

using Test
using Base64

@testset "base64encode basic strings" begin
    @test base64encode("") == ""
    @test base64encode("f") == "Zg=="
    @test base64encode("fo") == "Zm8="
    @test base64encode("foo") == "Zm9v"
    @test base64encode("foob") == "Zm9vYg=="
    @test base64encode("fooba") == "Zm9vYmE="
    @test base64encode("foobar") == "Zm9vYmFy"
end

@testset "base64encode common strings" begin
    @test base64encode("Hello, World!") == "SGVsbG8sIFdvcmxkIQ=="
    @test base64encode("Hello!") == "SGVsbG8h"
    @test base64encode("abc") == "YWJj"
    @test base64encode("A") == "QQ=="
end

@testset "base64decode byte values" begin
    result = base64decode("SGVsbG8h")
    @test length(result) == 6
    @test Int64(result[1]) == 72   # 'H'
    @test Int64(result[2]) == 101  # 'e'
    @test Int64(result[3]) == 108  # 'l'
    @test Int64(result[4]) == 108  # 'l'
    @test Int64(result[5]) == 111  # 'o'
    @test Int64(result[6]) == 33   # '!'
end

@testset "base64decode empty" begin
    result = base64decode("")
    @test length(result) == 0
end

@testset "base64decode single char padding" begin
    result = base64decode("Zg==")
    @test length(result) == 1
    @test Int64(result[1]) == 102  # 'f'
end

@testset "base64decode two chars padding" begin
    result = base64decode("Zm8=")
    @test length(result) == 2
    @test Int64(result[1]) == 102  # 'f'
    @test Int64(result[2]) == 111  # 'o'
end

@testset "base64decode no padding" begin
    result = base64decode("Zm9v")
    @test length(result) == 3
    @test Int64(result[1]) == 102  # 'f'
    @test Int64(result[2]) == 111  # 'o'
    @test Int64(result[3]) == 111  # 'o'
end

true
