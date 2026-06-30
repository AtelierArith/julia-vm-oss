# Test that print(::Vector{T}) uses show-style quoting for elements
# (Issue #3574). Julia's `print(io, ::AbstractVector)` calls `show(io, x)`
# for each element, which adds quotes around String/Char.

using Test

@testset "String vector show quotes - Issue #3574" begin
    # Vector of String literals shows with quotes inline.
    @test sprint(print, ["a", "b"]) == "[\"a\", \"b\"]"
    @test sprint(print, ["foo", "bar", "baz"]) == "[\"foo\", \"bar\", \"baz\"]"

    # Vector of Char shows with single quotes.
    @test sprint(print, ['a', 'b']) == "['a', 'b']"

    # Numeric vectors are unaffected (no quotes).
    @test sprint(print, [1, 2, 3]) == "[1, 2, 3]"
    @test sprint(print, [1.0, 2.5]) == "[1.0, 2.5]"
end

true
