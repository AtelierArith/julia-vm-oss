# Test sorting algorithm types (InsertionSort, QuickSort, MergeSort, PartialQuickSort)
# Note: In Julia, Algorithm is Base.Sort.Algorithm (not exported from Base)
# SubsetJuliaVM keeps Algorithm in Base, but it is not exported

using Test

@testset "Sorting algorithm types" begin
    # Test concrete algorithm types exist
    @test isa(InsertionSort, typeof(InsertionSort))
    @test isa(QuickSort, typeof(QuickSort))
    @test isa(MergeSort, typeof(MergeSort))

    # Test PartialQuickSort with parameter
    pqs = PartialQuickSort(5)
    @test pqs.k == 5

    # Verify types are distinct singletons
    @test typeof(InsertionSort) !== typeof(QuickSort)
    @test typeof(QuickSort) !== typeof(MergeSort)
end

true  # Test passed
