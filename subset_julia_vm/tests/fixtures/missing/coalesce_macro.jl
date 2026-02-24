# Test @coalesce macro
# Similar to @something but returns first non-missing value

using Test

@testset "@coalesce macro" begin
    # Basic case: first non-missing value
    @test @coalesce(missing, 42) == 42

    # Multiple missing values
    @test @coalesce(missing, missing, 1) == 1

    # First argument is not missing
    @test @coalesce(1, 2, 3) == 1

    # Mixed missing and nothing
    @test @coalesce(missing, nothing) === nothing

    # All non-missing returns first
    @test @coalesce(10, 20, 30) == 10
end

true
