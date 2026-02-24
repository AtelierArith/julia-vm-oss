# Test: Short-form function definitions with varargs (Issue #1721)
# Ensures that f(args...) = expr works the same as function f(args...) ... end

using Test

# Basic varargs - short form
sum_short(args...) = isempty(args) ? 0 : sum(args)

# Basic varargs - full form
function sum_full(args...)
    isempty(args) ? 0 : sum(args)
end

# Mixed params with varargs - short form
first_plus_short(x, rest...) = x + (isempty(rest) ? 0 : sum(rest))

# Mixed params with varargs - full form
function first_plus_full(x, rest...)
    x + (isempty(rest) ? 0 : sum(rest))
end

# Count varargs - short form
count_short(args...) = length(args)

# Count varargs - full form
function count_full(args...)
    length(args)
end

@testset "Short-form varargs equivalence" begin
    # Test basic varargs
    @test sum_short(1, 2, 3) == 6
    @test sum_full(1, 2, 3) == 6
    @test sum_short(1, 2, 3) == sum_full(1, 2, 3)

    # Test with single argument
    @test sum_short(10) == 10
    @test sum_full(10) == 10

    # Test with empty varargs
    @test sum_short() == 0
    @test sum_full() == 0

    # Test mixed positional + varargs
    @test first_plus_short(10, 1, 2, 3) == 16
    @test first_plus_full(10, 1, 2, 3) == 16
    @test first_plus_short(10, 1, 2, 3) == first_plus_full(10, 1, 2, 3)

    # Test mixed with empty rest
    @test first_plus_short(5) == 5
    @test first_plus_full(5) == 5

    # Test count function
    @test count_short() == 0
    @test count_short(1) == 1
    @test count_short(1, 2, 3, 4, 5) == 5
    @test count_full() == 0
    @test count_full(1) == 1
    @test count_full(1, 2, 3, 4, 5) == 5
end

true
