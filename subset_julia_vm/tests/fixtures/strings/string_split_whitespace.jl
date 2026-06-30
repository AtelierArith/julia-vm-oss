# Test split(s::String) with no separator — whitespace default (Issue #3571)
# Julia: split(s::AbstractString; limit=0, keepempty=false) =
#         split(s, isspace; limit, keepempty)

using Test

@testset "split(s::String) whitespace default - Issue #3571" begin
    # Basic single-space separation
    @test split("a b c") == ["a", "b", "c"]
    @test split("a") == ["a"]

    # Multiple consecutive whitespace must collapse, leading/trailing trimmed
    @test split("  hi  there  ") == ["hi", "there"]
    @test split("   leading") == ["leading"]
    @test split("trailing   ") == ["trailing"]

    # Empty / all-whitespace inputs return an empty array
    # (Compare via isempty/length to avoid a separate VM limitation
    # where `Vector{String}() == Vector{String}()` lacks a method.)
    @test isempty(split(""))
    @test isempty(split("   "))
    @test isempty(split("\t\n "))

    # Mixed whitespace: tabs, newlines, spaces
    @test split("a\tb\nc") == ["a", "b", "c"]
    @test split("\ta\n b\rc") == ["a", "b", "c"]
end

true
