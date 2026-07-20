using Test

# Issues #10145 / #10264: Base.infer_effects must detect a side-effecting
# statement reachable through control flow (if/elseif/else, ternary,
# short-circuit &&/||, loops) inside a user method whose parameters are
# fully typed (e.g. `x::Bool`) — not just untyped params (already covered by
# `infer_effects_body_side_effect_8441.jl`). The root cause was not the
# control-flow walker (which already joins every branch/loop/short-circuit
# arm correctly) but a reflection-time registration gap: a method whose every
# parameter has a concrete, non-generic type annotation was never retained
# for body-derived effect composition, so `Base.infer_effects` silently fell
# back to the fully-total `(+c,+e,+n,+t,+s,+m,+u,+o,+r)` representative
# regardless of the body's actual effects.
#
# Assertions here intentionally check only the WEAK properties
# (`nothrow === false`, `effect_free != 0x00`) that upstream Julia and
# sjulia's own already-correct untyped-param path agree on — not exact
# bit-for-bit parity with upstream. sjulia's `consistent`/`effect_free` bits
# land on the tri-state `Conditional` (`?`) where upstream proves
# `AlwaysFalse` (`!`); that is a separate, pre-existing, intentionally
# untouched limitation of `EffectBit`'s symmetric merge
# (`subset_julia_vm_types/src/runtime_types/effects.rs`), not part of this
# fix.

function if_only_10145(x::Bool)
    if x
        println("p")
    end
    return 1
end

function if_else_no_return_10145(x::Bool)
    if x
        1
    else
        println("p")
        2
    end
end

function if_else_explicit_return_10145(x::Bool)
    if x
        return 1
    else
        println("p")
        return 2
    end
end

function elseif_chain_10145(x::Int64)
    if x == 1
        return 1
    elseif x == 2
        println("p")
        return 2
    else
        return 3
    end
end

function ternary_10145(x::Bool)
    x ? println("p") : 1
end

function and_short_circuit_10145(x::Bool)
    x && println("p")
    return 1
end

function or_short_circuit_10145(x::Bool)
    x || println("p")
    return 1
end

function while_loop_10145(x::Int64)
    while x > 0
        println("p")
        x -= 1
    end
    return 1
end

function for_loop_10145(x::Int64)
    for i in 1:x
        println("p")
    end
    return 1
end

function for_loop_nested_if_error_10145(x::Int64)
    for i in 1:x
        if i == 3
            error("boom")
        end
    end
    return 1
end

function for_loop_rand_10145(x::Int64)
    total = 0
    for i in 1:x
        total += rand(Int64)
    end
    return total
end

# Control: fully-typed params, plain if/else, NO side effect anywhere — must
# still be proven fully total (i.e. the fix must not over-taint pure typed
# methods just because they contain control flow).
function pure_typed_if_else_10145(x::Bool)
    if x
        return 1
    else
        return 2
    end
end

@testset "infer_effects detects control-flow-nested side effects for typed-param methods (Issues #10145, #10264)" begin
    # `nothrow === false` is the universal signal shared by every effectful
    # case below (upstream and sjulia's own already-correct untyped-param
    # path agree on it regardless of which builtin produces the effect):
    # before this fix, ALL of these incorrectly reported `nothrow === true`
    # (part of the fully-total `(+c,+e,+n,+t,+s,+m,+u,+o,+r)` representative)
    # because the method was never registered for body-derived composition.
    for (f, ty) in (
        (if_only_10145, Tuple{Bool}),
        (if_else_no_return_10145, Tuple{Bool}),
        (if_else_explicit_return_10145, Tuple{Bool}),
        (elseif_chain_10145, Tuple{Int64}),
        (ternary_10145, Tuple{Bool}),
        (and_short_circuit_10145, Tuple{Bool}),
        (or_short_circuit_10145, Tuple{Bool}),
        (while_loop_10145, Tuple{Int64}),
        (for_loop_10145, Tuple{Int64}),
        (for_loop_nested_if_error_10145, Tuple{Int64}),
        (for_loop_rand_10145, Tuple{Int64}),
    )
        ef = Base.infer_effects(f, ty)
        @test ef.nothrow === false
    end

    # `println`/`rand`-based cases additionally prove non-`effect_free`
    # (visible I/O / RNG state mutation); `error`-only bodies do not (upstream
    # classifies a bare `error()` as effect-free but throwing — effect-free
    # tracks I/O/mutation, not throwing).
    for (f, ty) in (
        (if_only_10145, Tuple{Bool}),
        (if_else_no_return_10145, Tuple{Bool}),
        (if_else_explicit_return_10145, Tuple{Bool}),
        (elseif_chain_10145, Tuple{Int64}),
        (ternary_10145, Tuple{Bool}),
        (and_short_circuit_10145, Tuple{Bool}),
        (or_short_circuit_10145, Tuple{Bool}),
        (while_loop_10145, Tuple{Int64}),
        (for_loop_10145, Tuple{Int64}),
        (for_loop_rand_10145, Tuple{Int64}),
    )
        ef = Base.infer_effects(f, ty)
        @test ef.effect_free != 0x00
    end

    # Control: a genuinely pure typed-param if/else must remain fully total —
    # the registration fix must not indiscriminately taint every typed-param
    # control-flow method.
    ef_pure = Base.infer_effects(pure_typed_if_else_10145, Tuple{Bool})
    @test ef_pure.nothrow === true
    @test ef_pure.effect_free == 0x00
    @test string(ef_pure) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end

true
