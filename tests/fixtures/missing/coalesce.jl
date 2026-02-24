# Test @coalesce macro and coalesce function
# Returns the first non-missing value from arguments

using Test

@testset "coalesce function tests" begin
    # coalesce with first value missing
    @test coalesce(missing, 42) == 42

    # coalesce with first value not missing
    @test coalesce(1, 42) == 1

    # coalesce with all values missing returns missing
    @test ismissing(coalesce(missing, missing))

    # coalesce with 3 arguments
    @test coalesce(missing, missing, 3) == 3

    # coalesce with 4 arguments
    @test coalesce(missing, missing, missing, 4) == 4

    # coalesce with nothing (nothing is not missing)
    @test coalesce(nothing, 42) === nothing
end

@testset "@coalesce macro tests" begin
    # Basic @coalesce usage
    @test @coalesce(missing, 42) == 42

    # First non-missing value returned
    @test @coalesce(1, 2) == 1

    # Multiple missing values
    @test @coalesce(missing, missing, 3) == 3

    # All missing returns missing
    @test ismissing(@coalesce(missing, missing))

    # nothing is not missing
    @test @coalesce(nothing, 42) === nothing
end

true
