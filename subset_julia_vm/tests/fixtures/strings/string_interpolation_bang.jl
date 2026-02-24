# String interpolation should NOT include `!` as part of identifier
# In Julia, "$name!" interpolates `name` and `!` is literal text (Issue #2130)

using Test

@testset "String interpolation with bang" begin
    name = "World"
    # "$name!" should produce "World!" not look for variable `name!`
    @test "$name!" == "World!"

    # Multiple interpolations with trailing punctuation
    greeting = "Hello"
    @test "$greeting, $name!" == "Hello, World!"

    # Bang functions still work as regular identifiers
    arr = [3, 1, 2]
    sort!(arr)
    @test arr == [1, 2, 3]

    # Interpolation of bang-function results works via $(expr)
    @test "sorted: $(sort!([3,1,2]))" == "sorted: [1, 2, 3]"
end

true
