# Lambda expressions in keyword arguments (Issue #2073)
# Tests that arrow functions can be used as keyword argument values
# when calling functions.

using Test

@testset "Lambda in keyword arguments" begin
    # sort with by=lambda (comma syntax)
    @test sort([3,1,4,1,5], by=x->-x) == [5,4,3,1,1]

    # sort with by=lambda (semicolon syntax)
    @test sort([3,1,4,1,5]; by=x->-x) == [5,4,3,1,1]

    # sort strings by length
    @test sort(["bb", "a", "ccc"], by=x->length(x)) == ["a", "bb", "ccc"]

    # sort with rev=true and by=lambda combined
    @test sort([3,1,4,1,5], by=x->x*x, rev=true) == [5,4,3,1,1]
end

true
