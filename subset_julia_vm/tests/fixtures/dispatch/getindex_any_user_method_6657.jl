# Issue #6657: `getindex` (`xs[i]`) called through an `Any`-typed binding must
# reach a user-defined `getindex(::Vector{Int64}, ::Int)` override instead of the
# native-indexing fast path. Three layers cooperate:
#   1. the generic compiler routes an `Any`-receiver scalar index through
#      `CallTypedDispatchOrBuiltin(GetIndex, ..)` (runtime dispatch with a native
#      `IndexLoad` fallback);
#   2. the abstract-interpretation engine resolves the override's return type via
#      method-table dispatch (only when a *user* method wins), so the call site is
#      not mis-inferred as the array element type;
#   3. the runtime function specializer refuses its native-indexing fast path so
#      the generic dispatching body is used.
# Native arrays without an override keep the fast path (the common case is
# unaffected). Verified against upstream Julia 1.12.

using Test

import Base: getindex

getindex(xs::Vector{Int64}, i::Int) = :ov_int
getindex(m::Matrix{Float64}, i::Int, j::Int) = :ov_mat

# Receiver is Any-typed, so the override must be reached at runtime.
call_get(xs) = xs[1]
call_get2(m) = m[1, 1]
call_explicit(xs) = getindex(xs, 1)

@testset "getindex(::Any) dispatch to user array overrides (#6657)" begin
    # Index syntax through an Any binding reaches the override.
    @test call_get([10, 20, 30]) == :ov_int
    # Explicit getindex call through an Any binding too.
    @test call_explicit([10, 20, 30]) == :ov_int
    # Multi-dimensional override reached through an Any binding.
    @test call_get2([1.0 2.0; 3.0 4.0]) == :ov_mat

    # Non-overridden element types keep the native element-returning behavior.
    @test call_get([1.0, 2.0]) === 1.0
    @test call_get2([1 2; 3 4]) === 1

    # A concrete-typed receiver dispatches to the override as well.
    g(xs::Vector{Int64}) = xs[2]
    @test g([10, 20, 30]) == :ov_int

    # A hot loop over a non-overridden element type stays correct (fast path).
    function sumvec(v)
        s = 0.0
        for i in 1:length(v)
            s += v[i]
        end
        return s
    end
    @test sumvec([1.0, 2.0, 3.0, 4.0]) == 10.0
end

# Final value gates the in-harness nextest run on correctness, not just no-throw.
call_get([10, 20, 30]) == :ov_int &&
    call_explicit([10, 20, 30]) == :ov_int &&
    call_get2([1.0 2.0; 3.0 4.0]) == :ov_mat &&
    call_get([1.0, 2.0]) === 1.0 &&
    call_get2([1 2; 3 4]) === 1
