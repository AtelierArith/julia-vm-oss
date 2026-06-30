# Observable parity for the shared const-specialization policy (Issue #4272).
#
# The compile-side inference cache key and the AoT specialization key now derive
# their preserve-vs-widen decision from one shared predicate
# (`const_specialization` / `is_const_eligible`). This fixture exercises the
# value classes the policy treats specially -- Bool, Symbol, Nothing, and small
# Int -- to confirm const-influenced inference still produces correct runtime
# results across the pipeline (and matches upstream Julia).

using Test

# --- Helpers (OUTSIDE @testset per project guidelines) ---

# Bool: const flag selects a branch with a different result type.
flagged(flag) = flag ? 1 : 1.0

# Symbol: const symbol acts as a field selector / dispatch tag.
select(nt, name) = getfield(nt, name)

# Nothing: const `nothing` participates in a `=== nothing` test.
or_default(x) = x === nothing ? 0 : x

# Small Int: const small integer drives a branch.
classify(n) = n == 0 ? :zero : (n == 1 ? :one : :many)

@testset "const-specialization parity (#4272)" begin
    # Bool const branch selection preserves per-branch result types.
    @test flagged(true) === 1
    @test flagged(false) === 1.0

    # Symbol const field selection.
    nt = (a = 10, b = 20)
    @test select(nt, :a) === 10
    @test select(nt, :b) === 20

    # Nothing const singleton handling.
    @test or_default(nothing) === 0
    @test or_default(42) === 42

    # Small-int const dispatch.
    @test classify(0) === :zero
    @test classify(1) === :one
    @test classify(7) === :many
end

true
