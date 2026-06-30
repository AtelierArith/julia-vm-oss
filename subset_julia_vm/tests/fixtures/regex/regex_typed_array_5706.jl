using Test

# Issue #5706: a typed array literal Regex[...] failed to compile with
# "Cannot convert Regex to I64" — the element-type mapping had no Regex case, so it
# fell through to the numeric default and tried to coerce each element to Int64.
# Regex / RegexMatch are boxed scalar values stored verbatim in a boxed slot.

@testset "typed Regex array literal (Issue #5706)" begin
    v = Regex[r"a", r"b"]
    @test length(v) == 2
    @test eltype(v) == Regex
    @test v isa Vector{Regex}
    @test v[1] isa Regex
    @test match(v[1], "xax").match == "a"
    @test occursin(v[2], "zbz") == true

    # Single-element and push! onto a populated typed array.
    one = Regex[r"\d+"]
    @test length(one) == 1
    push!(one, r"[a-z]+")
    @test length(one) == 2
    @test occursin(one[2], "hello") == true

    # RegexMatch element type.
    m = match(r"(\d+)", "a12")
    ms = RegexMatch[m]
    @test length(ms) == 1
    @test eltype(ms) == RegexMatch
    @test ms[1].match == "12"
end

true
