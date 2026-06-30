# Issue #7196: in a whitespace-separated matrix/`hcat` row, a `-`/`+` with a
# space BEFORE it but NO space AFTER it begins a new (unary-signed) ELEMENT,
# not a binary `-`/`+`. The parser previously treated `[0.20 -0.26; ...]` as
# `[0.20 - 0.26; ...]` (binary subtraction), producing a row with one column
# and failing lowering with MalformedMatrix "inconsistent column count".
#
# Disambiguation rule (matches upstream Julia):
#   `[1 -2]`   -> 1x2  two elements        (space before `-`, none after)
#   `[1 - 2]`  -> 1x1  binary subtraction  (spaces on both sides)
#   `[1+2]`    -> 1x1  binary, no spaces
#   `[1 *2]`   -> 1x1  `*` is NOT affected (only `+`/`-` have a unary form)
# Comma vectors (`[1, -2]`) and ordinary subtraction (`x - y`, `f(1 -2)`) are
# unchanged.
using Test

@testset "Issue #7196: matrix-row negative-element disambiguation" begin
    # The exact repro from the issue.
    W = [0.20 -0.26; 0.23 0.22]
    @test size(W) == (2, 2)
    @test W[1, 1] == 0.20
    @test W[1, 2] == -0.26
    @test W[2, 1] == 0.23
    @test W[2, 2] == 0.22

    # `space then -value` (no trailing space) => new element.
    @test size([1 -2]) == (1, 2)
    @test [1 -2][1] == 1
    @test [1 -2][2] == -2

    # `space - space` (both sides) => binary subtraction, single element.
    @test size([1 - 2]) == (1,)
    @test [1 - 2][1] == -1

    # No spaces => binary, single element.
    @test size([1+2]) == (1,)
    @test [1+2][1] == 3

    # Unary `+` element.
    @test size([1 +2]) == (1, 2)
    @test [1 +2][2] == 2

    # Three space-separated elements with leading signs.
    @test size([1 -2 +3]) == (1, 3)
    @test [1 -2 +3] == [1, -2, 3]'  # row vector compare via transpose of column
    @test [1 -2 +3][1] == 1
    @test [1 -2 +3][2] == -2
    @test [1 -2 +3][3] == 3

    # Two rows, each disambiguated independently.
    M = [1 -2; 3 4]
    @test size(M) == (2, 2)
    @test M[1, 2] == -2
    M2 = [1 1; 2 -3]
    @test M2[2, 2] == -3

    # `*` is NOT subject to the rule: `[1 *2 3]` is `[1*2, 3]` = two elements.
    @test size([1 *2 3]) == (1, 2)
    @test [1 *2 3][1] == 2
    @test [1 *2 3][2] == 3

    # `[1 - 2 3]` => binary `1 - 2` then a new element `3`.
    @test size([1 - 2 3]) == (1, 2)
    @test [1 - 2 3][1] == -1
    @test [1 - 2 3][2] == 3

    # Variables behave like literals.
    a = 10
    b = 3
    @test size([a -b]) == (1, 2)
    @test [a -b][2] == -3
    @test size([a - b]) == (1,)
    @test [a - b][1] == 7

    # Float element with an exponent after a space-separated sign.
    @test size([2 -1.5e3]) == (1, 2)
    @test [2 -1.5e3][2] == -1500.0

    # Typed matrix literal `Float64[...]` obeys the same rule.
    @test size(Float64[1 -2]) == (1, 2)
    @test Float64[1 -2][2] == -2.0
    @test size(Float64[0.20 -0.26; 0.23 0.22]) == (2, 2)

    # Comma vectors and ordinary subtraction are unaffected.
    @test [1, -2] == [1, -2]
    @test [1, -2][2] == -2
    f(x) = x
    @test f(1 -2) == -1     # call argument: binary subtraction
    @test (1 - 2) == -1
end

true
