# Test copy! function for in-place array copying
# copy!(dest, src) - copy elements from src to dest
# For vectors: resizes dest to match src length if needed

using Test

@testset "copy! - in-place array copying with resize for vectors" begin

    # =============================================================================
    # copy! for vectors (resizing)
    # =============================================================================

    # Test copying to same-sized vector
    v1_src = [1, 2, 3, 4, 5]
    v1_dest = zeros(Int64, 5)
    result1 = copy!(v1_dest, v1_src)
    check1 = result1 === v1_dest && v1_dest[1] == 1 && v1_dest[5] == 5

    # Test copying to smaller vector (should resize)
    v2_src = [10, 20, 30, 40]
    v2_dest = zeros(Int64, 2)
    result2 = copy!(v2_dest, v2_src)
    check2 = length(v2_dest) == 4 && v2_dest[1] == 10 && v2_dest[4] == 40

    # Test copying to larger vector (should resize)
    v3_src = [1, 2]
    v3_dest = zeros(Int64, 5)
    result3 = copy!(v3_dest, v3_src)
    check3 = length(v3_dest) == 2 && v3_dest[1] == 1 && v3_dest[2] == 2

    # Test copying with floats
    v4_src = [1.5, 2.5, 3.5]
    v4_dest = zeros(Float64, 3)
    result4 = copy!(v4_dest, v4_src)
    check4 = v4_dest[1] == 1.5 && v4_dest[2] == 2.5 && v4_dest[3] == 3.5

    # =============================================================================
    # Final check
    # =============================================================================

    @test (check1 && check2 && check3 && check4)
end

true  # Test passed
