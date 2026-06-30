# Issue #7269: nested field-access compound assignment as a return value.
using Test

mutable struct Inner; v::Int; end
mutable struct Outer; inner::Inner; end
g(o) = (return o.inner.v += 7)

@testset "compound assignment nested field as return value (Issue #7269)" begin
    o = Outer(Inner(3))
    r = g(o)
    @test r == 10
    @test o.inner.v == 10
end

true
