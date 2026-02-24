# Test filter! function (in-place filter)
# filter!(f, arr) removes elements where f returns false

using Test

@testset "filter! - in-place filter operation (Issue #351)" begin

    # Basic filter!
    arr1 = [1.0, 2.0, 3.0, 4.0, 5.0]
    filter!(x -> x > 2, arr1)
    check1 = length(arr1) == 3 && arr1[1] == 3.0 && arr1[2] == 4.0 && arr1[3] == 5.0

    # Filter to keep even numbers
    arr2 = [1.0, 2.0, 3.0, 4.0, 6.0, 8.0]
    filter!(x -> mod(x, 2) == 0, arr2)
    check2 = length(arr2) == 4 && arr2[1] == 2.0 && arr2[2] == 4.0

    # All elements match
    arr3 = [2.0, 4.0, 6.0]
    filter!(x -> x > 0, arr3)
    check3 = length(arr3) == 3

    # No elements match
    arr4 = [1.0, 2.0, 3.0]
    filter!(x -> x > 10, arr4)
    check4 = length(arr4) == 0

    @test (check1 && check2 && check3 && check4)
end

true  # Test passed
