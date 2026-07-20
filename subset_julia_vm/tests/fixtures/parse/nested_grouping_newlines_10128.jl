# Issue #10128 (CompileSpeed): deeply nested/grouped expressions with
# newlines in the middle of an operator chain must keep parsing identically
# after `peek_non_newline_token`/`peek_non_newline_token_after_current`
# stopped rebuilding a full `Lexer::new(self.source)` (and its O(source
# length) `SourceMap` scan) on every newline encountered inside
# `(...)`/`[...]`/`{...}` groupings. This is a regression test for the
# continuation-check logic itself (Pratt loop, expressions/mod.rs), not for
# any single language feature, so it deliberately stacks several independent
# grouping/newline shapes together.
using Test

@testset "Deeply nested parenthesized arithmetic across newlines (Issue #10128)" begin
    x = (
        1 +
        2 * (
            3
            + 4
            - (5
               * 6)
        )
        - 7
    )
    @test x == -52
end

@testset "Nested vector/tuple literals across newlines (Issue #10128)" begin
    y = [
        1, 2,
        3, (4
            + 5),
        [6, 7,
         8],
    ]
    @test y == Any[1, 2, 3, 9, [6, 7, 8]]
end

@testset "Function signature with grouped default value across newlines (Issue #10128)" begin
    function f(a, b,
               c = (1
                    + 2),
               ; d = 3)
        return a + b + c + d
    end
    @test f(1, 2) == 9
end

@testset "Generator with parenthesized filter clause across newlines (Issue #10128)" begin
    z = (a for a in 1:3
         if a
            > 1)
    @test collect(z) == [2, 3]
end

@testset "Deeply nested parentheses (many levels, each split across a newline) (Issue #10128)" begin
    w = (
        (
            (
                (
                    1
                    + 2
                )
                + 3
            )
            + 4
        )
        + 5
    )
    @test w == 15
end

true # Test passed
