# Issue #6749: pure-Julia public regex search wrappers count(::Regex, s) and
# findall(::Regex, s). The regex engine (match/eachmatch) stays as the Rust
# boundary; these public wrappers are built on top of eachmatch in pure Julia.
# Values verified against upstream julia 1.12.

using Test

@testset "count(::Regex, ::AbstractString) (Issue #6749)" begin
    @test count(r"\d", "a1b2c3") == 3
    @test count(r"\d+", "a12b345") == 2
    @test count(r"x", "abc") == 0
    @test count(r"\d", "") == 0
    @test count(r"\d", "αβ1γ2") == 2  # unicode haystack
end

@testset "findall(::Regex, ::AbstractString) (Issue #6749)" begin
    @test findall(r"\d", "a1b2c3") == [2:2, 4:4, 6:6]
    @test findall(r"\d+", "a12b345") == [2:3, 5:7]
    @test findall(r"x", "abc") == UnitRange{Int64}[]
    # byte-offset ranges for a unicode haystack (α,β,γ are 2 bytes each)
    @test findall(r"\d", "αβ1γ2") == [5:5, 8:8]
    # NB: the result vector's eltype is not asserted here — sjulia currently
    # degrades an empty `Vector{UnitRange{Int64}}` to `Vector{Any}` after push!
    # (affects the merged string findall too; tracked by #6768), while the
    # element *values* match upstream exactly.
    @test first(findall(r"\d", "a1b2c3")) === 2:2
end

true
