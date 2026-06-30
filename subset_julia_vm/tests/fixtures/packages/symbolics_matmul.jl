# Symbolics subset: symbolic-element matrix products `A*v` / `A*B` (Issue #7889).
#
# The VM's numeric `matmul` builtin assumes Float64/Complex elements and rejects
# `Symbolics.Num`. Upstream Julia's `*` is generic over the element type; the
# subset mirrors that with element-type-generic loops (in the bundled Symbolics
# package `linear_algebra.jl`) that accumulate symbolic values and store them in
# a `similar`-allocated result whose eltype is inferred from the first product.
#
# Value correctness is asserted by `substitute`-evaluating the symbolic result at
# concrete points (the canonical string form `x^2 - x*y` is the separate concern
# of Issue #7894); these assertions pass identically under upstream julia.

using Test
using Symbolics

@testset "Symbolics matrix * vector" begin
    @variables x y
    A = [x y; x x]
    v = [x, y]
    r = A * v
    @test r isa AbstractVector
    @test length(r) == 2
    @test r[1] isa Num
    d = Dict(x => 2, y => 3)
    # A*v = [x^2 + y^2, x^2 + x*y]  ->  [13, 10] at (x,y)=(2,3)
    @test substitute(r[1], d) == 13
    @test substitute(r[2], d) == 10
end

@testset "Symbolics matrix * matrix" begin
    @variables x y
    A = [x y; x x]
    B = A * A
    @test B isa AbstractMatrix
    @test size(B) == (2, 2)
    d = Dict(x => 2, y => 3)
    # [x^2+x*y  2x*y; 2x^2  x^2+x*y]  ->  [10 12; 8 10] at (x,y)=(2,3)
    @test substitute(B[1, 1], d) == 10
    @test substitute(B[1, 2], d) == 12
    @test substitute(B[2, 1], d) == 8
    @test substitute(B[2, 2], d) == 10
end

@testset "Symbolics matrix * numeric vector (mixed element types)" begin
    @variables x y
    A = [x y; x x]
    w = [1, 2]
    r = A * w
    @test r[1] isa Num
    d = Dict(x => 2, y => 3)
    # A*w = [x + 2y, 3x]  ->  [8, 6] at (x,y)=(2,3)
    @test substitute(r[1], d) == 8
    @test substitute(r[2], d) == 6
end

@testset "Symbolics 3x3 matrix * vector" begin
    @variables x y z
    A = [x y z; z x y; y z x]
    v = [x, y, z]
    r = A * v
    @test length(r) == 3
    d = Dict(x => 2, y => 3, z => 5)
    # r[1] = x*x + y*y + z*z = 4+9+25 = 38
    @test substitute(r[1], d) == 38
    # r[2] = z*x + x*y + y*z = 10+6+15 = 31
    @test substitute(r[2], d) == 31
    # r[3] = y*x + z*y + x*z = 6+15+10 = 31
    @test substitute(r[3], d) == 31
end

true
