# Multi-line typed array literals (Issue #8188)
#
# `T[...]` with elements split across lines must parse like the untyped `[...]`
# form. Previously only single-line typed literals and multi-line UNtyped
# literals parsed; the "typed + multi-line" combination raised a parse error
# because the newline right after `[` was not skipped.

using Test

@testset "multi-line typed array literals (Issue #8188)" begin
    # Multi-line typed vector with a trailing comma.
    y = Bool[
        true,
        false,
    ]
    @test y == Bool[true, false]
    @test length(y) == 2

    # Multi-line typed vector without a trailing comma.
    z = Int[
        1,
        2,
        3
    ]
    @test z == [1, 2, 3]

    # String element type, multi-line.
    s = String[
        "a",
        "b",
    ]
    @test s == ["a", "b"]

    # Single-line typed literals are unaffected.
    @test Int[1, 2, 3] == [1, 2, 3]
    @test Bool[true, false, true] == Bool[true, false, true]

    # Untyped multi-line still works.
    w = [
        10,
        20,
    ]
    @test w == [10, 20]

    # Multi-line indexing (newlines are cosmetic around the index list).
    v = [10, 20, 30]
    @test v[
        2
    ] == 20
    m = [1 2; 3 4]
    @test m[
        1, 2
    ] == 2
end

true  # Test passed
