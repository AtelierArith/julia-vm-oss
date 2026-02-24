# Test resize! function
# resize!(a, n) resizes collection a to contain n elements

using Test

@testset "resize!: change array size with preservation or initialization" begin

    # Test shrinking array
    a1 = [1, 2, 3, 4, 5]
    resize!(a1, 3)
    check1 = length(a1) == 3
    check2 = a1[1] == 1 && a1[2] == 2 && a1[3] == 3

    # Test growing array
    a2 = [1.0, 2.0]
    resize!(a2, 5)
    check3 = length(a2) == 5
    check4 = a2[1] == 1.0 && a2[2] == 2.0
    check5 = a2[3] == 0.0 && a2[4] == 0.0 && a2[5] == 0.0

    # Test no change (same size)
    a3 = [10, 20, 30]
    resize!(a3, 3)
    check6 = length(a3) == 3
    check7 = a3[1] == 10 && a3[2] == 20 && a3[3] == 30

    # Test resize to empty
    a4 = [1, 2, 3]
    resize!(a4, 0)
    check8 = length(a4) == 0

    # Test returns the array itself
    a5 = [1, 2, 3]
    result = resize!(a5, 5)
    check9 = result === a5

    # Test with Bool array
    a6 = [true, true]
    resize!(a6, 4)
    check10 = length(a6) == 4
    check11 = a6[1] == true && a6[2] == true && a6[3] == false && a6[4] == false

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8 && check9 && check10 && check11)
end

true  # Test passed
