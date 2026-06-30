# Issue #5138: a generator expression `(expr for x in it)` whose body is not a
# plain unary call is wrapped in an *eager* Base.Generator. Passing that
# generator as an iterable to `map`, a comprehension, or another generator
# lowered to `collect(Generator(f, gen))` previously hit
# `collect: unsupported iterator type Generator(...)` because `collect_iterator`
# had no `Value::Generator` arm. The eager generator already holds the
# materialized values, so collecting it must yield those values.

using Test

@testset "map / collect consume an eager generator as an iterable (Issue #5138)" begin
    g = (x^2 for x in 1:3)
    @test map(y -> y + 1, g) == [2, 5, 10]
    @test map(y -> y + 1, (x^2 for x in 1:3)) == [2, 5, 10]
    @test sort(collect(x^2 for x in 1:3)) == [1, 4, 9]
    @test [2v for v in (x^2 for x in 1:3)] == [2, 8, 18]
    @test map(string, (x^2 for x in 1:3)) == ["1", "4", "9"]
    @test reduce(+, (x^2 for x in 1:3)) == 14
end

true  # Test passed
