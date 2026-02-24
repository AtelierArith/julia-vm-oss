# reverse(::String) returns reversed String, not Vector{Char} (Issue #2053)

using Test

@testset "reverse(::String) returns String" begin
    @test reverse("hello") == "olleh"
    @test reverse("abc") == "cba"
    @test reverse("a") == "a"
    @test reverse("ab") == "ba"
    @test reverse("racecar") == "racecar"
end

@testset "reverse roundtrip" begin
    @test reverse(reverse("hello")) == "hello"
end

true
