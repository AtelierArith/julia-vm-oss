# Test slicing String arrays preserves element type
# Issue #1565: Array slicing only handles F64/I64, other types become 0.0

using Test

@testset "String array slicing" begin
    # Create a String array
    arr = ["apple", "banana", "cherry", "date", "elderberry"]

    # Test 1D slicing
    slice1 = arr[2:4]
    @test length(slice1) == 3
    @test slice1[1] == "banana"
    @test slice1[2] == "cherry"
    @test slice1[3] == "date"

    # Test that the element type is preserved (String, not F64)
    @test eltype(slice1) == String

    # Test slicing with :
    slice_all = arr[:]
    @test length(slice_all) == 5
    @test slice_all[1] == "apple"
    @test slice_all[5] == "elderberry"
    @test eltype(slice_all) == String
end

true
