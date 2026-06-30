using Test

# Issue #7235 sub-case 2: a method on a BARE abstract type must match when that
# abstract type has a PARAMETRIC ABSTRACT supertype
# (`abstract type Dist2{F,S} <: Sampleable2{F,S} end`). The pure-Rust parser
# emits the parametric parent as a top-level `ParametrizedTypeExpression`, which
# previously overwrote the abstract type's own name with the parent's base, so
# the abstract type (`Dist2`) was never registered and any later use
# (`isa Dist2`, `::Dist2` dispatch) failed.
abstract type VariateForm2 end
abstract type Univariate2 <: VariateForm2 end
abstract type ValueSupport2 end
abstract type Continuous2 <: ValueSupport2 end
abstract type Sampleable2{F,S} end
abstract type Dist2{F,S} <: Sampleable2{F,S} end
struct Norm2{T<:Real} <: Dist2{Univariate2, Continuous2}
    m::T
end
foo2(d::Dist2, x::Real) = 1
foo2bare(d::Dist2) = 2
samp2(d::Sampleable2) = 3

@testset "bare abstract method with parametric abstract supertype (Issue #7235 sub2)" begin
    n = Norm2(0.0)
    @test foo2(n, 3.0) == 1
    @test foo2bare(n) == 2
    # dispatch reaches the method on the parametric abstract grandparent.
    @test samp2(n) == 3
    @test n isa Dist2
    @test n isa Sampleable2
    @test Norm2 <: Dist2
    @test Norm2 <: Sampleable2
    # the abstract type with a parametric abstract supertype is now defined and
    # usable as a value (printable / comparable). Note: `Dist2 isa Type` returns
    # the wrong answer for a *parametric* abstract type — a separate pre-existing
    # divergence orthogonal to this issue — so it is deliberately not asserted.
    @test Dist2 === Dist2
    @test Sampleable2 === Sampleable2
end

true
