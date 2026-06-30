# Issue #5358: `ComplexF32` array `setindex!` (a[i] = z) previously failed with
# "Invalid Complex struct for IndexStore" because `as_complex_parts` /
# `complex_part_value_to_f64` did not handle F32 struct fields, so the
# interleaved-storage write path could not extract (re, im) from a
# Complex{Float32} value. (ComplexF64 already worked.)
#
# Assertions use real/imag/== rather than display, since the `f0` suffix and
# the `ComplexF32` vs `Complex{Float32}` element-type alias in array show are a
# separate, pre-existing display gap (PR #5261).

using Test

@testset "ComplexF32 array setindex! (Issue #5358)" begin
    b = ComplexF32[1.0f0 + 2.0f0im]
    b[1] = 7.0f0 + 8.0f0im
    @test real(b[1]) == 7.0f0
    @test imag(b[1]) == 8.0f0
    @test b[1] == 7.0f0 + 8.0f0im

    c = ComplexF32[1.0f0 + 1.0f0im, 2.0f0 + 2.0f0im]
    c[2] = 5.0f0 + 6.0f0im
    @test c[2] == 5.0f0 + 6.0f0im
    @test c[1] == 1.0f0 + 1.0f0im          # other element untouched
    @test c[1] + c[2] == 6.0f0 + 7.0f0im   # arithmetic over stored ComplexF32

    # zeros(ComplexF32, ...) + setindex! round-trips through the same path.
    z = zeros(ComplexF32, 3)
    z[2] = 3.0f0 - 4.0f0im
    @test z[1] == 0.0f0 + 0.0f0im
    @test real(z[2]) == 3.0f0
    @test imag(z[2]) == -4.0f0
end

true  # Test passed
