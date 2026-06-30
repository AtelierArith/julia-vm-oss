# Issue #5789: `==` between two ComplexF64 arrays of equal value returned false
# when the operands used different internal carriers — a literal array stores its
# Complex elements as immutable `Value::Struct`, while a broadcast/copy result
# stores them as mutable `StructRef`. The element comparison only handled the
# (Struct, Struct) case and fell through to a Debug-string comparison for the
# mixed case, so every element compared unequal despite equal values.

using Test

@testset "ComplexF64 array == across carriers (Issue #5789)" begin
    a = ComplexF64[1+2im, 3+4im]

    # literal vs broadcast result (the headline case)
    @test (a == (a .+ 0)) == true
    @test (a == (a .+ 0im)) == true

    # literal vs copy, and zeros vs scaled
    @test (a == copy(a)) == true
    @test (ComplexF64[0im, 0im] == (a .* 0)) == true

    # collect normalizes the carrier; must still be equal
    @test (a == collect(a .+ 0)) == true

    # genuinely different values stay unequal
    @test (a == ComplexF64[1+2im, 9+9im]) == false
    @test (a == ComplexF64[1+2im]) == false           # length mismatch
end

@testset "non-complex array == regressions (Issue #5789)" begin
    @test (Float64[1.0, 2.0] == (Float64[1.0, 2.0] .+ 0.0)) == true
    @test ([1, 2] == ([1, 2] .+ 0)) == true
    @test ([1.0, 2.0] == [1.0, 3.0]) == false
end

true
