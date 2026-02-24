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
