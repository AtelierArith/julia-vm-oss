# Test NAND and NOR functions

using Test

@testset "NAND and NOR operators (⊼ and ⊽)" begin

    # NAND truth table: !(a && b)
    @assert nand(true, true) == false
    @assert nand(true, false) == true
    @assert nand(false, true) == true
    @assert nand(false, false) == true

    # NOR truth table: !(a || b)
    @assert nor(true, true) == false
    @assert nor(true, false) == false
    @assert nor(false, true) == false
    @assert nor(false, false) == true

    @test (true)
end

true  # Test passed
