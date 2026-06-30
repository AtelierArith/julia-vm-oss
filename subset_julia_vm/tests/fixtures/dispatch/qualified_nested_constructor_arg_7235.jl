using Test

# Issue #7235 sub-case 3 (cross-module qualified part): a module-qualified
# constructor call used DIRECTLY as the argument of another module-qualified
# call (`M.onearg(M.Norm(0.0))`) must infer the inner call's return type as the
# constructed struct, so the outer qualified dispatch can match a method on the
# struct's (possibly abstract) parameter. Previously the `ModuleCall` inference
# arm did not recognize a qualified constructor and fell through to `Any`, so
# the nested call's argument imaged as `Any` and the outer qualified dispatch
# failed at compile time. Binding the constructor result to a local first
# already worked; this closes the inline-argument gap.
module M7235
abstract type VariateForm end
abstract type Univariate <: VariateForm end
abstract type ValueSupport end
abstract type Continuous <: ValueSupport end
abstract type Dist{F,S} end
struct Norm{T<:Real} <: Dist{Univariate, Continuous}
    m::T
end
onearg(d::Dist) = 42
twoarg(d::Dist, x::Real) = 7
end

@testset "cross-module qualified nested dispatch (Issue #7235 sub3)" begin
    # qualified-call argument inside a qualified call (the gap).
    @test M7235.onearg(M7235.Norm(0.0)) == 42
    @test M7235.twoarg(M7235.Norm(0.0), 3.0) == 7
    # binding first already worked; keep as a regression anchor.
    n = M7235.Norm(0.0)
    @test M7235.onearg(n) == 42
end

true
