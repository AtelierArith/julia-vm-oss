# Sum of array elements: 1+2+3+4+5 = 15

using Test

@testset "Sum of array elements" begin
    arr = [1, 2, 3, 4, 5]
    # Pure Julia sum preserves element type (Int64 for integer arrays)
    @test sum(arr) == 15
    @test sum(arr) isa Int64

    # Float64 arrays
    @test sum([1.0, 2.0, 3.0]) == 6.0
    @test sum([1.0, 2.0, 3.0]) isa Float64

    # Empty array
    @test sum(Int64[]) == 0
end

true  # Test passed
