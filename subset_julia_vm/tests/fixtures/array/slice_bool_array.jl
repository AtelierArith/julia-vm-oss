# Test slicing Bool arrays preserves element type
# Issue #1565: Array slicing only handles F64/I64, other types become 0.0

using Test

@testset "Bool array slicing" begin
    # Create a Bool array
    arr = [true, false, true, false, true]

    # Test 1D slicing
    slice1 = arr[2:4]
    @test length(slice1) == 3
    @test slice1[1] == false
    @test slice1[2] == true
    @test slice1[3] == false

    # Test that the element type is preserved (Bool, not F64)
    @test eltype(slice1) == Bool

    # Test slicing with :
    slice_all = arr[:]
    @test length(slice_all) == 5
    @test slice_all[1] == true
    @test slice_all[5] == true
    @test eltype(slice_all) == Bool
end

true
