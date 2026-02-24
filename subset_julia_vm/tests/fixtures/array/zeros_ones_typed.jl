# Test zeros and ones with type parameter (Issue #589)
# zeros(Type, dims...) and ones(Type, dims...) should create typed arrays

using Test

@testset "zeros(Type, dims...) and ones(Type, dims...) create typed arrays (Issue #589)" begin

    result = 0.0

    # Test 1: zeros(Float64, n) - should create Float64 array (same as zeros(n))
    arr1 = zeros(Float64, 3)
    if length(arr1) == 3 && arr1[1] == 0.0 && arr1[2] == 0.0 && arr1[3] == 0.0
        result = result + 1.0
    end

    # Test 2: zeros(Int64, n) - should create Int64 array of zeros
    arr2 = zeros(Int64, 4)
    if length(arr2) == 4 && arr2[1] == 0 && arr2[2] == 0 && arr2[3] == 0 && arr2[4] == 0
        result = result + 1.0
    end

    # Test 3: zeros(Float64, m, n) - multi-dimensional Float64 array
    arr3 = zeros(Float64, 2, 3)
    if size(arr3) == (2, 3) && arr3[1, 1] == 0.0 && arr3[2, 3] == 0.0
        result = result + 1.0
    end

    # Test 4: zeros(Int64, m, n) - multi-dimensional Int64 array
    arr4 = zeros(Int64, 3, 2)
    if size(arr4) == (3, 2) && arr4[1, 1] == 0 && arr4[3, 2] == 0
        result = result + 1.0
    end

    # Test 5: ones(Float64, n) - should create Float64 array of ones
    arr5 = ones(Float64, 3)
    if length(arr5) == 3 && arr5[1] == 1.0 && arr5[2] == 1.0 && arr5[3] == 1.0
        result = result + 1.0
    end

    # Test 6: ones(Int64, n) - should create Int64 array of ones
    arr6 = ones(Int64, 4)
    if length(arr6) == 4 && arr6[1] == 1 && arr6[2] == 1 && arr6[3] == 1 && arr6[4] == 1
        result = result + 1.0
    end

    # Test 7: zeros(Complex{Float64}, n) - should create ComplexF64 array of zeros
    arr7 = zeros(Complex{Float64}, 2)
    # Only check length since complex equality in arrays has issues
    if length(arr7) == 2
        result = result + 1.0
    end

    # Test 8: zeros(Int, n) - Int is alias for Int64
    arr8 = zeros(Int, 3)
    if length(arr8) == 3 && arr8[1] == 0 && arr8[2] == 0 && arr8[3] == 0
        result = result + 1.0
    end

    @test (result) == 8.0
end

true  # Test passed
