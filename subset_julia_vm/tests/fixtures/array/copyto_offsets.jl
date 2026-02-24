# Test copyto! with offset arguments
# copyto!(dest, dstart, src) - copy all of src to dest starting at dstart
# copyto!(dest, dstart, src, sstart) - copy from src[sstart:end] to dest starting at dstart
# copyto!(dest, dstart, src, sstart, n) - copy n elements from src[sstart] to dest[dstart]

using Test

@testset "copyto! with offset arguments (dstart, sstart, n)" begin

    # =============================================================================
    # copyto!(dest, dstart, src) - 3 argument version
    # =============================================================================

    # Test copying to middle of dest array
    d1 = zeros(Int64, 10)
    s1 = [1, 2, 3]
    result1 = copyto!(d1, 4, s1)
    check1 = result1 === d1 && d1[4] == 1 && d1[5] == 2 && d1[6] == 3 && d1[3] == 0 && d1[7] == 0

    # Test copying to beginning
    d2 = zeros(Int64, 5)
    s2 = [10, 20, 30]
    copyto!(d2, 1, s2)
    check2 = d2[1] == 10 && d2[2] == 20 && d2[3] == 30 && d2[4] == 0 && d2[5] == 0

    # Test with floats
    d3 = zeros(Float64, 6)
    s3 = [1.5, 2.5]
    copyto!(d3, 3, s3)
    check3 = d3[3] == 1.5 && d3[4] == 2.5 && d3[1] == 0.0

    # =============================================================================
    # copyto!(dest, dstart, src, sstart) - 4 argument version
    # =============================================================================

    # Test copying partial source
    d4 = zeros(Int64, 5)
    s4 = [1, 2, 3, 4, 5]
    copyto!(d4, 1, s4, 3)  # Copy [3, 4, 5] to d4
    check4 = d4[1] == 3 && d4[2] == 4 && d4[3] == 5 && d4[4] == 0 && d4[5] == 0

    # Test with offset in both
    d5 = zeros(Int64, 10)
    s5 = [10, 20, 30, 40, 50]
    copyto!(d5, 3, s5, 2)  # Copy [20, 30, 40, 50] starting at d5[3]
    check5 = d5[3] == 20 && d5[4] == 30 && d5[5] == 40 && d5[6] == 50 && d5[2] == 0

    # =============================================================================
    # copyto!(dest, dstart, src, sstart, n) - 5 argument version
    # =============================================================================

    # Test copying specific number of elements
    d6 = zeros(Int64, 10)
    s6 = [1, 2, 3, 4, 5]
    copyto!(d6, 2, s6, 1, 3)  # Copy first 3 elements to d6[2:4]
    check6 = d6[2] == 1 && d6[3] == 2 && d6[4] == 3 && d6[5] == 0 && d6[1] == 0

    # Test copying from middle of source
    d7 = zeros(Int64, 5)
    s7 = [10, 20, 30, 40, 50]
    copyto!(d7, 1, s7, 2, 2)  # Copy [20, 30] to d7[1:2]
    check7 = d7[1] == 20 && d7[2] == 30 && d7[3] == 0

    # Test copying zero elements (should be no-op)
    d8 = [1, 2, 3]
    s8 = [10, 20, 30]
    copyto!(d8, 1, s8, 1, 0)
    check8 = d8[1] == 1 && d8[2] == 2 && d8[3] == 3

    # Test with floats
    d9 = zeros(Float64, 5)
    s9 = [1.1, 2.2, 3.3, 4.4, 5.5]
    copyto!(d9, 2, s9, 3, 2)  # Copy [3.3, 4.4] to d9[2:3]
    check9 = d9[2] == 3.3 && d9[3] == 4.4

    # =============================================================================
    # Final check
    # =============================================================================

    @test (check1 && check2 && check3 && check4 && check5 && check6 && check7 && check8 && check9)
end

true  # Test passed
