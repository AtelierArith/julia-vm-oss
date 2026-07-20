using Test

# Issue #10368: `Base.infer_effects(f, types)` reported `terminates = true` for
# a `println` call reachable only through `Expr::Ternary` (`cond ? a : b`) or
# short-circuit `&&`/`||`, but correctly reported `terminates = false`
# (matching upstream) for the SAME `println` at top level, inside an
# `if`/`elseif`/`else` block, or a loop.
#
# Root cause: the shared body-effect walker
# (`subset_julia_vm_types/src/runtime_types/function_effects.rs`,
# `compute_expr_effects`) resolves an unknown callee (one with no
# body-derived summary in `effects_map`, e.g. `println`, a Base Rust
# builtin excluded from the whole-program propagation slice) to the fully
# conservative `Effects::arbitrary()` when the call sits directly in
# statement position (a bare call, or inside an `if`/`elseif`/`else`
# branch) — but a call nested inside `Expr::Ternary` or short-circuit
# `Expr::BinaryOp{And,Or}` was routed through the `infer_expr_effects_with_callees`
# bridge instead, whose own missing-callee fallback consults the curated
# builtin name table (`infer_builtin_effects`), which classifies
# `println`/`print`/`show`/`display`/`write` as `Effects::with_side_effects()`
# — a summary that optimistically claims `terminates = true` (plus
# `noub`/`nonoverlayed`/`nortcall` = true) where upstream Julia proves none
# of those. This is distinct from Issues #10145/#10264 (a registration-
# eligibility gap for fully-typed-param methods that also covers ternary/
# short-circuit but only asserts the ALREADY-correct `nothrow`/`effect_free`
# bits — both of which were already false via `with_side_effects()` too, so
# it did not catch the `terminates`/`noub`/`nonoverlayed`/`nortcall`
# over-claim exercised here). Fix: `Expr::Ternary` and short-circuit
# `Expr::BinaryOp{And,Or}` now recurse through `compute_expr_effects`
# directly (mirroring `Stmt::If`), so a nested call resolves via the same
# effects_map lookup as a bare statement or if/else branch.

function ternary_effect_10368(x)
    x ? println("p") : 1
end

function and_effect_10368(x)
    x && println("p")
    return 1
end

function or_effect_10368(x)
    x || println("p")
    return 1
end

# Control: the SAME println, reached through if/elseif/else — already
# correct before this fix; used to assert exact structural parity with the
# ternary/short-circuit forms above (the fix's soundness bar: match if/else
# bit-for-bit, not just `nothrow`/`effect_free`).
function if_effect_10368(x)
    if x
        println("p")
    end
    return 1
end

@testset "Base.infer_effects: terminates parity for ternary/short-circuit vs if/else (Issue #10368)" begin
    ef_ternary = Base.infer_effects(ternary_effect_10368, Tuple{Bool})
    ef_and = Base.infer_effects(and_effect_10368, Tuple{Bool})
    ef_or = Base.infer_effects(or_effect_10368, Tuple{Bool})
    ef_if = Base.infer_effects(if_effect_10368, Tuple{Bool})

    # `terminates` is the bit the bug directly over-claimed: upstream proves
    # `false` for all four forms (a call to an unanalyzed `println` is never
    # provably terminating), and sjulia's if/else path already agreed.
    @test ef_ternary.terminates === false
    @test ef_and.terminates === false
    @test ef_or.terminates === false
    @test ef_if.terminates === false

    # Exact structural parity: ternary/short-circuit must match the if/else
    # control bit-for-bit (every one of the 9 Effects fields, via `string`),
    # not merely agree on individual weak properties. This also transitively
    # covers the other bits the bug over-claimed alongside `terminates`
    # (`with_side_effects()` vs `arbitrary()`): `noub`, `nonoverlayed`,
    # `nortcall`.
    @test string(ef_ternary) == string(ef_if)
    @test string(ef_and) == string(ef_if)
    @test string(ef_or) == string(ef_if)

    # `nothrow`/`effect_free` were already correct before this fix (covered
    # by Issue #10145/#10264); keep them locked here too so a future
    # regression in either direction is caught by this same fixture.
    @test ef_ternary.nothrow === false
    @test ef_and.nothrow === false
    @test ef_or.nothrow === false
    @test ef_ternary.effect_free != 0x00
    @test ef_and.effect_free != 0x00
    @test ef_or.effect_free != 0x00
end

true
