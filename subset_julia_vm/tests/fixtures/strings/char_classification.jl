# Test character classification functions: iscntrl, ispunct, isxdigit (Issue #1874)

using Test

@testset "iscntrl" begin
    @test iscntrl('\n') == true
    @test iscntrl('\t') == true
    @test iscntrl('\r') == true
    @test iscntrl('a') == false
    @test iscntrl(' ') == false
    @test iscntrl('0') == false
end

@testset "ispunct" begin
    @test ispunct('!') == true
    @test ispunct('.') == true
    @test ispunct(',') == true
    @test ispunct(':') == true
    @test ispunct('?') == true
    @test ispunct('@') == true
    @test ispunct('[') == true
    @test ispunct('{') == true
    @test ispunct('~') == true
    @test ispunct('a') == false
    @test ispunct('0') == false
    @test ispunct(' ') == false
end

@testset "isxdigit" begin
    @test isxdigit('0') == true
    @test isxdigit('9') == true
    @test isxdigit('a') == true
    @test isxdigit('f') == true
    @test isxdigit('A') == true
    @test isxdigit('F') == true
    @test isxdigit('g') == false
    @test isxdigit('G') == false
    @test isxdigit('z') == false
    @test isxdigit(' ') == false
end

true
