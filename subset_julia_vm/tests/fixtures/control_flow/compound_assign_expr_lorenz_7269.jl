# Issue #7269: the real-world Lorenz-attractor `step!` driver:
# `return l.z += l.dt * dz` — field += of a product, as a return value.
using Test

mutable struct L
    z::Float64
    dt::Float64
end
step!(l, dz) = (return l.z += l.dt * dz)

@testset "compound assignment field-of-product as return value (Issue #7269)" begin
    l = L(1.0, 0.5)
    r = step!(l, 4.0)   # l.z = 1.0 + 0.5*4.0 = 3.0
    @test r == 3.0
    @test l.z == 3.0
end

true
