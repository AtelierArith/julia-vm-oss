# Test for src/base/bool.jl
# Based on Julia's test/numbers.jl "basic booleans" testset
using Test

@testset "basic booleans" begin
    # xor tests - from Julia's test/numbers.jl
    @test xor(false, false) == false
    @test xor(true,  false) == true
    @test xor(false, true)  == true
    @test xor(true,  true)  == false

    # nand tests - from Julia's test/numbers.jl
    @test nand(false, false) == true
    @test nand(true, false) == true
    @test nand(false, true) == true
    @test nand(true, true) == false

    # nor tests - from Julia's test/numbers.jl
    @test nor(false, false) == true
    @test nor(true, false) == false
    @test nor(false, true) == false
    @test nor(true, true) == false
end

println("test_bool.jl: All tests passed!")
