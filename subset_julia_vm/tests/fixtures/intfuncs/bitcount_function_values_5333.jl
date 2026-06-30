# Bit-count helpers as first-class function values (Issue #5333)
#
# count_ones / count_zeros / leading_zeros / trailing_zeros / leading_ones /
# trailing_ones / bitrotate work when called directly but previously raised
# UndefVarError when referenced as bare identifiers / function values. They must
# resolve as first-class function values (matching upstream Julia) so they can be
# passed to higher-order functions and reflection surfaces.

using Test

@testset "bit-count helpers as function values (Issue #5333)" begin
    # Bare identifier references must resolve to callable function values
    # (previously raised UndefVarError).
    cones = count_ones
    @test cones(11) == 3

    czeros = count_zeros
    @test czeros(Int8(1)) == 7

    g = leading_zeros
    @test g(1) == 63

    h = bitrotate
    @test h(1, 1) == 2

    # Passed to a higher-order function.
    @test map(count_ones, [7, 8]) == [3, 1]
    @test map(trailing_zeros, [8, 16]) == [3, 4]
    @test map(count_zeros, Int8[0, 1]) == [8, 7]
    @test map(leading_ones, [-1, 0]) == [64, 0]
    @test map(trailing_ones, [7, 6]) == [3, 0]

    # A tuple of bit-count function values can be built and iterated.
    fns = (count_ones, count_zeros, leading_zeros, trailing_zeros,
           leading_ones, trailing_ones, bitrotate)
    @test length(fns) == 7
end

# Return true to indicate success.
true
