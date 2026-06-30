# Symbolics subset: symbolic-matrix `det` / `inv` / `\` (Issue #7892, Epic #7888).
#
# `det`/`inv`/`\` over `AbstractMatrix{<:Symbolics.Num}` are implemented in the
# bundled Symbolics package `linear_algebra.jl` with Laplace (cofactor)
# expansion, so they never touch the VM's numeric `det`/`inv`/matmul builtins
# (which assume Float64/Complex elements). `\` is solved via `inv(A) * b` (the
# symbolic matmul from Issue #7889).
#
# Value correctness is asserted by `substitute`-evaluating the symbolic result at
# concrete points (the canonical string form is the separate concern of #7894);
# these assertions pass identically under upstream julia.

using Test
using Symbolics
using LinearAlgebra

# `substitute` returns a `Num` wrapping the numeric result; `value` peels the
# wrapper so the comparison works on both upstream julia and sjulia.
approx0(v) = isapprox(Float64(Symbolics.value(v)), 0.0; atol = 1e-9)
approx(v, w) = isapprox(Float64(Symbolics.value(v)), Float64(w); atol = 1e-9)

@testset "Symbolics det via cofactor expansion" begin
    @variables x y
    # det of a singular symbolic matrix is structurally zero after simplify.
    @test isequal(det([x y; x y]), 0)
    A = [x y; x x]
    # det([x y; x x]) = x^2 - x*y  ->  -2 at (x,y)=(2,3)
    @test substitute(det(A), Dict(x => 2, y => 3)) == -2
    @variables x y z
    B = [x y z; z x y; y z x]
    # 3x3 determinant -> 70 at (2,3,5)
    @test substitute(det(B), Dict(x => 2, y => 3, z => 5)) == 70
end

@testset "Symbolics inv via adjugate / det" begin
    @variables x y
    A = [x y; x x]
    Ai = inv(A)
    @test Ai isa AbstractMatrix
    @test size(Ai) == (2, 2)
    # inv(A) * A == I  (checked at a concrete point)
    P = Ai * A
    pt = Dict(x => 2, y => 3)
    @test approx(substitute(P[1, 1], pt), 1)
    @test approx0(substitute(P[1, 2], pt))
    @test approx0(substitute(P[2, 1], pt))
    @test approx(substitute(P[2, 2], pt), 1)
    # The redefinition must not perturb purely numeric inv (still the builtin).
    @test inv([1.0 2.0; 3.0 4.0]) ≈ [-2.0 1.0; 1.5 -0.5]
end

@testset "Symbolics linear solve A \\ b" begin
    @variables x y
    A = [x y; x x]
    b = [x, y]
    sol = A \ b
    @test length(sol) == 2
    # A * (A \ b) == b  (checked at a concrete point)
    chk = A * sol
    pt = Dict(x => 2, y => 3)
    @test approx(substitute(chk[1], pt), 2)
    @test approx(substitute(chk[2], pt), 3)
end

true
