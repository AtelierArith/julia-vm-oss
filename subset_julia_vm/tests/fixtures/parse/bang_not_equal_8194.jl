# `!=` directly after an identifier with no space (Issue #8194)
#
# The lexer used to greedily fold a trailing `!` into the preceding identifier
# (like `push!`), so `a!=b` was mis-lexed as `a!` `=` `b` (a chained assignment)
# instead of `a` `!=` `b`. A trailing `!` must only join the identifier when it
# does not begin a `!=` / `!==` operator. Identifiers that genuinely end in `!`
# (`sort!`, `push!`) and the unary `!` must keep working.

using Test

@testset "no-space != after identifier (Issue #8194)" begin
    a = 5
    b = 3

    # All spacing variants of `!=` must agree.
    @test (a!=b) == true        # no spaces
    @test (a!= b) == true       # space only after
    @test (a !=b) == true       # space only before
    @test (a != b) == true      # spaces both sides

    # The chained-assignment mis-parse returned the RHS value (3); now it is a
    # proper comparison.
    c = a!=b
    @test c == true
    @test c isa Bool

    # `!==` (not-identical) likewise must not be split.
    @test (a!==b) == true

    # Equal operands.
    @test (5!=5) == false
    @test (1!=2) == true        # literal LHS already worked

    # Trailing-`!` identifiers still parse as names.
    v = [3, 1, 2]
    sort!(v)
    @test v == [1, 2, 3]
    push!(v, 4)
    @test v == [1, 2, 3, 4]

    # Unary `!` (logical not) is unaffected.
    @test (!true) == false
    @test !(a == b) == true
end

true  # Test passed
