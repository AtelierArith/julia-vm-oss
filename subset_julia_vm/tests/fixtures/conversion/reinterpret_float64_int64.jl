# Test reinterpret Float64 <-> Int64
# Use simple values to test bit-level reinterpretation
# reinterpret(Int64, 1.0) and reinterpret(Float64, result) should round-trip

using Test

@testset "reinterpret Float64 <-> Int64 bit conversion" begin
    x = 1.0
    bits = reinterpret(Int64, x)
    back = reinterpret(Float64, bits)
    @test (back == x)
end

true  # Test passed
