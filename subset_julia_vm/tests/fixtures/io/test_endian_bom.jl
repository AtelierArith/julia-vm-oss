# Test ENDIAN_BOM constant

using Test

@testset "ENDIAN_BOM constant" begin
    # ENDIAN_BOM should be a 32-bit value indicating byte order
    # Little-endian: 0x04030201 (67305985)
    # Big-endian: 0x01020304 (16909060)
    # Note: In Julia it's UInt32, in SubsetJuliaVM it's Int64
    @test typeof(ENDIAN_BOM) <: Integer
    # On most modern systems (x86, ARM), it should be little-endian
    @test ENDIAN_BOM == 0x04030201 || ENDIAN_BOM == 0x01020304
end

true  # Test passed
