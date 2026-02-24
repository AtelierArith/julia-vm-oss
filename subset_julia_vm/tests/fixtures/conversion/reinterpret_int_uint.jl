# Test reinterpret signed <-> unsigned
# -1 reinterpreted as UInt64 gives max UInt64 value (all bits set)

using Test

@testset "reinterpret signed <-> unsigned integers" begin
    a = reinterpret(UInt64, Int64(-1))
    # Back to Int64 should give -1
    b = reinterpret(Int64, a)
    # Compare b to -1 (both Int64)
    @test (b == -1)
end

true  # Test passed
