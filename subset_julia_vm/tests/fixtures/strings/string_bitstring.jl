# Test bitstring function - binary representation as string

using Test

@testset "bitstring(x) - binary representation as string" begin

    # === Int64 ===
    # Positive integers
    bs5 = bitstring(5)
    @assert length(bs5) == 64
    @assert endswith(bs5, "0101")
    @assert startswith(bs5, "0000000000000000000000000000000000000000000000000000000000000")

    # Zero
    bs0 = bitstring(0)
    @assert length(bs0) == 64
    @assert bs0 == "0000000000000000000000000000000000000000000000000000000000000000"

    # Negative integers (two's complement)
    bsn1 = bitstring(-1)
    @assert length(bsn1) == 64
    @assert bsn1 == "1111111111111111111111111111111111111111111111111111111111111111"

    # === Float64 ===
    # Positive float
    bs1_5 = bitstring(1.5)
    @assert length(bs1_5) == 64
    # IEEE 754 representation of 1.5

    # Zero float
    bs0_0 = bitstring(0.0)
    @assert length(bs0_0) == 64
    @assert bs0_0 == "0000000000000000000000000000000000000000000000000000000000000000"

    # === Bool ===
    bstrue = bitstring(true)
    @assert bstrue == "1"

    bsfalse = bitstring(false)
    @assert bsfalse == "0"

    # All tests passed
    @test (true)
end

true  # Test passed
