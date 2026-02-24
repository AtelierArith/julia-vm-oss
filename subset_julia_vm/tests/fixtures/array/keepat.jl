# Test keepat! function
# keepat!(a, inds) keeps only elements at specified indices

using Test

@testset "keepat!: keep only elements at specified indices" begin

    # Test with index array
    a1 = [6, 5, 4, 3, 2, 1]
    keepat!(a1, [1, 3, 5])
    check1 = length(a1) == 3
    check2 = a1[1] == 6 && a1[2] == 4 && a1[3] == 2

    # Test keeping consecutive indices
    a2 = [10, 20, 30, 40, 50]
    keepat!(a2, [2, 3, 4])
    check3 = length(a2) == 3
    check4 = a2[1] == 20 && a2[2] == 30 && a2[3] == 40

    # Test keeping all elements
    a3 = [1, 2, 3]
    keepat!(a3, [1, 2, 3])
    check5 = length(a3) == 3
    check6 = a3[1] == 1 && a3[2] == 2 && a3[3] == 3

    # Test keeping first element only
    a4 = [100, 200, 300]
    keepat!(a4, [1])
    check7 = length(a4) == 1
    check8 = a4[1] == 100

    # Test keeping last element only
    a5 = [100, 200, 300]
    keepat!(a5, [3])
    check9 = length(a5) == 1
    check10 = a5[1] == 300

    # Test with boolean mask
    a6 = [1, 2, 3, 4, 5]
    keepat!(a6, [true, false, true, false, true])
    check11 = length(a6) == 3
    check12 = a6[1] == 1 && a6[2] == 3 && a6[3] == 5

    # Test returns the array itself
    a7 = [1, 2, 3, 4]
    result = keepat!(a7, [2, 4])
    check13 = result === a7

    # Test with Float64 array
    a8 = [1.0, 2.0, 3.0, 4.0]
    keepat!(a8, [1, 3])
    check14 = length(a8) == 2
    check15 = a8[1] == 1.0 && a8[2] == 3.0

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8 && check9 && check10 && check11 && check12 && check13 && check14 && check15)
end

true  # Test passed
