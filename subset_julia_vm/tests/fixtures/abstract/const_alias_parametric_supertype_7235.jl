using Test

# Issue #7235 sub-case 1: a struct inheriting via a `const` alias for a
# parametric abstract type must still satisfy the subtype relation for `isa`
# and multiple dispatch. The alias must be resolved to its underlying
# parametric type when the struct's declared supertype is recorded; otherwise
# the recorded parent stays the bare alias name and the hierarchy chain walk
# cannot follow `Norm -> Dist`.
abstract type VariateForm end
abstract type Univariate <: VariateForm end
abstract type ValueSupport end
abstract type Continuous <: ValueSupport end
abstract type Dist7235{F<:VariateForm,S<:ValueSupport} end
const ContUni7235 = Dist7235{Univariate, Continuous}
struct Norm7235{T<:Real} <: ContUni7235
    m::T
end
distfn7235(d::Dist7235) = "dist"

@testset "const alias parametric supertype (Issue #7235 sub1)" begin
    n = Norm7235(0.0)
    # isa through the alias and through the underlying bare abstract.
    @test n isa Dist7235
    @test n isa ContUni7235
    # bare-name subtype relation through the alias.
    @test Norm7235 <: Dist7235
    # multiple dispatch on the bare abstract reaches the struct declared via alias.
    @test distfn7235(n) == "dist"
end

true
