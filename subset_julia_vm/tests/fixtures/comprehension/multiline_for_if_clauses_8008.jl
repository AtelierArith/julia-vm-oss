# Multi-line comprehension / generator: a newline before a `for`/`if` guard (or
# before a 2D binding-separating comma) is insignificant inside `[...]`/`(...)`,
# so the multi-line form must parse and evaluate identically to the single-line
# form (Issue #8008).

using Test

@testset "Multi-line comprehension/generator clauses (Issue #8008)" begin
    xs = [1, -2, 3, -4, 5]

    # newline before the `if` guard (the issue's MWE)
    pos = [x for x in xs
           if x > 0]
    @test pos == [1, 3, 5]

    # newline before BOTH the first `for` and the `if` guard
    big = [x for x in 1:5
             if x > 2]
    @test big == [3, 4, 5]

    # transform body with a multi-line `if` guard
    sq = [x^2 for x in 1:5
            if isodd(x)]
    @test sq == [1, 9, 25]

    # 2D comprehension with a newline after the binding-separating comma
    grid = [10i + j for i in 1:2,
                        j in 1:3]
    @test grid == [11 12 13; 21 22 23]

    # multi-line generator consumed by a function call
    s = sum(x for x in xs
            if x > 0)
    @test s == 9
end

true  # Test passed
