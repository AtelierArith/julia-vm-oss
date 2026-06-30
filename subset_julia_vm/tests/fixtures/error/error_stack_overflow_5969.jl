using Test

# Issue #5969 (prevention for #5966): a runaway / infinite recursion must raise
# a *catchable* `StackOverflowError` rather than growing the VM call-frame stack
# without bound until the host runs out of memory (the 80GB OOM of #5966).
#
# Upstream Julia throws `StackOverflowError` (see julia/base/boot.jl) when a call
# recurses beyond the call stack; it is an ordinary catchable `Exception`.

# Infinite self-recursion with no base case. The trailing `+ 1` keeps the call
# non-tail (a real frame must outlive each recursive call), so the call-frame
# stack genuinely grows on every level.
infinite_recursion() = infinite_recursion() + 1

function recursion_is_caught()
    caught = false
    try
        infinite_recursion()
    catch e
        caught = true
    end
    return caught
end

function recursion_throws_stackoverflow()
    is_so = false
    try
        infinite_recursion()
    catch e
        is_so = e isa StackOverflowError
    end
    return is_so
end

# Bounded, finite recursion must still complete normally: the depth guard must
# not fire on legitimate recursion. NOTE: this VM does not tail-call-optimize,
# so `countdown` grows the call-frame stack one level per call. Recursion deeper
# than ~MAX_CALL_DEPTH (10_000) frames therefore raises StackOverflowError
# *earlier* than upstream Julia would — an intentional tradeoff for the
# memory-constrained no-JIT iOS target (see Vm::MAX_CALL_DEPTH). `countdown(5000)`
# pins that legitimate thousands-deep recursion still succeeds, so an accidental
# lowering of the limit that breaks real recursion is caught here.
countdown(n) = n <= 0 ? 0 : countdown(n - 1)

@testset "runaway recursion raises catchable StackOverflowError (Issue #5969)" begin
    @test recursion_is_caught()
    @test recursion_throws_stackoverflow()
    @test StackOverflowError <: Exception
end

@testset "bounded recursion is unaffected by the depth guard" begin
    @test countdown(500) == 0
    @test countdown(0) == 0
    # Pin the legitimate-recursion boundary well below the guard limit so a
    # future reduction of MAX_CALL_DEPTH that breaks real deep recursion fails.
    @test countdown(5000) == 0
end

true
