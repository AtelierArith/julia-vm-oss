# Test character classification predicates (Issue #1885)

using Test

@testset "isdigit" begin
    @test isdigit('0') == true
    @test isdigit('5') == true
    @test isdigit('9') == true
    @test isdigit('a') == false
    @test isdigit('Z') == false
    @test isdigit(' ') == false
end

@testset "isletter" begin
    @test isletter('a') == true
    @test isletter('z') == true
    @test isletter('A') == true
    @test isletter('Z') == true
    @test isletter('0') == false
    @test isletter(' ') == false
end

@testset "isuppercase" begin
    @test isuppercase('A') == true
    @test isuppercase('Z') == true
    @test isuppercase('a') == false
    @test isuppercase('z') == false
    @test isuppercase('0') == false
end

@testset "islowercase" begin
    @test islowercase('a') == true
    @test islowercase('z') == true
    @test islowercase('A') == false
    @test islowercase('Z') == false
    @test islowercase('0') == false
end

@testset "isascii" begin
    @test isascii('A') == true
    @test isascii('0') == true
    @test isascii(' ') == true
end

@testset "isspace" begin
    @test isspace(' ') == true
    @test isspace('A') == false
    @test isspace('0') == false
end

@testset "isprint" begin
    @test isprint('A') == true
    @test isprint(' ') == true
    @test isprint('0') == true
    @test isprint('~') == true
end

true
