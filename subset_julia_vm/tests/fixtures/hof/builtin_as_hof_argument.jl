# Builtin functions passed as HOF arguments (Issue #2070)
# Tests that builtin functions like uppercase, lowercase, string
# can be passed directly to higher-order functions like map.

using Test

@testset "Builtin functions as HOF arguments" begin
    # map with uppercase builtin
    @test map(uppercase, ["hello", "world"]) == ["HELLO", "WORLD"]

    # map with lowercase builtin
    @test map(lowercase, ["HELLO", "WORLD"]) == ["hello", "world"]

    # map with string builtin
    @test map(string, [1, 2, 3]) == ["1", "2", "3"]
    r = map(string, [42])
    @test r[1] == "42"
    @test typeof(r[1]) === String

    # Prevention (Issue #10538, follow-up to #10512): named callables whose
    # tfunc result is a fixed type independent of the collection's element
    # type (`repr`, `bitstring`) must not be statically inferred as the
    # *input* element type either — indexing the mapped result must produce
    # the callable's own result type.
    r2 = map(repr, [42])
    @test r2[1] == "42"
    @test typeof(r2[1]) === String

    r3 = map(bitstring, [42])
    @test r3[1] == "0000000000000000000000000000000000000000000000000000000000101010"
    @test typeof(r3[1]) === String

    # Direct builtin via variable still works
    f = uppercase
    @test f("hello") == "HELLO"

    # Named function wrapper still works
    g(x) = uppercase(x)
    @test map(g, ["a", "b"]) == ["A", "B"]

    # Lambda wrapper still works
    @test map(x -> uppercase(x), ["a", "b"]) == ["A", "B"]
end

true
