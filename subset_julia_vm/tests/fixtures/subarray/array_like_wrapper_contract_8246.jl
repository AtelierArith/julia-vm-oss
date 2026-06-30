# Issue #8246: array-like wrapper constructors must keep compile-time inference
# and runtime equality normalization aligned.

using Test

@testset "array-like wrapper equality/inference contract (#8246)" begin
    # #8240 MWE shape: both operands are views. If `view(Vector, UnitRange)`
    # widens to Any at compile time, `==` can fall back to identity instead of
    # AbstractArray element comparison.
    w = view([1, 2, 3, 4], 1:3)
    w2 = view([0, 1, 2, 3], 2:4)
    native = [1, 2, 3]
    @test w == w2
    @test isequal(w, w2)
    @test w == native
    @test native == w
    @test isequal(w, native)
    @test isequal(native, w)
    @test !(w == view([1, 2, 4, 4], 1:3))

    # Non-SubArray wrapper case: reshape must also compare through the logical
    # Array/AbstractArray view against itself and a native matrix.
    r = reshape([1, 2, 3, 4], 2, 2)
    r2 = reshape([1, 2, 3, 4], 2, 2)
    native_matrix = [1 3; 2 4]
    @test r == r2
    @test isequal(r, r2)
    @test r == native_matrix
    @test native_matrix == r
    @test isequal(r, native_matrix)
    @test isequal(native_matrix, r)
    @test !(r == reshape([1, 2, 3, 5], 2, 2))
end

true
