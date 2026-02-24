# isascii(::String) - check if all characters in string are ASCII (Issue #2046)

using Test

@testset "isascii for Char (existing)" begin
    @test isascii('a') == true
    @test isascii('A') == true
    @test isascii('0') == true
    @test isascii(' ') == true
end

@testset "isascii for String (Issue #2046)" begin
    @test isascii("hello") == true
    @test isascii("Hello World") == true
    @test isascii("abc123") == true
    @test isascii("") == true
    @test isascii("!@#\$%") == true
end

true
