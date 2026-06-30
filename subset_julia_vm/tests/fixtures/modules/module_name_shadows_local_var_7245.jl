# Issue #7245 (root cause, LinearAlgebra-free): a local binding (function
# parameter or local variable) named after the enclosing module must shadow
# the module name, matching Julia's scoping rules. Previously a field access
# `D.field` on a parameter `D` defined inside a module literally named `D`
# was mis-resolved as the module-qualified call `D.field(...)` and failed
# with "Module D has no function named field".

using Test

module D
struct Foo
    diag::Int
    val::Float64
end
# `D` here is a parameter, not the module — `D.diag` is a field access.
getdiag(D::Foo) = D.diag
getval(D::Foo) = D.val
export getdiag, getval, Foo
end
using .D

@testset "local var shadows same-named module in field access (Issue #7245)" begin
    f = D.Foo(42, 3.5)
    @test D.getdiag(f) == 42
    @test D.getval(f) == 3.5
end

true  # Test passed
