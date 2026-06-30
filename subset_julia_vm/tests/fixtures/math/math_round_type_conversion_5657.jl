using Test

# Issue #5657: round(Int, x) / floor(T, x) / ceil(T, x) / trunc(T, x) (the
# type-conversion form) worked at the top level but failed inside a function body,
# loop, or comprehension (where x is Any-typed) with "No method matching
# round([TypeOf(Int64), Any])". The compiler now routes the (TypeName, value) form
# to the builtin even when the value's type is not statically known.

@testset "round/floor/ceil/trunc(T, x) type conversion (Issue #5657)" begin
    f(x) = round(Int, x)
    @test f(3.7) == 4
    @test f(-2.5) == -2          # round half to even
    @test f(3.7) isa Int

    @test floor(Int, 3.9) == 3
    @test ceil(Int, 3.1) == 4
    @test trunc(Int, 3.9) == 3
    @test trunc(Int, -3.9) == -3

    g(x) = floor(Int, x)
    @test g(3.9) == 3

    # in a comprehension
    @test [round(Int, y) for y in [1.4, 2.6, 3.5]] == [1, 3, 4]

    # via a local variable inside a function body
    function via_local(x)
        t = round(Int, x)
        return t
    end
    @test via_local(7.8) == 8

    # narrower target types
    @test round(Int8, 100.0) == Int8(100)
    @test floor(Int32, 5.9) == Int32(5)

    # The non-type forms are unaffected.
    @test round(3.14159; digits=2) == 3.14
    @test round(2.5) == 2.0
end

true
