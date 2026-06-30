using Test

# Issue #8079: a module that defines its OWN `log2`/`log10` (a fresh generic that
# shadows the Base library function of the same name) and forwards to the
# explicitly module-qualified `Base.log2` / `Base.log10` must reach the BASE
# implementation — not re-dispatch the qualified call back to its own shadow.
# A shadow whose body calls `Base.<name>` self-recurses (NaNMath.log2 →
# Base.log2 → NaNMath.log2 → …) into a spurious `StackOverflowError`. This is
# exactly the NaNMath pattern used by the Optim BFGS line search.
module NaNMathish8079
export log2, log10
log2(x) = x < zero(x) ? oftype(float(x), NaN) : Base.log2(float(x))
log10(x) = x < zero(x) ? oftype(float(x), NaN) : Base.log10(float(x))

# A nested closure (the BFGS line-search context) that recurses, then forwards
# to `Base.log2` through the module's own `log2`. The unqualified `log2` here is
# the module's own definition (no ambiguity), but its body's `Base.log2(...)`
# is the qualified call that previously re-dispatched to the shadow.
function iterfinitemax_deep(n)
    step = m -> m <= 0 ? ceil(Int, -log2(eps(Float64))) : iterfinitemax_deep(m - 1)
    step(n)
end
end

# A separate module that calls the shadow QUALIFIED, deep inside a nested
# closure crossing the module boundary (the Optim → LineSearches structure).
module Caller8079
import ..NaNMathish8079

function deep(n)
    step = m -> m <= 0 ? ceil(Int, -NaNMathish8079.log2(eps(Float64))) : deep(m - 1)
    step(n)
end
end

@testset "qualified Base call from a shadowing module (Issue #8079)" begin
    # The shadow forwards to Base.log2 / Base.log10 instead of recursing.
    @test NaNMathish8079.log2(0.5) == -1.0
    @test NaNMathish8079.log2(8.0) == 3.0
    @test NaNMathish8079.log10(1000.0) ≈ 3.0
    # Negative-domain branch returns NaN (the reason NaNMath exists).
    @test isnan(NaNMathish8079.log2(-1.0))

    # The exact expression from the issue, evaluated through the shadow.
    @test ceil(Int, -NaNMathish8079.log2(eps(Float64))) == 52

    # Same expression nested deep inside a closure: forwarded through the
    # module's own `log2` (no spurious StackOverflowError at any depth).
    @test NaNMathish8079.iterfinitemax_deep(0) == 52
    @test NaNMathish8079.iterfinitemax_deep(160) == 52

    # And reached qualified across a module boundary, deep inside a closure.
    @test Caller8079.deep(0) == 52
    @test Caller8079.deep(160) == 52
end

true
