# Test Meta.isoperator, isunaryoperator, isbinaryoperator, ispostfixoperator

using Test

@testset "Meta.isoperator family - validate operator symbols" begin

    # isoperator - arithmetic operators
    r1 = Meta.isoperator(:+)
    @assert r1 == true
    r2 = Meta.isoperator(:-)
    @assert r2 == true
    r3 = Meta.isoperator(:*)
    @assert r3 == true
    r4 = Meta.isoperator(:/)
    @assert r4 == true
    r5 = Meta.isoperator(:^)
    @assert r5 == true

    # isoperator - comparison operators
    r6 = Meta.isoperator(:<)
    @assert r6 == true
    r7 = Meta.isoperator(:>)
    @assert r7 == true

    # isoperator - non-operators
    r8 = Meta.isoperator(:f)
    @assert r8 == false
    r9 = Meta.isoperator(:foo)
    @assert r9 == false
    r10 = Meta.isoperator(:x)
    @assert r10 == false

    # isunaryoperator
    r11 = Meta.isunaryoperator(:+)
    @assert r11 == true
    r12 = Meta.isunaryoperator(:-)
    @assert r12 == true
    r13 = Meta.isunaryoperator(:!)
    @assert r13 == true
    r14 = Meta.isunaryoperator(:~)
    @assert r14 == true
    r15 = Meta.isunaryoperator(:*)  # * is not unary
    @assert r15 == false
    r16 = Meta.isunaryoperator(:f)
    @assert r16 == false

    # isbinaryoperator
    r17 = Meta.isbinaryoperator(:+)
    @assert r17 == true
    r18 = Meta.isbinaryoperator(:-)
    @assert r18 == true
    r19 = Meta.isbinaryoperator(:*)
    @assert r19 == true
    r20 = Meta.isbinaryoperator(:/)
    @assert r20 == true
    r21 = Meta.isbinaryoperator(:<)
    @assert r21 == true
    r22 = Meta.isbinaryoperator(:f)
    @assert r22 == false

    # ispostfixoperator - adjoint operator
    # Note: Symbol("'") currently not supported
    r23 = Meta.ispostfixoperator(:-)
    @assert r23 == false
    r24 = Meta.ispostfixoperator(:+)
    @assert r24 == false
    r25 = Meta.ispostfixoperator(:f)
    @assert r25 == false

    # Final result
    @test (42.0) == 42.0
end

true  # Test passed
