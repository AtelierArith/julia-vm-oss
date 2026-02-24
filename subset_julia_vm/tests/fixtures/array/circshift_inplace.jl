# Test circshift! function (in-place circular shift)
# circshift!(a, shift) rotates elements of a by shift positions in place

using Test

@testset "circshift!: circular shift array in place" begin

    # Test shift right by 2
    a1 = [1, 2, 3, 4, 5]
    circshift!(a1, 2)
    check1 = a1[1] == 4 && a1[2] == 5 && a1[3] == 1 && a1[4] == 2 && a1[5] == 3

    # Test shift left by 2 (negative shift)
    a2 = [1, 2, 3, 4, 5]
    circshift!(a2, -2)
    check2 = a2[1] == 3 && a2[2] == 4 && a2[3] == 5 && a2[4] == 1 && a2[5] == 2

    # Test shift by 0 (no change)
    a3 = [1, 2, 3, 4, 5]
    circshift!(a3, 0)
    check3 = a3[1] == 1 && a3[2] == 2 && a3[3] == 3 && a3[4] == 4 && a3[5] == 5

    # Test shift by array length (no change)
    a4 = [1, 2, 3, 4, 5]
    circshift!(a4, 5)
    check4 = a4[1] == 1 && a4[2] == 2 && a4[3] == 3 && a4[4] == 4 && a4[5] == 5

    # Test with Float64 array
    a5 = [1.0, 2.0, 3.0, 4.0]
    circshift!(a5, 1)
    check5 = a5[1] == 4.0 && a5[2] == 1.0 && a5[3] == 2.0 && a5[4] == 3.0

    # Test returns the array itself
    a6 = [1, 2, 3]
    result = circshift!(a6, 1)
    check6 = result === a6

    # Test empty array
    a7 = Int64[]
    circshift!(a7, 2)
    check7 = length(a7) == 0

    # Test single element array
    a8 = [42]
    circshift!(a8, 3)
    check8 = a8[1] == 42

    # All checks must pass
    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8)
end

true  # Test passed
