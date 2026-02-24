# titlecase(c::Char) support (Issue #2067)

using Test

@testset "titlecase(::Char)" begin
    @test titlecase('a') == 'A'
    @test titlecase('z') == 'Z'
    @test titlecase('A') == 'A'
    @test titlecase('1') == '1'
end

@testset "titlecase still works on String" begin
    @test titlecase("hello world") == "Hello World"
    @test titlecase("HELLO") == "Hello"
end

true
