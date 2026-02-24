# Test comparison operators with Missing values
# In Julia, comparison operators (==, <, >, <=, >=) return missing when
# either operand is the literal `missing` (three-valued logic).
# This is different from isequal/isless which return Bool.
#
# Note: This test only covers compile-time literal `missing` comparisons.
# Runtime Missing value comparisons (via variables) require VM-level changes.

using Test

@testset "Missing comparison operators (literal missing)" begin
    # == returns missing
    @test ismissing(missing == missing)
    @test ismissing(missing == 1)
    @test ismissing(1 == missing)

    # != returns missing
    @test ismissing(missing != missing)
    @test ismissing(missing != 1)
    @test ismissing(1 != missing)

    # < returns missing
    @test ismissing(missing < missing)
    @test ismissing(missing < 1)
    @test ismissing(1 < missing)

    # > returns missing
    @test ismissing(missing > missing)
    @test ismissing(missing > 1)
    @test ismissing(1 > missing)

    # <= returns missing
    @test ismissing(missing <= missing)
    @test ismissing(missing <= 1)
    @test ismissing(1 <= missing)

    # >= returns missing
    @test ismissing(missing >= missing)
    @test ismissing(missing >= 1)
    @test ismissing(1 >= missing)

    # === returns Bool (identity comparison)
    @test (missing === missing) == true
    @test (missing === 1) == false
    @test (1 === missing) == false

    # !== returns Bool (identity comparison)
    @test (missing !== missing) == false
    @test (missing !== 1) == true
    @test (1 !== missing) == true
end

# Note: ispositive(missing), isnegative(missing), and isapprox(missing, x) tests
# are disabled because they require VM-level support for runtime Missing dispatch.
# The Pure Julia methods for Missing are defined but method dispatch with typed
# parameters (x::Missing vs x) needs additional work (Issue #719 related).
# These tests should be re-enabled once VM-level support is added.

true  # Test passed
