# Explicit shape verification for 2D slicing
# This test will fail if shape is wrong

using Test

@testset "2D slice explicit shape check" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Column slice
    col1 = A[:, 1]

    # Get size as tuple
    s = size(col1)

    # If shape is (2,), then:
    #   - length(s) == 1 (1-tuple)
    #   - s[1] == 2
    # If shape is (2, 1), then:
    #   - length(s) == 2 (2-tuple)
    #   - s[1] == 2, s[2] == 1

    # This test explicitly checks the tuple length
    tuple_len = length(s)
    @test tuple_len == 1  # Should be 1-tuple (2,) not 2-tuple (2, 1)

    # Also check ndims directly
    @test ndims(col1) == 1

    # Check the element values are correct
    @test col1[1] == 1.0
    @test col1[2] == 4.0
end

true
