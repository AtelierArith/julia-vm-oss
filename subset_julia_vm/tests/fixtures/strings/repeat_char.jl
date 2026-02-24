# repeat(c::Char, n) returns String of repeated character (Issue #2057)

using Test

@testset "repeat(::Char, ::Int) basic" begin
    @test repeat('a', 5) == "aaaaa"
    @test repeat('-', 3) == "---"
    @test repeat('x', 1) == "x"
    @test repeat('z', 0) == ""
end

@testset "repeat(::Char, ::Int) special chars" begin
    @test repeat(' ', 4) == "    "
    @test repeat('0', 3) == "000"
end

true
