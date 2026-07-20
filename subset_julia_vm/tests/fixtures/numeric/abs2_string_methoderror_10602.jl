# Regression test for Issue #10602
#
# abs2's real-number fallback was defined as an untyped `function abs2(x)`
# in subset_julia_vm/src/julia/base/number.jl instead of upstream's
# `abs2(x::Real) = x*x` (julia/base/number.jl:189). The untyped signature
# matched any argument, including String, so `abs2("a")` silently dispatched
# to the `x * x` body and string-concatenated to "aa" instead of raising a
# MethodError like upstream Julia.
#
# julia vs sjulia (before fix):
#   abs2("a")      -> MethodError: no method matching abs2(::String)   | "aa"
#   typeof(e)      -> MethodError                                      | String (no exception)

using Test

@testset "abs2(::String) raises MethodError like upstream (Issue #10602)" begin
    e = try
        abs2("a")
        nothing
    catch err
        err
    end
    @test typeof(e) == MethodError

    # Numeric abs2 must remain unaffected by the ::Real annotation.
    @test abs2(3) == 9
    @test abs2(-4) == 16
    @test abs2(2.5) == 6.25
    @test abs2(-1.5) == 2.25
    @test abs2(true) == 1
    @test abs2(false) == 0

    # Complex abs2 (typed methods in complex.jl) must keep dispatching to
    # their own specific methods, not the new Real-typed real fallback
    # (Issue #10775: dispatch order there is seed-dependent).
    @test abs2(3 + 4im) == 25
    @test abs2(3.0 + 4.0im) == 25.0
    @test abs2(Complex{Float32}(3.0f0, 4.0f0)) == 25.0f0
end
println("done")
true
