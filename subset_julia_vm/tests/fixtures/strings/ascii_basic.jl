# Test ascii(s) validates string contains only ASCII characters (Issue #1842)

using Test

@testset "ascii with valid ASCII strings" begin
    @test ascii("hello") == "hello"
    @test ascii("abc123") == "abc123"
    @test ascii("") == ""
    @test ascii(" ") == " "
    @test ascii("Hello, World!") == "Hello, World!"
end

@testset "ascii with special ASCII characters" begin
    @test ascii("tab\there") == "tab\there"
    @test ascii("newline\nhere") == "newline\nhere"
    @test ascii("0123456789") == "0123456789"
    @test ascii("!@#\$%^&*()") == "!@#\$%^&*()"
end

@testset "ascii returns same string" begin
    s = "test"
    @test ascii(s) === s
end

true
