# Test reinterpret Float32 <-> Int32
# Round-trip test: convert to Int32 and back

using Test

@testset "reinterpret Float32 <-> Int32 bit conversion" begin
    x = Float32(1.0)
    bits = reinterpret(Int32, x)
    back = reinterpret(Float32, bits)
    # Compare via Float64 to avoid Float32 == Float32 issue
    @test (Float64(back) == Float64(x))
end

true  # Test passed
