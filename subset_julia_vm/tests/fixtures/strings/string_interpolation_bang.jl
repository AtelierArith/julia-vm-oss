# String interpolation identifier boundary around `!` (Issues #10322 / #10237)
# In upstream Julia, `!` IS part of the interpolated identifier: "$name!"
# interpolates the variable `name!`. The identifier stops before `!=`,
# which lexes as the operator. A literal bang after an interpolated
# variable requires the "$(name)!" form.
# (Issue #2130 previously asserted the opposite; corrected to upstream.)

using Test

@testset "String interpolation with bang" begin
    name = "World"
    name! = "Bang"

    # "$name!" interpolates `name!`, not `name` followed by literal "!"
    @test "$name!" == "Bang"

    # Literal "!" after an interpolated variable requires $(name)!
    @test "$(name)!" == "World!"

    # `!` stops the identifier when followed by `=` (operator boundary)
    @test "$name!=" == "World!="

    # Multiple interpolations
    greeting = "Hello"
    @test "$greeting, $(name)!" == "Hello, World!"
    @test "$greeting, $name!" == "Hello, Bang"

    # Bang functions still work as regular identifiers
    arr = [3, 1, 2]
    sort!(arr)
    @test arr == [1, 2, 3]

    # Interpolation of bang-function results works via $(expr)
    @test "sorted: $(sort!([3,1,2]))" == "sorted: [1, 2, 3]"
end

true
