# Test: Complex array with interleaved storage
# This tests the Phase 1 implementation of efficient complex arrays

using Test

@testset "Complex array with interleaved storage" begin

    # Create complex array and access elements
    z1 = Complex(1.0, 2.0)
    z2 = Complex(3.0, 4.0)
    z3 = Complex(5.0, 6.0)

    # Test array literal and access
    arr = [z1, z2, z3]
    result = 0.0

    # Test element access and real/imag
    result += real(arr[1])  # 1.0
    result += imag(arr[1])  # 2.0
    result += real(arr[2])  # 3.0
    result += imag(arr[2])  # 4.0
    result += real(arr[3])  # 5.0
    result += imag(arr[3])  # 6.0

    # Total: 1 + 2 + 3 + 4 + 5 + 6 = 21.0
    @test (result) == 21.0
end

true  # Test passed
