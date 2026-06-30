# Issue #5615: a user parametric struct with a PARAMETRIC abstract parent
# (`struct MyVec{T} <: Wrapper{T}`) must subtype an EXISTENTIAL parametric right
# operand (`Wrapper{S} where S`) from a forall/bare left. The runtime reflection
# supertype of such a struct is its param-erased base (or `Any`), losing the
# invariant element binding, so the supertype-chain walk could not reach
# `Wrapper{T}`. It now consults the declared parametric parent TEMPLATE
# (`Wrapper{T} where T`) and re-enters the structured CoreType solver, while a
# concrete invariant parent (`Wrapper{Real}`) correctly stays false.

using Test

abstract type Wrapper5615{S} end
struct MyVec5615{T} <: Wrapper5615{T} end

@testset "parametric struct <: existential parametric parent (Issue #5615)" begin
    # forall-left / bare-left against an existential right → true
    @test (MyVec5615{T} where T) <: (Wrapper5615{S} where S)
    @test (MyVec5615{T} where T <: Real) <: (Wrapper5615{S} where S)
    @test MyVec5615 <: (Wrapper5615{S} where S)

    # concrete-left and bare-right already held; keep them green
    @test MyVec5615{Int} <: (Wrapper5615{S} where S)
    @test (MyVec5615{T} where T) <: Wrapper5615

    # element invariance is preserved: a concrete parent instantiation does NOT
    # match a forall-left, and a mismatched element stays false
    @test !((MyVec5615{T} where T) <: Wrapper5615{Real})
    @test MyVec5615{Int} <: Wrapper5615{Int}
    @test !(MyVec5615{Int} <: Wrapper5615{Real})
end

true
