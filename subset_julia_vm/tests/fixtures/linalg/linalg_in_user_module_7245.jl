# Issue #7245: `using LinearAlgebra` inside a user-defined module named `D`
# must work. LinearAlgebra's Diagonal methods access a parameter named `D`
# via `D.diag` (field access). When the user module is literally named `D`,
# that field access used to be mis-resolved as the module-qualified call
# `D.diag(...)`, failing with "Module D has no function named diag".

using Test
using LinearAlgebra

# Module deliberately named `D` to collide with LinearAlgebra's `D::Diagonal`
# parameter name in its internal field accesses (`D.diag`).
module D
using LinearAlgebra
ddet(S) = det(S)
ddiag(S) = diag(S)
dinv(S) = inv(S)
export ddet, ddiag, dinv
end
using .D

@testset "LinearAlgebra usable from a user module named D (Issue #7245)" begin
    M = [2.0 0.0; 0.0 3.0]
    @test D.ddet(M) == 6.0
    @test D.ddiag(M) == [2.0, 3.0]
    # `inv` reaches its LinearAlgebra method from inside module D. Compare
    # element-wise: an unrelated, pre-existing bug makes the `inv` Array
    # wrapper's whole-matrix `==` against a plain literal Matrix return false
    # even when every element is equal (reproduces at top level, independent
    # of #7245), so we assert the values rather than `==` the matrices.
    Inv = D.dinv([2.0 0.0; 0.0 4.0])
    @test all(Inv .== [0.5 0.0; 0.0 0.25])
end

true  # Test passed
