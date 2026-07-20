/// Fail-closed guard (Issue #9199 review r3535721788; mirrors the exhaustive
/// #9323 `BuiltinOp` audit). `Instr` has no cheap runtime enumeration (many
/// boxed operands), so the eligibility gate's soundness — that no
/// function-index / closure-bearing opcode reaches the `_ => true` catch-all —
/// is pinned two ways:
///
/// 1. Every currently-known function-bearing variant NAME still exists in
///    `Instr::VARIANTS` (catches a rename that would silently drop it from the
///    checked/reject arms).
/// 2. The total `Instr` variant count is frozen. Adding ANY variant trips this
///    test, forcing the author to decide whether the new opcode carries a
///    function index / lifts a function (→ add it to the gate's checked/reject
///    arms AND to `FUNCTION_BEARING_INSTRS`) or is function-free (→ bump the
///    count). This is the compile-adjacent "you cannot skip the decision" guard
///    that a full wildcard-free match over ~440 cross-crate variants would give
///    but at a fraction of the maintenance cost.
#[test]
fn live_append_gate_function_bearing_classification_is_fail_closed_9199() {
    use strum::VariantNames;

    // (1) No function-bearing name has been renamed out from under the gate.
    for name in FUNCTION_BEARING_INSTRS {
        assert!(
            Instr::VARIANTS.contains(name),
            "`Instr::{name}` is in FUNCTION_BEARING_INSTRS but no longer exists in \
             Instr::VARIANTS — it was renamed/removed. Update the eligibility gate \
             `user_main_calls_only_existing_functions` and this list together."
        );
    }

    // (2) Frozen variant count: a new `Instr` variant must be classified.
    // Each variant below is FUNCTION-FREE — it carries no function-table
    // index and lifts/creates no function or closure — so all are correctly
    // handled by `user_main_calls_only_existing_functions`'s catch-all
    // (`_ => true`, safe to splice) and are NOT in FUNCTION_BEARING_INSTRS:
    //   432 — Issue #10354's `ThrowUndefVarError(String)`: a compile-time
    //         undefined-name diagnostic (mirrors `ThrowMethodError`; the
    //         `String` operand is just the name).
    //   431 — Issue #10191's `ApplyTypeDynamicSplat` (its mask only controls
    //         runtime argument flattening before type application).
    //         Issue #10491's two F64-specialize variants ARE function-bearing
    //         and are listed in FUNCTION_BEARING_INSTRS.
    //         Issue #10107's `TakeSlot` (a destructive slot load — it moves a
    //         value out of a local slot) and Issue #10105's
    //         `JumpIfCmpI64SlotConst` (a fused compare-and-branch).
    //   436 — Issues #9784/#11546's `DefineEvalStruct(usize)` carries a
    //         type-table index, not a function-table index. The concrete
    //         struct append gate validates that index independently.
    //   441 — Issues #11569/#9784 append the five function-free lexical
    //         environment instructions. Their operands are binding names,
    //         never function-table indices.
    //   443 — Issues #9784/#11635 append abstract/primitive publication
    //         markers. Their operands are nominal-registry indices, not
    //         function-table indices.

    // Issue #11320's `RaiseUndefVarErrorIfFunctionInvisible(String)` (444)
    // IS function-bearing (a by-name runtime lookup, same shape as
    // `PushFunction`/`CallGlobalRef`) and is listed in
    // FUNCTION_BEARING_INSTRS + the gate's conservative-reject arm, not here.

    //   445 — Issue #11654's `DefineRuntimeNominal` carries structured nominal
    //         metadata but no function-table index.
    //   446 — Issues #11025/#11654's `ProbeRuntimeBinding` carries only a binding
    //         name and is likewise function-free.
    //   447 — Issue #9784's `CreateResolvedClosure` carries compiler-resolved
    //         candidate indices; the append gate validates them via the
    //         explicit `CreateResolvedClosure` arm above, not this list.
    //   448 — Issue #11748's `ActivateUsing` carries an owner module path and a
    //         local import index, never a function-table identity.
    //   449 — Issue #11761's `ActivateModule` carries only an owner module path.
    const EXPECTED_INSTR_VARIANT_COUNT: usize = 449;
    assert_eq!(
        Instr::VARIANTS.len(),
        EXPECTED_INSTR_VARIANT_COUNT,
        "the `Instr` variant count changed ({} now vs {} pinned). A variant was \
         added or removed. If the new opcode carries a function-table index or \
         lifts/creates a function/closure, add it to a checked/reject arm of \
         `user_main_calls_only_existing_functions` AND to FUNCTION_BEARING_INSTRS \
         (Issue #9199 review r3535721788); otherwise it is function-free. Then set \
         EXPECTED_INSTR_VARIANT_COUNT to {}.",
        Instr::VARIANTS.len(),
        EXPECTED_INSTR_VARIANT_COUNT,
        Instr::VARIANTS.len(),
    );
}
