using Test

# Issue #5662: `isequal(x)` (single argument) is the curried predicate form —
# `y -> isequal(y, x)` — matching upstream `isequal(x) = Base.Fix2(isequal, x)`.
# The comparison operators (`==(x)`, `<(x)`) already curried, but `isequal` (an
# ordinary function, not an operator) had no single-argument method, so
# `map(isequal(3), v)` / `filter(isequal(2), v)` failed. sjulia represents the
# curried form as a closure (like its curried operators, #5127), so it flows
# through every HOF. NOTE: upstream defines NO `isless(x)` curried form.

@testset "isequal(x) curried predicate works across HOFs (Issue #5662)" begin
    @test map(isequal(3), [1, 2, 3]) == Bool[0, 0, 1]
    @test filter(isequal(2), [1, 2, 2, 3]) == [2, 2]
    @test findfirst(isequal(3), [1, 2, 3]) == 3
    @test findall(isequal(2), [1, 2, 2, 3]) == [2, 3]
    @test count(isequal(2), [1, 2, 2, 3]) == 2

    # Direct application of the curried predicate.
    @test isequal(3)(3) == true
    @test isequal(3)(5) == false
    @test isequal("a")("a") == true
end

@testset "two-argument isequal is unchanged (Issue #5662)" begin
    @test isequal(3, 3) == true
    @test isequal(1, 2) == false
    @test isequal(NaN, NaN) == true
    @test isequal(missing, missing) == true
end

true
