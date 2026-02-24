# count(predicate, string) - count characters satisfying predicate (Issue #2078)
# In Julia, count(f, itr) works for any iterable including strings.

using Test

@testset "count(predicate, string) (Issue #2078)" begin
    # Character classification predicates
    @test count(isletter, "h3ll0") == 3
    @test count(isdigit, "abc123") == 3
    @test count(isspace, "hello world") == 1
    @test count(isuppercase, "Hello World") == 2

    # Lambda predicates
    @test count(x -> x == 'l', "hello") == 2
    @test count(c -> c == 'a', "banana") == 3

    # Edge cases
    @test count(isletter, "") == 0
    @test count(isletter, "123") == 0
    @test count(isdigit, "abc") == 0

    # Regression: count(f, array) still works
    @test count(x -> x > 3, [1, 2, 3, 4, 5]) == 2
    @test count(isodd, [1, 2, 3, 4, 5]) == 3

    # Regression: count(pattern, string) still works
    @test count("ab", "ababab") == 3
end

true
