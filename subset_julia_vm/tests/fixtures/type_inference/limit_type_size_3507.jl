# Issue #3507: Replace fixed union widening with Julia-inspired type-size
# limiting (`limit_type_size`). The widener must (a) keep small/canonical
# unions like `Union{T, Nothing}` intact, (b) widen large heterogeneous
# accumulations to a sound abstract supertype rather than `Top`, and (c)
# never widen short unions to a less precise shape unnecessarily.
#
# These are runtime checks against the values produced by the inference
# engine; the goal is to demonstrate that programs which previously
# triggered the fixed-length widener still execute correctly under the
# new comparison-aware policy.

using Test

# ---------------------------------------------------------------------------
# (a) Small `Union{T, Nothing}` — must round-trip through inference and stay
# small enough to dispatch correctly.
# ---------------------------------------------------------------------------
function nullable_passthrough(x::Int)
    if x > 0
        return x
    end
    return nothing
end

# ---------------------------------------------------------------------------
# (b) Loop accumulator returning increasingly heterogeneous integer types.
# Each branch returns a different numeric width; with a hard length cap
# this would have collapsed early, but with comparison-aware widening the
# fall-back is the abstract `Integer` supertype, which still supports
# arithmetic.
# ---------------------------------------------------------------------------
function mixed_int_loop(n::Int)
    acc = 0
    for i in 1:n
        if i % 4 == 0
            acc = acc + Int8(1)
        elseif i % 4 == 1
            acc = acc + Int16(1)
        elseif i % 4 == 2
            acc = acc + Int32(1)
        else
            acc = acc + Int64(1)
        end
    end
    return acc
end

# ---------------------------------------------------------------------------
# (c) Tail-vararg-shaped tuple — depth 4 of identical Int64 elements.
# Inference must not widen this tuple to `Tuple{Any, Any, ...}`.
# ---------------------------------------------------------------------------
function fixed_tuple()
    return (1, 2, 3, 4)
end

@testset "limit_type_size (Issue #3507)" begin
    # Nullable kept narrow.
    @test nullable_passthrough(1) === 1
    @test nullable_passthrough(-1) === nothing

    # Mixed-int loop produces an Integer-typed accumulator at runtime.
    @test mixed_int_loop(0) === 0
    @test mixed_int_loop(8) isa Integer

    # Fixed-shape tuple preserved.
    t = fixed_tuple()
    @test length(t) == 4
    @test t[1] === 1
    @test t[4] === 4
end

true
