# N-dimensional array literals with array-valued blocks (Issue #10381)
#
# `;;`/`;;;`/... separators with non-scalar blocks route through the
# pure-Julia `hvncat` (ragged shape form). Previously these mis-shaped
# through the 2-D `hvcat` path (e.g. `[A B; C D;;; A B; C D]` produced a
# (4, 4) matrix instead of (2, 4, 2)).

using Test

@testset "matrix blocks across ;;; slices" begin
    A = [1 2]; B = [3 4]; C = [5 6]; D = [7 8]
    r = [A B; C D;;; A B; C D]
    @test size(r) == (2, 4, 2)
    @test r[1, 1, 1] == 1
    @test r[1, 3, 1] == 3
    @test r[2, 1, 1] == 5
    @test r[2, 3, 2] == 7
    @test r[:, :, 1] == [1 2 3 4; 5 6 7 8]
    @test r[:, :, 2] == [1 2 3 4; 5 6 7 8]
end

@testset "whole-matrix slices" begin
    v = [1 2; 3 4]
    w = [5 6; 7 8]
    r = [v;;; w]
    @test size(r) == (2, 2, 2)
    @test r[:, :, 1] == v
    @test r[:, :, 2] == w
end

@testset "trailing separator pads rank" begin
    A = [1 2]; B = [3 4]
    r = [A B;;;]
    @test size(r) == (1, 4, 1)
    @test r[1, :, 1] == [1, 2, 3, 4]
end

@testset "ragged rows across slices" begin
    A = [1 2]; C = [3 4 5 6]
    r = [A A; C;;; A A; C]
    @test size(r) == (2, 4, 2)
    @test r[1, :, 1] == [1, 2, 1, 2]
    @test r[2, :, 2] == [3, 4, 5, 6]
end

@testset "vector blocks in pure-semicolon form" begin
    a = [1, 2]; b = [3, 4]
    r = [a;; b]
    @test size(r) == (2, 2)
    @test r == [1 3; 2 4]
    r2 = [a; b;; b; a]
    @test size(r2) == (4, 2)
    @test r2[:, 1] == [1, 2, 3, 4]
    @test r2[:, 2] == [3, 4, 1, 2]
end

@testset "direct Base.hvncat calls" begin
    A = [1 2]; B = [3 4]
    r = hvncat(3, A, B)
    @test size(r) == (1, 2, 2)
    @test r[:, :, 2] == B
    r2 = hvncat((2, 2, 2), true, 1, 2, 3, 4, 5, 6, 7, 8)
    @test size(r2) == (2, 2, 2)
    @test r2 == [1 2; 3 4;;; 5 6; 7 8]
    r3 = hvncat(((2, 2), (4,)), false, 1, 2, 3, 4)
    @test size(r3) == (2, 2)
    @test r3 == [1 3; 2 4]
end

@testset "shape mismatch raises" begin
    A = [1 2]; C = [3 4 5]
    @test_throws DimensionMismatch [A A; C;;; A A; C]
end

true
