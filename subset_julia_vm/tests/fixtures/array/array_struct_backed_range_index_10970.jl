# Struct-backed ranges remain valid array indices through untyped parameters (Issue #10970).

using Test

@testset "struct-backed range indices through untyped parameters (Issue #10970)" begin
    dynamic_getindex(a, k) = a[k]

    values = [10, 20, 30, 40]
    @test dynamic_getindex(values, 2:4) == [20, 30, 40]
    @test dynamic_getindex(values, 1:2:4) == [10, 30]
    @test dynamic_getindex(values, big(2):big(4)) == [20, 30, 40]
    @test dynamic_getindex(values, big(1):big(2):big(4)) == [10, 30]

    too_large = big(typemax(Int64)) + big(1)
    # This assertion only guards graceful failure; exact BoundsError parity is #11010.
    overflow_errored = false
    try
        dynamic_getindex(values, too_large:too_large)
    catch
        overflow_errored = true
    end
    @test overflow_errored

    # Negative controls: routing AbstractRange indices through the slice path
    # must not disturb the existing scalar and CartesianIndex paths.
    @test dynamic_getindex(values, 3) == 30
    matrix = [10 20; 30 40]
    @test dynamic_getindex(matrix, CartesianIndex(2, 1)) == 30
end

true
