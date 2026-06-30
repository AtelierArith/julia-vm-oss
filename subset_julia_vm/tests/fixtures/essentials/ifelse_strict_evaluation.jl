# Regression fixture for Issue #3733.
#
# Before the fix, public `ifelse(c, x, y)` was lowered directly to
# `BuiltinOp::IfElse`, which short-circuits the non-selected arm. That
# violates Julia's strict-evaluation semantics: ifelse is an ordinary
# function call, so BOTH `x` and `y` must be evaluated before the
# selected one is returned.
#
# After the fix, `ifelse` flows through the Pure Julia method already
# present in `subset_julia_vm/src/julia/base/essentials.jl`, which
# receives both arguments evaluated.
#
# Side effects are tracked by `push!`-ing into a global Vector probe
# rather than by `println`, because Issue #3780 is a separate VM stack
# bug that breaks dispatch when one arg involves `println` (the
# canonical example in the #3733 issue body). Once #3780 lands, the
# literal example from the #3733 issue body becomes runnable too.

using Test

const PROBE = Int[]

function bump1!()
    push!(PROBE, 1)
    return 999
end

function bump2!()
    push!(PROBE, 2)
    return 42
end

@testset "ifelse evaluates both branches (Issue #3733)" begin
    @assert length(PROBE) == 0

    r = ifelse(true, 1, bump1!())
    @test r == 1
    @test length(PROBE) == 1

    r2 = ifelse(false, bump1!(), 2)
    @test r2 == 2
    @test length(PROBE) == 2

    r3 = ifelse(true, bump1!(), bump2!())
    @test r3 == 999
    @test length(PROBE) == 4
end

@testset "ifelse return value selection" begin
    @test ifelse(true, 10, 20) == 10
    @test ifelse(false, 10, 20) == 20
    @test ifelse(true, "yes", "no") == "yes"
    @test ifelse(false, 1.5, 2.5) == 2.5
end

true
