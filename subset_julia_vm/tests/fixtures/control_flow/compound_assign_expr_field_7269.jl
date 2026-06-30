# Issue #7269: compound assignment in expression (return / RHS) position
# Field-access LHS used as a return value.
using Test

mutable struct P; z::Float64; end
f(p) = (return p.z += 1.0)

@testset "compound assignment field as return value (Issue #7269)" begin
    p = P(1.0)
    r = f(p)
    @test r == 2.0       # expression value is the new value
    @test p.z == 2.0     # the field was actually updated
end

true
