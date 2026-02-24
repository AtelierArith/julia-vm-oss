# uppercase(c::Char) and lowercase(c::Char) support (Issue #2064)

using Test

@testset "uppercase(::Char)" begin
    @test uppercase('a') == 'A'
    @test uppercase('z') == 'Z'
    @test uppercase('A') == 'A'
    @test uppercase('1') == '1'
end

@testset "lowercase(::Char)" begin
    @test lowercase('A') == 'a'
    @test lowercase('Z') == 'z'
    @test lowercase('a') == 'a'
    @test lowercase('1') == '1'
end

@testset "uppercase/lowercase still work on String" begin
    @test uppercase("hello") == "HELLO"
    @test lowercase("HELLO") == "hello"
end

true
