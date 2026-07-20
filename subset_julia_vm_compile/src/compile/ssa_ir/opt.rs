//! SSA optimization passes (Issue #8551).
//!
//! Passes run on [`SsaFunction`]. Since the Issue #8552 lowering they are
//! reachable from the compilation pipeline behind the `SJULIA_SSA_PIPELINE=1`
//! gate (`super::lower` runs [`optimize`] before emission); with the gate off
//! the pipeline output is unchanged. In debug builds every pass re-runs
//! [`super::verify`] after mutating the function.
//!
//! * [`fold_constants`]: constant folding/propagation. Operations whose
//!   operands are [`SsaValue::Const`] are evaluated with the shared
//!   `compile::const_prop` evaluators (identical fold coverage to Core IR
//!   constant propagation; evaluation failure — overflow, division by zero,
//!   unsupported operand types — keeps the operation and its runtime error).
//!   Trivial phis (all incoming values present and equal, ignoring
//!   self-references through loop latches) fold to their single value, which
//!   subsumes the Core IR bridge's identical-branch-assignment fold on real
//!   [`PhiNode`]s. `Branch` terminators with a constant `Bool` condition
//!   rewrite to `Jump` (non-`Bool` constants keep the branch and its runtime
//!   TypeError).
//! * [`eliminate_unreachable_blocks`] / [`eliminate_dead_defs`]: DCE. Blocks
//!   left unreachable by branch folding are deleted (phi edges pruned, block
//!   ids renumbered); unused definitions are removed by liveness mark &
//!   sweep, which also collects dead def *cycles* such as the unpruned
//!   loop-header phis of SSA construction. Deletion requires
//!   [`Effects::is_removable`] (`:effect_free` + `:nothrow` + terminating),
//!   upstream's rule in `julia/Compiler/src/optimize.jl`.
//! * [`cse_pure_calls`]: dominator-scoped value numbering of pure calls; a
//!   duplicate merges only into an identical call that dominates it, and only
//!   when the callee summary satisfies [`Effects::is_foldable`] (so
//!   fresh-allocation callees are never merged — Issue #7176).
//! * [`optimize`] / [`optimize_with_effects`]: run the passes to a combined
//!   fixpoint.
//!
//! Purity of calls comes from the body-derived effect summaries of Issue
//! #8441: callers pass the `infer_program_effects` map into the `_with_effects`
//! entry points; names missing from the map fall back to the curated
//! `infer_builtin_effects` name table (default: `Effects::arbitrary()`).
//! Because summaries are keyed by name, a call through a parameter (HOF) or to
//! a name rebound by a nested definition must not trust the summary; those
//! callee names are collected by [`shadowed_callee_names`] and treated as
//! arbitrary. Locals bound to closures by plain assignment are not
//! reconstructible from the SSA (calls are by name in this slice) and are a
//! documented limitation shared with the Core IR effects machinery — see
//! `docs/vm/SSA_IR.md`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::compile::const_prop::{
    const_value_to_literal, eval_const_binary, eval_const_unary, literal_to_const_value,
};
use crate::compile::effects::inference::{
    infer_binary_op_effects, infer_builtin_effects, infer_unary_op_effects,
};
use crate::compile::effects::propagation::FuncId;
use crate::compile::effects::static_dispatch::StaticDispatchResolver;
use crate::compile::effects::{EffectBit, Effects};
use crate::compile::utils::{binary_op_to_function_name, unary_op_to_function_name};
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::Literal;

use super::dom::{compute_idoms, compute_reachable};
use super::model::{
    BlockId, PhiNode, SsaFunction, SsaOp, SsaParam, SsaValue, SsaValueId, Terminator,
};

/// Defensive bound on `fold -> DCE -> CSE` rounds in
/// [`optimize_with_effects`]. Every pass strictly shrinks the function
/// (statements, blocks, or branch terminators), so the bound is unreachable
/// for correct passes.
const MAX_ROUNDS: usize = 64;

/// Run all SSA optimization passes to a fixpoint with no interprocedural
/// effect information (equivalent to [`optimize_with_effects`] with an empty
/// summary map: only the curated builtin name table gates purity).
///
/// Called by the gated `super::lower` pipeline (Issue #8552); wiring the
/// body-derived `infer_program_effects` summaries into the gate site is part
/// of the default-flip criteria (`docs/vm/SSA_IR.md`).
pub fn optimize(func: &mut SsaFunction) {
    optimize_with_effects(func, &HashMap::new());
}

/// Run constant folding, DCE, and pure-call CSE to a fixpoint, using the
/// body-derived per-function effect summaries of Issue #8441 (as produced by
/// `compile::effects::propagation::infer_program_effects`) to gate deletion
/// and merging of calls.
///
/// Equivalent to [`optimize_scoped`] with an empty locally-bound name set;
/// the gated pipeline (Issue #8552) enters through [`optimize_scoped`] so
/// plain-assignment rebinds cannot be misattributed (Issue #8799).
pub fn optimize_with_effects(func: &mut SsaFunction, effects: &HashMap<FuncId, Effects>) {
    optimize_scoped(func, effects, &BTreeSet::new());
}

/// Run all passes to a fixpoint with effect summaries scoped to this
/// function: `locally_bound` names the source-level locals the body rebinds
/// by plain assignment (`f = sin; f(x)` — the set `scan::block_write_names`
/// computes, minus `global` declarations). Calls through such names must not
/// use the name-keyed summary of an unrelated global or builtin of the same
/// name (Issue #8799): DCE could otherwise delete the very call whose
/// presence makes the bytecode lowering fall back to the legacy path.
pub fn optimize_scoped(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    locally_bound: &BTreeSet<String>,
) {
    optimize_scoped_inner(func, effects, None, locally_bound);
}

/// [`optimize_scoped`] that additionally consults per-method effect summaries
/// at statically-resolved call sites (Issue #9495). At a call whose argument
/// types resolve to a single, unambiguous method of a fully-visible
/// multi-method generic, the DCE/CSE gates use that method's precise summary
/// instead of the conservative name-level merge; every other call keeps using
/// `effects` (`by_name`). Passing `resolver: None` is exactly
/// [`optimize_scoped`]. Wired from the SSA pipeline gate in
/// `ssa_ir::lower::try_lower`.
pub(in crate::compile) fn optimize_scoped_resolved(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    locally_bound: &BTreeSet<String>,
) {
    optimize_scoped_inner(func, effects, resolver, locally_bound);
}

fn optimize_scoped_inner(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    locally_bound: &BTreeSet<String>,
) {
    for _ in 0..MAX_ROUNDS {
        let mut changed = fold_constants_scoped_inner(func, effects, resolver, locally_bound);
        changed |= eliminate_unreachable_blocks(func);
        changed |= eliminate_dead_defs_scoped_inner(func, effects, resolver, locally_bound);
        changed |= cse_pure_calls_scoped_inner(func, effects, resolver, locally_bound);
        if !changed {
            return;
        }
    }
    debug_assert!(
        false,
        "SSA optimization did not reach a fixpoint within {MAX_ROUNDS} rounds (Issue #8551)"
    );
}

/// Effect information the SSA opt passes consult per call site: the sound
/// name-level merge (`by_name`), an optional static-dispatch `resolver` for
/// precise per-method summaries at statically-resolved call sites (Issue
/// #9495), and this function's parameters (to type `Argument` operands during
/// dispatch resolution).
struct EffectCtx<'a> {
    by_name: &'a HashMap<FuncId, Effects>,
    resolver: Option<&'a StaticDispatchResolver>,
    params: &'a [SsaParam],
}

/// Constant folding and propagation (Issue #8551).
///
/// Sweeps the function to a fixpoint (the degenerate worklist: every deleted
/// definition's uses are rewritten in the same round, so re-sweeping visits
/// exactly the statements a use-list worklist would): each round folds every
/// statement whose operands are constants, rewrites all uses of the folded
/// definitions, deletes the now-unreferenced statements, and rewrites
/// constant-`Bool` branches to jumps (pruning the dropped edge from the
/// target's phis). Returns whether anything changed.
pub fn fold_constants(func: &mut SsaFunction) -> bool {
    fold_constants_inner(func, None)
}

fn fold_constants_scoped_inner(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    locally_bound: &BTreeSet<String>,
) -> bool {
    fold_constants_inner(func, Some((effects, resolver, locally_bound)))
}

fn fold_constants_inner(
    func: &mut SsaFunction,
    effect_inputs: Option<(
        &HashMap<FuncId, Effects>,
        Option<&StaticDispatchResolver>,
        &BTreeSet<String>,
    )>,
) -> bool {
    let mut changed_any = false;
    loop {
        let mut subst: BTreeMap<SsaValueId, SsaValue> = BTreeMap::new();
        {
            let effect_state = effect_inputs.map(|(effects, resolver, locally_bound)| {
                let shadowed = shadowed_callee_names(func, locally_bound);
                let ctx = EffectCtx {
                    by_name: effects,
                    resolver,
                    params: &func.params,
                };
                (ctx, shadowed)
            });
            let effect_refs = effect_state.as_ref().map(|(ctx, shadowed)| (ctx, shadowed));
            for block in &func.blocks {
                for stmt in &block.stmts {
                    if let Some(replacement) = fold_statement(stmt.id, &stmt.op, effect_refs) {
                        subst.insert(stmt.id, replacement);
                    }
                }
            }
        }
        let subst = resolve_substitutions(&subst);
        let folded_defs = !subst.is_empty();
        if folded_defs {
            apply_substitutions(func, &subst);
            for block in &mut func.blocks {
                block.stmts.retain(|stmt| !subst.contains_key(&stmt.id));
            }
        }
        let folded_branches = fold_constant_branches(func);
        if !folded_defs && !folded_branches {
            break;
        }
        changed_any = true;
    }
    if changed_any {
        debug_assert_eq!(
            super::verify(func),
            Ok(()),
            "SSA verifier failed after constant folding (Issue #8551)"
        );
    }
    changed_any
}

/// The folded value of one statement, if its operation can be evaluated at
/// compile time. Deleting a folded statement is sound because the shared
/// evaluators only succeed on pure, non-throwing evaluations.
fn fold_statement(
    id: SsaValueId,
    op: &SsaOp,
    effect_ctx: Option<(&EffectCtx<'_>, &BTreeSet<String>)>,
) -> Option<SsaValue> {
    match op {
        SsaOp::Unary {
            op,
            operand: SsaValue::Const(operand),
        } => {
            let operand = literal_to_const_value(operand)?;
            let folded = eval_const_unary(unary_op_to_function_name(op), &operand)?;
            Some(SsaValue::Const(const_value_to_literal(folded)))
        }
        SsaOp::Binary {
            op,
            left: SsaValue::Const(left),
            right: SsaValue::Const(right),
        } => {
            let left = literal_to_const_value(left)?;
            let right = literal_to_const_value(right)?;
            let folded = eval_const_binary(binary_op_to_function_name(op), &left, &right)?;
            Some(SsaValue::Const(const_value_to_literal(folded)))
        }
        SsaOp::Call {
            module: None,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
        } if kwargs.is_empty()
            && splat_mask.iter().all(|flag| !*flag)
            && kwargs_splat_mask.iter().all(|flag| !*flag) =>
        {
            let (ctx, shadowed) = effect_ctx?;
            let effects = op_effects(op, ctx, shadowed);
            if !effects.is_foldable() || !effects.nothrow {
                return None;
            }
            let const_args: Option<Vec<_>> = args
                .iter()
                .map(|arg| match arg {
                    SsaValue::Const(lit) => literal_to_const_value(lit),
                    _ => None,
                })
                .collect();
            let folded = eval_const_call(function, &const_args?)?;
            Some(SsaValue::Const(const_value_to_literal(folded)))
        }
        SsaOp::Phi(phi) => trivial_phi_value(id, phi),
        _ => None,
    }
}

fn eval_const_call(
    function: &str,
    args: &[crate::compile::lattice::types::ConstValue],
) -> Option<crate::compile::lattice::types::ConstValue> {
    match args {
        [operand] => eval_const_unary(function, operand),
        [left, right] => eval_const_binary(function, left, right),
        _ => None,
    }
}

/// The single value of a trivial phi: all incoming values present and equal,
/// ignoring self-references through loop latches (Braun et al. 2013 trivial
/// phi rule; a constant phi is the special case where the value is a
/// constant). `None`-valued (maybe-undefined) edges block folding because
/// replacing the phi would erase a potential undefined-variable error path.
/// Returns `None` for phis whose operands are all self-references (dead
/// cycles, left for DCE).
fn trivial_phi_value(id: SsaValueId, phi: &PhiNode) -> Option<SsaValue> {
    let mut unique: Option<&SsaValue> = None;
    for value in &phi.values {
        let value = value.as_ref()?;
        if *value == SsaValue::Def(id) {
            continue;
        }
        match unique {
            None => unique = Some(value),
            Some(existing) if existing == value => {}
            Some(_) => return None,
        }
    }
    unique.cloned()
}

/// Resolve `Def -> Def` chains inside one round's substitution map so that
/// simultaneously-folded definitions land on their final value. Cycles of
/// mutually-trivial phis (possible only among dead loop phis) are dropped
/// from the map and left for DCE.
fn resolve_substitutions(subst: &BTreeMap<SsaValueId, SsaValue>) -> BTreeMap<SsaValueId, SsaValue> {
    let mut resolved = BTreeMap::new();
    'entries: for (&id, value) in subst {
        let mut seen = BTreeSet::from([id]);
        let mut value = value.clone();
        while let SsaValue::Def(next) = value {
            let Some(next_value) = subst.get(&next) else {
                break;
            };
            if !seen.insert(next) {
                continue 'entries;
            }
            value = next_value.clone();
        }
        resolved.insert(id, value);
    }
    resolved
}

/// Rewrite every use site (statement operands, phi incoming values, and
/// terminator operands) according to the substitution map.
fn apply_substitutions(func: &mut SsaFunction, subst: &BTreeMap<SsaValueId, SsaValue>) {
    let apply = |value: &mut SsaValue| {
        if let SsaValue::Def(id) = value {
            if let Some(replacement) = subst.get(id) {
                *value = replacement.clone();
            }
        }
    };
    for block in &mut func.blocks {
        for stmt in &mut block.stmts {
            for value in stmt.op.operands_mut() {
                apply(value);
            }
        }
        for value in block.terminator.operands_mut() {
            apply(value);
        }
    }
}

/// Rewrite `Branch` terminators with a constant `Bool` condition to `Jump`,
/// removing the not-taken edge from the dropped target's predecessor list and
/// phis. Constant non-`Bool` conditions keep the branch (and its runtime
/// TypeError), and non-constant conditions with equal targets keep the branch
/// so the condition's `Bool` check is preserved.
fn fold_constant_branches(func: &mut SsaFunction) -> bool {
    let mut changed = false;
    for index in 0..func.blocks.len() {
        let (taken, dropped) = match &func.blocks[index].terminator {
            Terminator::Branch {
                condition: SsaValue::Const(Literal::Bool(condition)),
                then_target,
                else_target,
            } => {
                if *condition {
                    (*then_target, *else_target)
                } else {
                    (*else_target, *then_target)
                }
            }
            _ => continue,
        };
        let from = func.blocks[index].id;
        func.blocks[index].terminator = Terminator::Jump { target: taken };
        func.blocks[index].succs = vec![taken];
        if dropped != taken {
            remove_edge(func, from, dropped);
        }
        changed = true;
    }
    changed
}

/// Remove the CFG edge `from -> to`: drop `from` from `to`'s predecessor list
/// and delete the corresponding incoming entry of every phi in `to`. (Phi
/// edges mirror the predecessor list position-for-position, an invariant of
/// [`super::verify`].)
fn remove_edge(func: &mut SsaFunction, from: BlockId, to: BlockId) {
    let block = &mut func.blocks[to.0 as usize];
    let Some(position) = block.preds.iter().position(|pred| *pred == from) else {
        return;
    };
    block.preds.remove(position);
    for stmt in &mut block.stmts {
        let SsaOp::Phi(phi) = &mut stmt.op else { break };
        phi.edges.remove(position);
        phi.values.remove(position);
    }
}

/// Delete blocks unreachable from the entry (DCE, Issue #8551), typically
/// left behind by constant branch folding. Edges from deleted predecessors
/// are pruned from the survivors' phis, and the surviving blocks are
/// renumbered densely (the remap is monotonic, preserving ascending
/// predecessor order). Uses can never dangle: a definition inside an
/// unreachable block cannot dominate a use in a reachable one.
pub fn eliminate_unreachable_blocks(func: &mut SsaFunction) -> bool {
    let reachable = compute_reachable(func);
    if reachable.len() == func.blocks.len() {
        return false;
    }
    for index in 0..func.blocks.len() {
        let id = func.blocks[index].id;
        if !reachable.contains(&id) {
            continue;
        }
        let dead_preds: Vec<BlockId> = func.blocks[index]
            .preds
            .iter()
            .copied()
            .filter(|pred| !reachable.contains(pred))
            .collect();
        for pred in dead_preds {
            remove_edge(func, pred, id);
        }
    }

    let remap: BTreeMap<BlockId, BlockId> = reachable
        .iter()
        .enumerate()
        .map(|(new_index, &id)| (id, BlockId(new_index as u32)))
        .collect();
    let renumber = |id: BlockId| remap[&id];
    func.blocks.retain(|block| reachable.contains(&block.id));
    for block in &mut func.blocks {
        block.id = renumber(block.id);
        for pred in &mut block.preds {
            *pred = renumber(*pred);
        }
        for succ in &mut block.succs {
            *succ = renumber(*succ);
        }
        match &mut block.terminator {
            Terminator::Jump { target } => *target = renumber(*target),
            Terminator::Branch {
                then_target,
                else_target,
                ..
            } => {
                *then_target = renumber(*then_target);
                *else_target = renumber(*else_target);
            }
            Terminator::Return { .. } => {}
        }
        for stmt in &mut block.stmts {
            let SsaOp::Phi(phi) = &mut stmt.op else { break };
            for edge in &mut phi.edges {
                *edge = renumber(*edge);
            }
        }
    }
    func.entry = renumber(func.entry);
    debug_assert_eq!(
        super::verify(func),
        Ok(()),
        "SSA verifier failed after unreachable block elimination (Issue #8551)"
    );
    true
}

/// Delete unused definitions (DCE, Issue #8551) by liveness mark & sweep:
/// roots are terminator operands and every statement whose operation is not
/// removable; liveness propagates through operands (phi incoming values
/// included). Sweeping non-live statements also removes *cycles* of dead
/// definitions, in particular the dead loop-header phis left by the unpruned
/// `while` pre-scan of SSA construction (Issue #8550).
///
/// A definition may be deleted only when its operation is `:effect_free`,
/// `:nothrow`, and terminating per the `compile::effects` machinery
/// ([`Effects::is_removable`]), following upstream's removability rule in
/// `julia/Compiler/src/optimize.jl`. Opaque payloads, builtins, and global
/// accesses are never removed in this slice.
pub fn eliminate_dead_defs(func: &mut SsaFunction, effects: &HashMap<FuncId, Effects>) -> bool {
    eliminate_dead_defs_scoped(func, effects, &BTreeSet::new())
}

/// [`eliminate_dead_defs`] with additional locally-bound callee names whose
/// summaries must be treated as arbitrary (Issue #8799; see
/// [`optimize_scoped`]).
pub fn eliminate_dead_defs_scoped(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    locally_bound: &BTreeSet<String>,
) -> bool {
    eliminate_dead_defs_scoped_inner(func, effects, None, locally_bound)
}

/// [`eliminate_dead_defs_scoped`] with the optional static-dispatch resolver
/// (Issue #9495): a call statically resolved to a pure method is removable when
/// dead even if its name-level merge (over impure siblings) is not.
fn eliminate_dead_defs_scoped_inner(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    locally_bound: &BTreeSet<String>,
) -> bool {
    let shadowed = shadowed_callee_names(func, locally_bound);
    let ctx = EffectCtx {
        by_name: effects,
        resolver,
        params: &func.params,
    };
    let mut deps: BTreeMap<SsaValueId, Vec<SsaValueId>> = BTreeMap::new();
    let mut live: BTreeSet<SsaValueId> = BTreeSet::new();
    let mut worklist: Vec<SsaValueId> = Vec::new();

    for block in &func.blocks {
        for stmt in &block.stmts {
            deps.insert(stmt.id, operand_defs(&stmt.op));
            if !op_effects(&stmt.op, &ctx, &shadowed).is_removable() && live.insert(stmt.id) {
                worklist.push(stmt.id);
            }
        }
        for value in block.terminator.operands() {
            if let SsaValue::Def(id) = value {
                if live.insert(*id) {
                    worklist.push(*id);
                }
            }
        }
    }
    while let Some(id) = worklist.pop() {
        let Some(operand_defs) = deps.get(&id) else {
            continue;
        };
        for &dep in operand_defs {
            if live.insert(dep) {
                worklist.push(dep);
            }
        }
    }

    let mut changed = false;
    for block in &mut func.blocks {
        let before = block.stmts.len();
        block.stmts.retain(|stmt| live.contains(&stmt.id));
        changed |= block.stmts.len() != before;
    }
    if changed {
        debug_assert_eq!(
            super::verify(func),
            Ok(()),
            "SSA verifier failed after dead definition elimination (Issue #8551)"
        );
    }
    changed
}

/// Definition ids read by an operation, phi incoming values included.
fn operand_defs(op: &SsaOp) -> Vec<SsaValueId> {
    let values: Vec<&SsaValue> = match op {
        SsaOp::Phi(phi) => phi.values.iter().flatten().collect(),
        other => other.operands(),
    };
    values
        .into_iter()
        .filter_map(|value| match value {
            SsaValue::Def(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// Callee names whose effect summaries must not be trusted inside this
/// function: parameters (a call through a parameter name dispatches on the
/// runtime argument — the HOF pattern), every local rebound by an opaque
/// barrier (surfacing as a [`SsaOp::BarrierReload`] var), which covers nested
/// `function` definitions shadowing a summarized name, and the caller-provided
/// `locally_bound` set. Locals bound to closures by **plain assignment**
/// (`f = sin; f(x)`) are not reconstructible from the SSA (calls are by
/// name), so the gate site passes the source-level write-name set here
/// (Issue #8799); callers without source access pass an empty set and keep
/// the previous behavior.
fn shadowed_callee_names(func: &SsaFunction, locally_bound: &BTreeSet<String>) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = func.params.iter().map(|param| param.name.clone()).collect();
    names.extend(locally_bound.iter().cloned());
    for block in &func.blocks {
        for stmt in &block.stmts {
            if let SsaOp::BarrierReload { var, .. } = &stmt.op {
                names.insert(var.clone());
            }
        }
    }
    names
}

/// Conservative effect summary of one SSA operation.
///
/// Operands are already-computed SSA values, so operand evaluation
/// contributes no effects; only the operation itself is classified.
/// Numeric/comparison operators use the `compile::effects` operator rules
/// (which, like the rest of that machinery, do not model MethodErrors on
/// exotic operand types). Calls look up the body-derived summary (Issue
/// #8441) by callee name — module qualifiers are ignored, matching
/// `compile::effects::propagation` — and fall back to the curated builtin
/// name table; splatted calls and shadowed callee names are arbitrary.
/// `Builtin`, global accesses, and opaque payloads are never assumed pure
/// ("when in doubt, keep"). A `BarrierReload` reads mutable frame state:
/// removable when unused, never consistent (so never folded or CSE'd).
fn op_effects(op: &SsaOp, ctx: &EffectCtx, shadowed: &BTreeSet<String>) -> Effects {
    match op {
        SsaOp::Phi(_) => Effects::total(),
        SsaOp::Unary { op, .. } => infer_unary_op_effects(op, &Effects::total()),
        SsaOp::Binary { op, .. } => {
            infer_binary_op_effects(op, &Effects::total(), &Effects::total())
        }
        SsaOp::Call {
            module,
            function,
            args,
            kwargs,
            splat_mask,
            kwargs_splat_mask,
        } => {
            if splat_mask.iter().any(|flag| *flag)
                || kwargs_splat_mask.iter().any(|flag| *flag)
                || shadowed.contains(function)
            {
                return Effects::arbitrary();
            }
            // Statically-resolved dispatch (Issue #9495): when the call's
            // argument types resolve to a single, unambiguous method of a
            // fully-visible multi-method generic, use that method's precise
            // per-method summary instead of the conservative name-level merge —
            // so a pure `f(::Int)` shadowed by an impure `f(::Float64)` is
            // still foldable/removable here. Restricted to bare calls (the
            // `by_name` map and the resolver are keyed by the bare generic
            // name); resolution is sound only when unambiguous, otherwise it
            // returns `None` and we fall through to `by_name`.
            if module.is_none() {
                if let Some(resolver) = ctx.resolver {
                    if let Some(arg_cores) = call_arg_cores(args, kwargs, ctx.params) {
                        if let Some(resolved) = resolver.resolve(function, &arg_cores) {
                            return resolved;
                        }
                    }
                }
            }
            ctx.by_name
                .get(function)
                .copied()
                .unwrap_or_else(|| infer_builtin_effects(function, &[]))
        }
        SsaOp::BarrierReload { .. } => Effects {
            consistent: EffectBit::AlwaysFalse,
            inaccessiblememonly: false,
            ..Effects::total()
        },
        SsaOp::Builtin { .. }
        | SsaOp::LoadGlobal { .. }
        | SsaOp::StoreGlobal { .. }
        | SsaOp::Opaque { .. }
        | SsaOp::OpaqueStmt { .. } => Effects::arbitrary(),
    }
}

/// Statically-known argument core types of a bare call, for static dispatch
/// resolution (Issue #9495). Returns `None` — leaving the call on the sound
/// name-level summary — whenever the argument tuple is not fully pinned:
/// keyword arguments (dispatch with kwargs is out of this slice), or any
/// positional argument whose static type is unknown (a non-constant, non-
/// parameter operand, or an untyped parameter). Splats are excluded by the
/// caller before this point.
fn call_arg_cores(
    args: &[SsaValue],
    kwargs: &[(String, SsaValue)],
    params: &[SsaParam],
) -> Option<Vec<CoreType>> {
    if !kwargs.is_empty() {
        return None;
    }
    args.iter().map(|v| ssa_value_arg_core(v, params)).collect()
}

/// Static core type of one call operand, or `None` when unknown. Only constant
/// literals (their type is exact) and typed parameters (`Argument` indices with
/// a declared annotation, a runtime guarantee within the method body) yield a
/// type; an SSA definition or an untyped parameter is unknown here.
fn ssa_value_arg_core(value: &SsaValue, params: &[SsaParam]) -> Option<CoreType> {
    match value {
        SsaValue::Const(lit) => literal_arg_core(lit),
        SsaValue::Argument(i) => params.get(*i)?.ty.as_ref().map(CoreType::from),
        SsaValue::Def(_) => None,
    }
}

/// Core type of a scalar literal used in dispatch-argument position. Non-scalar
/// or unmodeled literals return `None` (the call stays on the name-level
/// summary).
fn literal_arg_core(lit: &Literal) -> Option<CoreType> {
    let primitive = match lit {
        Literal::Int(_) => CorePrimitive::Int64,
        Literal::Int128(_) => CorePrimitive::Int128,
        Literal::Float(_) => CorePrimitive::Float64,
        Literal::Float32(_) => CorePrimitive::Float32,
        Literal::Float16(_) => CorePrimitive::Float16,
        Literal::Bool(_) => CorePrimitive::Bool,
        Literal::Str(_) => CorePrimitive::String,
        Literal::Char(_) => CorePrimitive::Char,
        _ => return None,
    };
    Some(CoreType::Primitive(primitive))
}

/// Common subexpression elimination for pure calls (Issue #8551): value-number
/// calls by callee and operand identity in a depth-first walk of the dominator
/// tree, so a call is merged only into an identical call whose definition
/// dominates it (same block earlier, or an ancestor in the dominator tree —
/// never a sibling branch).
///
/// Eligibility requires the callee's effect summary to satisfy
/// [`Effects::is_foldable`]: `:consistent` + `:effect_free` + terminating +
/// `inaccessiblememonly`. Consistency excludes fresh-allocation callees
/// (`Effects::allocating`, Issue #7176); `inaccessiblememonly` excludes
/// callees whose result could observe mutable state written between the two
/// calls. `:nothrow` is not required: with identical operands and no
/// observable state, the dominating call has already exhibited the throw, so
/// the dominated duplicate cannot behave differently.
pub fn cse_pure_calls(func: &mut SsaFunction, effects: &HashMap<FuncId, Effects>) -> bool {
    cse_pure_calls_scoped(func, effects, &BTreeSet::new())
}

/// [`cse_pure_calls`] with additional locally-bound callee names whose
/// summaries must be treated as arbitrary (Issue #8799; see
/// [`optimize_scoped`]).
pub fn cse_pure_calls_scoped(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    locally_bound: &BTreeSet<String>,
) -> bool {
    cse_pure_calls_scoped_inner(func, effects, None, locally_bound)
}

/// [`cse_pure_calls_scoped`] with the optional static-dispatch resolver (Issue
/// #9495): two identical calls that statically resolve to the same pure method
/// are CSE'd even when that name's merged summary (over impure siblings) is not
/// foldable.
fn cse_pure_calls_scoped_inner(
    func: &mut SsaFunction,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    locally_bound: &BTreeSet<String>,
) -> bool {
    let shadowed = shadowed_callee_names(func, locally_bound);
    let ctx = EffectCtx {
        by_name: effects,
        resolver,
        params: &func.params,
    };
    let reachable = compute_reachable(func);
    let idoms = compute_idoms(func, &reachable);
    let mut children: BTreeMap<BlockId, Vec<BlockId>> = BTreeMap::new();
    for (&block, &idom) in &idoms {
        if block != idom {
            children.entry(idom).or_default().push(block);
        }
    }

    // Scoped value-numbering table: entries are inserted at the first
    // (dominating) occurrence and removed when the dominator-tree DFS leaves
    // their scope; occupied entries are never replaced, so later occurrences
    // always merge into the dominating definition.
    let mut table: HashMap<CallKey, SsaValueId> = HashMap::new();
    let mut subst: BTreeMap<SsaValueId, SsaValueId> = BTreeMap::new();
    enum Action {
        Enter(BlockId),
        Exit(Vec<CallKey>),
    }
    let mut stack = vec![Action::Enter(func.entry)];
    while let Some(action) = stack.pop() {
        match action {
            Action::Exit(inserted) => {
                for key in inserted {
                    table.remove(&key);
                }
            }
            Action::Enter(block_id) => {
                let mut inserted = Vec::new();
                for stmt in &func.blocks[block_id.0 as usize].stmts {
                    let Some(key) = call_key(&stmt.op, &ctx, &shadowed, &subst) else {
                        continue;
                    };
                    match table.entry(key) {
                        std::collections::hash_map::Entry::Occupied(entry) => {
                            subst.insert(stmt.id, *entry.get());
                        }
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            inserted.push(entry.key().clone());
                            entry.insert(stmt.id);
                        }
                    }
                }
                stack.push(Action::Exit(inserted));
                if let Some(child_blocks) = children.get(&block_id) {
                    for &child in child_blocks.iter().rev() {
                        stack.push(Action::Enter(child));
                    }
                }
            }
        }
    }

    if subst.is_empty() {
        return false;
    }
    let replacements: BTreeMap<SsaValueId, SsaValue> = subst
        .iter()
        .map(|(&duplicate, &original)| (duplicate, SsaValue::Def(original)))
        .collect();
    apply_substitutions(func, &replacements);
    for block in &mut func.blocks {
        block.stmts.retain(|stmt| !subst.contains_key(&stmt.id));
    }
    debug_assert_eq!(
        super::verify(func),
        Ok(()),
        "SSA verifier failed after pure-call CSE (Issue #8551)"
    );
    true
}

/// Value-numbering key of one call: callee identity plus per-operand keys.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CallKey {
    module: Option<String>,
    function: String,
    args: Vec<ValueKey>,
    kwargs: Vec<(String, ValueKey)>,
}

/// Hashable operand identity for value numbering. Only scalar constants are
/// keyed; a call carrying any other literal payload does not participate.
/// Floats key by bit pattern, so `-0.0` and `0.0` (and distinct NaN payloads)
/// stay distinct — conservative for merging.
#[derive(Clone, PartialEq, Eq, Hash)]
enum ValueKey {
    Def(u32),
    Argument(usize),
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(String),
    Char(char),
    Symbol(String),
    Nothing,
}

fn call_key(
    op: &SsaOp,
    ctx: &EffectCtx,
    shadowed: &BTreeSet<String>,
    subst: &BTreeMap<SsaValueId, SsaValueId>,
) -> Option<CallKey> {
    let SsaOp::Call {
        module,
        function,
        args,
        kwargs,
        ..
    } = op
    else {
        return None;
    };
    if !op_effects(op, ctx, shadowed).is_foldable() {
        return None;
    }
    let args = args
        .iter()
        .map(|value| value_key(value, subst))
        .collect::<Option<Vec<_>>>()?;
    let kwargs = kwargs
        .iter()
        .map(|(name, value)| Some((name.clone(), value_key(value, subst)?)))
        .collect::<Option<Vec<_>>>()?;
    Some(CallKey {
        module: module.clone(),
        function: function.clone(),
        args,
        kwargs,
    })
}

/// Operand key, resolving definitions already merged earlier in this pass so
/// chains of duplicates (`h(f(x))` twice) value-number equal.
fn value_key(value: &SsaValue, subst: &BTreeMap<SsaValueId, SsaValueId>) -> Option<ValueKey> {
    match value {
        SsaValue::Def(id) => {
            // The substitution map is flat: an id recorded as a duplicate maps
            // to a table entry, and table entries are never themselves
            // recorded as duplicates.
            let id = subst.get(id).copied().unwrap_or(*id);
            Some(ValueKey::Def(id.0))
        }
        SsaValue::Argument(index) => Some(ValueKey::Argument(*index)),
        SsaValue::Const(literal) => literal_value_key(literal),
    }
}

fn literal_value_key(literal: &Literal) -> Option<ValueKey> {
    match literal {
        Literal::Int(v) => Some(ValueKey::Int(*v)),
        Literal::Float(v) => Some(ValueKey::Float(v.to_bits())),
        Literal::Bool(v) => Some(ValueKey::Bool(*v)),
        Literal::Str(s) => Some(ValueKey::Str(s.clone())),
        Literal::Char(c) => Some(ValueKey::Char(*c)),
        Literal::Symbol(s) => Some(ValueKey::Symbol(s.clone())),
        Literal::Nothing => Some(ValueKey::Nothing),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::super::test_util::{
        assign, binop, build, func_with, if_stmt, phi_count, ret, while_stmt,
    };
    use super::*;
    use crate::compile::test_helpers::{call_expr, int_lit, var_expr, zero_span};
    use crate::ir::core::{BinaryOp, Expr, Stmt, UnaryOp};

    fn count_calls(func: &SsaFunction, name: &str) -> usize {
        func.blocks
            .iter()
            .flat_map(|block| &block.stmts)
            .filter(|stmt| matches!(&stmt.op, SsaOp::Call { function, .. } if function == name))
            .count()
    }

    fn stmt_count(func: &SsaFunction) -> usize {
        func.blocks.iter().map(|block| block.stmts.len()).sum()
    }

    // -----------------------------------------------------------------------
    // fold_constants
    // -----------------------------------------------------------------------

    #[test]
    fn ssa_fold_const_binary_to_return_constant() {
        // x = 41; y = x + 1; return y — the Issue #8551 acceptance shape for
        // `f(x) = x + 1` over a constant argument: the add folds to 42 and no
        // statements remain.
        let func = func_with(
            &[],
            vec![
                assign("x", int_lit(41)),
                assign("y", binop(BinaryOp::Add, var_expr("x"), int_lit(1))),
                ret(var_expr("y")),
            ],
        );
        let mut ssa = build(&func);
        assert!(fold_constants(&mut ssa));
        assert_eq!(super::super::verify(&ssa), Ok(()));
        assert_eq!(stmt_count(&ssa), 0);
        assert_eq!(
            ssa.blocks[ssa.entry.0 as usize].terminator,
            Terminator::Return {
                value: Some(SsaValue::Const(Literal::Int(42)))
            }
        );
    }

    #[test]
    fn ssa_fold_const_unary_negation() {
        // return -(3) — unary fold through the shared evaluator.
        let func = func_with(
            &[],
            vec![ret(Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand: Box::new(int_lit(3)),
                span: zero_span(),
            })],
        );
        let mut ssa = build(&func);
        assert!(fold_constants(&mut ssa));
        assert_eq!(stmt_count(&ssa), 0);
        assert_eq!(
            ssa.blocks[0].terminator,
            Terminator::Return {
                value: Some(SsaValue::Const(Literal::Int(-3)))
            }
        );
    }

    #[test]
    fn ssa_fold_foldable_const_call_issue_9497() {
        // return abs(-5) — `abs` is foldable and nothrow, and all arguments are
        // scalar constants. The effects-aware optimizer can use the same const
        // evaluator as operator folding and delete the call.
        let func = func_with(&[], vec![ret(call_expr("abs", vec![int_lit(-5)]))]);
        let mut ssa = build(&func);

        optimize_with_effects(&mut ssa, &HashMap::new());

        assert_eq!(count_calls(&ssa, "abs"), 0);
        assert_eq!(stmt_count(&ssa), 0);
        assert_eq!(
            ssa.blocks[0].terminator,
            Terminator::Return {
                value: Some(SsaValue::Const(Literal::Int(5)))
            }
        );
        assert_eq!(super::super::verify(&ssa), Ok(()));
    }

    #[test]
    fn ssa_fold_foldable_const_call_respects_shadowed_callee_issue_9497() {
        // abs = println; return abs(-5) — the local rebinding means the call is
        // not the summarized builtin `abs`, so concrete eval must not fold it.
        let func = func_with(
            &[],
            vec![
                assign("abs", var_expr("println")),
                ret(call_expr("abs", vec![int_lit(-5)])),
            ],
        );
        let mut ssa = build(&func);
        let locally_bound = BTreeSet::from(["abs".to_string()]);

        optimize_scoped(&mut ssa, &HashMap::new(), &locally_bound);

        assert_eq!(count_calls(&ssa, "abs"), 1);
        assert_eq!(super::super::verify(&ssa), Ok(()));
    }

    #[test]
    fn ssa_fold_propagates_through_chained_defs() {
        // x = 1 + 2; y = x * 3; return y — folding x feeds the next round's
        // fold of y (fixpoint propagation).
        let func = func_with(
            &[],
            vec![
                assign("x", binop(BinaryOp::Add, int_lit(1), int_lit(2))),
                assign("y", binop(BinaryOp::Mul, var_expr("x"), int_lit(3))),
                ret(var_expr("y")),
            ],
        );
        let mut ssa = build(&func);
        assert!(fold_constants(&mut ssa));
        assert_eq!(stmt_count(&ssa), 0);
        assert_eq!(
            ssa.blocks[0].terminator,
            Terminator::Return {
                value: Some(SsaValue::Const(Literal::Int(9)))
            }
        );
    }

    #[test]
    fn ssa_fold_keeps_division_by_zero() {
        // return 1 ÷ 0 — the evaluator refuses, so the op and its runtime
        // DivideError are preserved.
        let func = func_with(
            &[],
            vec![ret(binop(BinaryOp::IntDiv, int_lit(1), int_lit(0)))],
        );
        let mut ssa = build(&func);
        assert!(!fold_constants(&mut ssa));
        assert_eq!(stmt_count(&ssa), 1);
    }

    #[test]
    fn ssa_fold_constant_phi_after_arm_folding() {
        // if c; x = 40 + 1 else x = 41 end; return x — after the then-arm
        // folds to 41, the join phi has equal incoming constants and folds
        // away (the SSA generalization of the bridge's identical-assignment
        // fold).
        let func = func_with(
            &["c"],
            vec![
                if_stmt(
                    var_expr("c"),
                    vec![assign("x", binop(BinaryOp::Add, int_lit(40), int_lit(1)))],
                    Some(vec![assign("x", int_lit(41))]),
                ),
                ret(var_expr("x")),
            ],
        );
        let mut ssa = build(&func);
        assert!(fold_constants(&mut ssa));
        let join = &ssa.blocks[3];
        assert_eq!(phi_count(join), 0);
        assert_eq!(
            join.terminator,
            Terminator::Return {
                value: Some(SsaValue::Const(Literal::Int(41)))
            }
        );
    }

    #[test]
    fn ssa_fold_trivial_loop_header_phi_with_self_latch() {
        // x = f(); while c; x = x; end; return x — the unpruned header phi's
        // latch operand is the phi itself, so it is trivial and folds to the
        // preheader definition.
        let func = func_with(
            &["c"],
            vec![
                assign("x", call_expr("f", vec![])),
                while_stmt(var_expr("c"), vec![assign("x", var_expr("x"))]),
                ret(var_expr("x")),
            ],
        );
        let mut ssa = build(&func);
        let f_id = ssa.blocks[0].stmts[0].id;
        assert!(fold_constants(&mut ssa));
        assert!(ssa.blocks.iter().all(|block| phi_count(block) == 0));
        let exit = ssa
            .blocks
            .iter()
            .find(|block| matches!(block.terminator, Terminator::Return { .. }))
            .expect("exit block");
        assert_eq!(
            exit.terminator,
            Terminator::Return {
                value: Some(SsaValue::Def(f_id))
            }
        );
    }

    #[test]
    fn ssa_fold_constant_true_branch_to_jump() {
        // if true; x = f() else x = g() end; return x — the branch becomes a
        // jump to the then arm; the else arm loses its entry edge (deleting
        // the unreachable block itself is DCE's job).
        let func = func_with(
            &[],
            vec![
                if_stmt(
                    Expr::Literal(Literal::Bool(true), zero_span()),
                    vec![assign("x", call_expr("f", vec![]))],
                    Some(vec![assign("x", call_expr("g", vec![]))]),
                ),
                ret(var_expr("x")),
            ],
        );
        let mut ssa = build(&func);
        assert!(fold_constants(&mut ssa));
        assert_eq!(super::super::verify(&ssa), Ok(()));
        assert_eq!(
            ssa.blocks[0].terminator,
            Terminator::Jump { target: BlockId(1) }
        );
        // The else block (b2) is no longer a predecessor-reachable target of
        // the entry; only its own jump into the join remains.
        assert_eq!(ssa.blocks[2].preds, Vec::<BlockId>::new());
    }

    #[test]
    fn ssa_fold_keeps_non_bool_constant_branch() {
        // if 1; f() end — Julia raises TypeError for non-Bool conditions, so
        // the branch must survive folding.
        let func = func_with(
            &[],
            vec![if_stmt(
                int_lit(1),
                vec![Stmt::Expr {
                    expr: call_expr("f", vec![]),
                    span: zero_span(),
                }],
                None,
            )],
        );
        let mut ssa = build(&func);
        assert!(!fold_constants(&mut ssa));
        assert!(matches!(
            ssa.blocks[0].terminator,
            Terminator::Branch { .. }
        ));
        assert_eq!(count_calls(&ssa, "f"), 1);
    }

    // -----------------------------------------------------------------------
    // eliminate_unreachable_blocks
    // -----------------------------------------------------------------------

    #[test]
    fn ssa_dce_deletes_blocks_unreachable_after_branch_fold() {
        // if false; x = g() else x = f() end; return x — folding kills the
        // then arm; unreachable elimination deletes its block and prunes the
        // join phi's dead edge.
        let func = func_with(
            &[],
            vec![
                if_stmt(
                    Expr::Literal(Literal::Bool(false), zero_span()),
                    vec![assign("x", call_expr("g", vec![]))],
                    Some(vec![assign("x", call_expr("f", vec![]))]),
                ),
                ret(var_expr("x")),
            ],
        );
        let mut ssa = build(&func);
        assert!(fold_constants(&mut ssa));
        let blocks_before = ssa.blocks.len();
        assert!(eliminate_unreachable_blocks(&mut ssa));
        assert!(ssa.blocks.len() < blocks_before);
        assert_eq!(super::super::verify(&ssa), Ok(()));
        assert_eq!(count_calls(&ssa, "g"), 0);
        assert_eq!(count_calls(&ssa, "f"), 1);
        // The join phi degenerated to a single edge; a following fold round
        // removes it entirely.
        assert!(fold_constants(&mut ssa));
        assert!(ssa.blocks.iter().all(|block| phi_count(block) == 0));
    }

    #[test]
    fn ssa_dce_unreachable_noop_when_all_reachable() {
        let func = func_with(
            &["c"],
            vec![
                if_stmt(
                    var_expr("c"),
                    vec![assign("x", call_expr("f", vec![]))],
                    None,
                ),
                ret(int_lit(1)),
            ],
        );
        let mut ssa = build(&func);
        assert!(!eliminate_unreachable_blocks(&mut ssa));
    }

    // -----------------------------------------------------------------------
    // eliminate_dead_defs
    // -----------------------------------------------------------------------

    #[test]
    fn ssa_dce_removes_unused_pure_binary() {
        // t = a + b; return a — the unused add is effect-free/nothrow per the
        // effects machinery and is deleted.
        let func = func_with(
            &["a", "b"],
            vec![
                assign("t", binop(BinaryOp::Add, var_expr("a"), var_expr("b"))),
                ret(var_expr("a")),
            ],
        );
        let mut ssa = build(&func);
        assert!(eliminate_dead_defs(&mut ssa, &HashMap::new()));
        assert_eq!(stmt_count(&ssa), 0);
    }

    #[test]
    fn ssa_dce_keeps_unused_may_throw_intdiv() {
        // t = a ÷ b; return a — ÷ may throw DivideError, so the unused def
        // must survive (upstream requires :nothrow for deletion).
        let func = func_with(
            &["a", "b"],
            vec![
                assign("t", binop(BinaryOp::IntDiv, var_expr("a"), var_expr("b"))),
                ret(var_expr("a")),
            ],
        );
        let mut ssa = build(&func);
        assert!(!eliminate_dead_defs(&mut ssa, &HashMap::new()));
        assert_eq!(stmt_count(&ssa), 1);
    }

    #[test]
    fn ssa_dce_keeps_call_with_unknown_effects() {
        // f(); return 1 — no summary for f, so the call is arbitrary and kept.
        let func = func_with(
            &[],
            vec![
                Stmt::Expr {
                    expr: call_expr("f", vec![]),
                    span: zero_span(),
                },
                ret(int_lit(1)),
            ],
        );
        let mut ssa = build(&func);
        assert!(!eliminate_dead_defs(&mut ssa, &HashMap::new()));
        assert_eq!(count_calls(&ssa, "f"), 1);
    }

    #[test]
    fn ssa_dce_removes_unused_call_with_removable_summary() {
        // pure_fn(a); return a — the body-derived summary proves the call
        // effect-free/nothrow/terminating, so the unused def is deleted.
        let func = func_with(
            &["a"],
            vec![
                Stmt::Expr {
                    expr: call_expr("pure_fn", vec![var_expr("a")]),
                    span: zero_span(),
                },
                ret(var_expr("a")),
            ],
        );
        let mut ssa = build(&func);
        let effects = HashMap::from([("pure_fn".to_string(), Effects::pure_arithmetic())]);
        assert!(eliminate_dead_defs(&mut ssa, &effects));
        assert_eq!(count_calls(&ssa, "pure_fn"), 0);
    }

    #[test]
    fn ssa_dce_keeps_summarized_call_through_shadowing_param() {
        // Function parameter `pure_fn` shadows the summarized global name:
        // the call dispatches on the runtime argument, so the summary must
        // not be trusted and the call is kept.
        let func = func_with(
            &["pure_fn", "a"],
            vec![
                Stmt::Expr {
                    expr: call_expr("pure_fn", vec![var_expr("a")]),
                    span: zero_span(),
                },
                ret(var_expr("a")),
            ],
        );
        let mut ssa = build(&func);
        let effects = HashMap::from([("pure_fn".to_string(), Effects::pure_arithmetic())]);
        assert!(!eliminate_dead_defs(&mut ssa, &effects));
        assert_eq!(count_calls(&ssa, "pure_fn"), 1);
    }

    #[test]
    fn ssa_dce_removes_dead_loop_header_phi_cycle() {
        // i = 0; while c; i = i + 1 end; return 7 — the header phi and the
        // increment only feed each other; the mark & sweep removes the whole
        // dead cycle (the unpruned loop-header phi case from Issue #8550).
        let func = func_with(
            &["c"],
            vec![
                assign("i", int_lit(0)),
                while_stmt(
                    var_expr("c"),
                    vec![assign("i", binop(BinaryOp::Add, var_expr("i"), int_lit(1)))],
                ),
                ret(int_lit(7)),
            ],
        );
        let mut ssa = build(&func);
        assert!(eliminate_dead_defs(&mut ssa, &HashMap::new()));
        assert_eq!(stmt_count(&ssa), 0);
        assert_eq!(super::super::verify(&ssa), Ok(()));
    }

    #[test]
    fn ssa_dce_keeps_loop_phi_feeding_return() {
        // i = 0; while i < n; i = i + 1 end; return i — everything is live.
        let func = func_with(
            &["n"],
            vec![
                assign("i", int_lit(0)),
                while_stmt(
                    binop(BinaryOp::Lt, var_expr("i"), var_expr("n")),
                    vec![assign("i", binop(BinaryOp::Add, var_expr("i"), int_lit(1)))],
                ),
                ret(var_expr("i")),
            ],
        );
        let mut ssa = build(&func);
        let before = stmt_count(&ssa);
        assert!(!eliminate_dead_defs(&mut ssa, &HashMap::new()));
        assert_eq!(stmt_count(&ssa), before);
    }

    #[test]
    fn ssa_dce_keeps_opaque_barrier_and_removes_unused_reload() {
        // x = f(); try; x = g(x); catch; end; return 1 — the opaque try/catch
        // barrier stays (arbitrary effects) and keeps its read of x live, but
        // the unused barrier reload of x is deleted.
        let func = func_with(
            &[],
            vec![
                assign("x", call_expr("f", vec![])),
                Stmt::Try {
                    try_block: super::super::test_util::block(vec![assign(
                        "x",
                        call_expr("g", vec![var_expr("x")]),
                    )]),
                    catch_var: None,
                    catch_block: Some(super::super::test_util::block(vec![])),
                    else_block: None,
                    finally_block: None,
                    span: zero_span(),
                },
                ret(int_lit(1)),
            ],
        );
        let mut ssa = build(&func);
        assert!(eliminate_dead_defs(&mut ssa, &HashMap::new()));
        let entry = &ssa.blocks[0];
        assert_eq!(entry.stmts.len(), 2);
        assert!(matches!(entry.stmts[0].op, SsaOp::Call { .. }));
        assert!(matches!(entry.stmts[1].op, SsaOp::OpaqueStmt { .. }));
    }

    // -----------------------------------------------------------------------
    // cse_pure_calls
    // -----------------------------------------------------------------------

    fn pure_effects(names: &[&str]) -> HashMap<FuncId, Effects> {
        names
            .iter()
            .map(|name| ((*name).to_string(), Effects::pure_arithmetic()))
            .collect()
    }

    #[test]
    fn ssa_cse_merges_repeated_pure_call_in_block() {
        // x = min(a, b); y = min(a, b); return x + y — with a body-derived
        // pure summary for min, the duplicate call merges into the first.
        let func = func_with(
            &["a", "b"],
            vec![
                assign("x", call_expr("min", vec![var_expr("a"), var_expr("b")])),
                assign("y", call_expr("min", vec![var_expr("a"), var_expr("b")])),
                ret(binop(BinaryOp::Add, var_expr("x"), var_expr("y"))),
            ],
        );
        let mut ssa = build(&func);
        let first_id = ssa.blocks[0].stmts[0].id;
        assert!(cse_pure_calls(&mut ssa, &pure_effects(&["min"])));
        assert_eq!(super::super::verify(&ssa), Ok(()));
        assert_eq!(count_calls(&ssa, "min"), 1);
        let entry = &ssa.blocks[0];
        assert!(matches!(
            &entry.stmts[1].op,
            SsaOp::Binary { left, right, .. }
                if left == &SsaValue::Def(first_id) && right == &SsaValue::Def(first_id)
        ));
    }

    #[test]
    fn ssa_cse_requires_effects_summary() {
        // Without a summary, `min` falls back to the builtin name table
        // (arbitrary) and both calls survive.
        let func = func_with(
            &["a", "b"],
            vec![
                assign("x", call_expr("min", vec![var_expr("a"), var_expr("b")])),
                assign("y", call_expr("min", vec![var_expr("a"), var_expr("b")])),
                ret(binop(BinaryOp::Add, var_expr("x"), var_expr("y"))),
            ],
        );
        let mut ssa = build(&func);
        assert!(!cse_pure_calls(&mut ssa, &HashMap::new()));
        assert_eq!(count_calls(&ssa, "min"), 2);
    }

    #[test]
    fn ssa_cse_key_includes_arguments() {
        // p(a) and p(b) are different values and must not merge.
        let func = func_with(
            &["a", "b"],
            vec![
                assign("x", call_expr("p", vec![var_expr("a")])),
                assign("y", call_expr("p", vec![var_expr("b")])),
                ret(binop(BinaryOp::Add, var_expr("x"), var_expr("y"))),
            ],
        );
        let mut ssa = build(&func);
        assert!(!cse_pure_calls(&mut ssa, &pure_effects(&["p"])));
        assert_eq!(count_calls(&ssa, "p"), 2);
    }

    #[test]
    fn ssa_cse_does_not_merge_sibling_branches() {
        // if c; x = p(a); f(x) else y = p(a); g(y) end — neither call
        // dominates the other, so both survive.
        let func = func_with(
            &["c", "a"],
            vec![
                if_stmt(
                    var_expr("c"),
                    vec![
                        assign("x", call_expr("p", vec![var_expr("a")])),
                        Stmt::Expr {
                            expr: call_expr("f", vec![var_expr("x")]),
                            span: zero_span(),
                        },
                    ],
                    Some(vec![
                        assign("y", call_expr("p", vec![var_expr("a")])),
                        Stmt::Expr {
                            expr: call_expr("g", vec![var_expr("y")]),
                            span: zero_span(),
                        },
                    ]),
                ),
                ret(int_lit(0)),
            ],
        );
        let mut ssa = build(&func);
        assert!(!cse_pure_calls(&mut ssa, &pure_effects(&["p"])));
        assert_eq!(count_calls(&ssa, "p"), 2);
    }

    #[test]
    fn ssa_cse_merges_across_dominating_block() {
        // x = p(a); if c; f(p(a)) end; return x — the entry call dominates
        // the then-block duplicate.
        let func = func_with(
            &["c", "a"],
            vec![
                assign("x", call_expr("p", vec![var_expr("a")])),
                if_stmt(
                    var_expr("c"),
                    vec![Stmt::Expr {
                        expr: call_expr("f", vec![call_expr("p", vec![var_expr("a")])]),
                        span: zero_span(),
                    }],
                    None,
                ),
                ret(var_expr("x")),
            ],
        );
        let mut ssa = build(&func);
        let first_id = ssa.blocks[0].stmts[0].id;
        assert!(cse_pure_calls(&mut ssa, &pure_effects(&["p"])));
        assert_eq!(count_calls(&ssa, "p"), 1);
        let f_call = ssa
            .blocks
            .iter()
            .flat_map(|block| &block.stmts)
            .find(|stmt| matches!(&stmt.op, SsaOp::Call { function, .. } if function == "f"))
            .expect("f call survives");
        assert!(matches!(
            &f_call.op,
            SsaOp::Call { args, .. } if args == &[SsaValue::Def(first_id)]
        ));
    }

    #[test]
    fn ssa_cse_skips_allocating_calls() {
        // Fresh allocations are effect-free but not :consistent — merging
        // them would alias independently-mutated results (Issue #7176).
        let func = func_with(
            &["n"],
            vec![
                assign("x", call_expr("alloc", vec![var_expr("n")])),
                assign("y", call_expr("alloc", vec![var_expr("n")])),
                ret(binop(BinaryOp::Egal, var_expr("x"), var_expr("y"))),
            ],
        );
        let mut ssa = build(&func);
        let effects = HashMap::from([("alloc".to_string(), Effects::allocating())]);
        assert!(!cse_pure_calls(&mut ssa, &effects));
        assert_eq!(count_calls(&ssa, "alloc"), 2);
    }

    #[test]
    fn ssa_cse_resolves_duplicate_chains() {
        // s = h(p(a)); t = h(p(a)); return s + t — after the inner p calls
        // merge, the outer h calls value-number equal through the
        // substitution map and merge too.
        let func = func_with(
            &["a"],
            vec![
                assign(
                    "s",
                    call_expr("h", vec![call_expr("p", vec![var_expr("a")])]),
                ),
                assign(
                    "t",
                    call_expr("h", vec![call_expr("p", vec![var_expr("a")])]),
                ),
                ret(binop(BinaryOp::Add, var_expr("s"), var_expr("t"))),
            ],
        );
        let mut ssa = build(&func);
        assert!(cse_pure_calls(&mut ssa, &pure_effects(&["h", "p"])));
        assert_eq!(count_calls(&ssa, "p"), 1);
        assert_eq!(count_calls(&ssa, "h"), 1);
    }

    // -----------------------------------------------------------------------
    // optimize / optimize_with_effects (combined driver)
    // -----------------------------------------------------------------------

    #[test]
    fn ssa_optimize_folds_branch_and_eliminates_dead_arm() {
        // x = 2 + 3; if x < 10; r = f() else r = g() end; return r — the
        // acceptance shape: folding decides the branch, DCE removes the dead
        // arm, and the phi collapses to the surviving call.
        let func = func_with(
            &[],
            vec![
                assign("x", binop(BinaryOp::Add, int_lit(2), int_lit(3))),
                if_stmt(
                    binop(BinaryOp::Lt, var_expr("x"), int_lit(10)),
                    vec![assign("r", call_expr("f", vec![]))],
                    Some(vec![assign("r", call_expr("g", vec![]))]),
                ),
                ret(var_expr("r")),
            ],
        );
        let mut ssa = build(&func);
        optimize(&mut ssa);
        assert_eq!(super::super::verify(&ssa), Ok(()));
        assert_eq!(count_calls(&ssa, "g"), 0);
        assert_eq!(count_calls(&ssa, "f"), 1);
        assert!(ssa
            .blocks
            .iter()
            .all(|block| !matches!(block.terminator, Terminator::Branch { .. })));
        assert!(ssa.blocks.iter().all(|block| phi_count(block) == 0));
        let f_id = ssa
            .blocks
            .iter()
            .flat_map(|block| &block.stmts)
            .find(|stmt| matches!(&stmt.op, SsaOp::Call { function, .. } if function == "f"))
            .expect("f call survives")
            .id;
        let exit = ssa
            .blocks
            .iter()
            .find(|block| matches!(block.terminator, Terminator::Return { .. }))
            .expect("return block");
        assert_eq!(
            exit.terminator,
            Terminator::Return {
                value: Some(SsaValue::Def(f_id))
            }
        );
    }

    #[test]
    fn ssa_optimize_is_idempotent() {
        let func = func_with(
            &["a", "b"],
            vec![
                assign("x", call_expr("min", vec![var_expr("a"), var_expr("b")])),
                assign("y", call_expr("min", vec![var_expr("a"), var_expr("b")])),
                ret(binop(BinaryOp::Add, var_expr("x"), var_expr("y"))),
            ],
        );
        let mut ssa = build(&func);
        let effects = pure_effects(&["min"]);
        optimize_with_effects(&mut ssa, &effects);
        let after_first = ssa.clone();
        optimize_with_effects(&mut ssa, &effects);
        assert_eq!(ssa, after_first);
    }

    #[test]
    fn ssa_optimize_with_body_derived_effects_end_to_end() {
        // The purity of `mymin` is derived from its body by the Issue #8441
        // machinery (infer_program_effects), not by name: the duplicate call
        // in `caller` is then CSE'd.
        use crate::compile::effects::propagation::infer_program_effects;
        use crate::ir::core::{Block, Program};

        let mymin = {
            let mut f = func_with(
                &["x", "y"],
                vec![ret(Expr::Ternary {
                    condition: Box::new(binop(BinaryOp::Lt, var_expr("x"), var_expr("y"))),
                    then_expr: Box::new(var_expr("x")),
                    else_expr: Box::new(var_expr("y")),
                    span: zero_span(),
                })],
            );
            f.name = "mymin".to_string();
            f
        };
        let caller = {
            let mut f = func_with(
                &["a", "b"],
                vec![
                    assign("s", call_expr("mymin", vec![var_expr("a"), var_expr("b")])),
                    assign("t", call_expr("mymin", vec![var_expr("a"), var_expr("b")])),
                    ret(binop(BinaryOp::Add, var_expr("s"), var_expr("t"))),
                ],
            );
            f.name = "caller".to_string();
            f
        };
        let program = Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            functions: vec![
                std::sync::Arc::new(mymin),
                std::sync::Arc::new(caller.clone()),
            ],
            base_function_count: 0,
            structs: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: Block {
                stmts: vec![],
                span: zero_span(),
            },
        };
        let effects = infer_program_effects(&program);
        assert!(effects["mymin"].is_foldable());

        let mut ssa = build(&caller);
        assert_eq!(count_calls(&ssa, "mymin"), 2);
        optimize_with_effects(&mut ssa, &effects);
        assert_eq!(count_calls(&ssa, "mymin"), 1);
        assert_eq!(super::super::verify(&ssa), Ok(()));
    }

    // -----------------------------------------------------------------------
    // per-method summaries at statically-resolved call sites (Issue #9495)
    // -----------------------------------------------------------------------

    /// Method `f(x::<param_ty>) = <body>` (untyped when `param_ty` is `None`).
    fn f_method(
        param_ty: Option<crate::types::JuliaType>,
        body: Expr,
    ) -> std::sync::Arc<crate::ir::core::Function> {
        let mut func = func_with(&["x"], vec![ret(body)]);
        func.name = "f".to_string();
        func.params[0].type_annotation = param_ty;
        std::sync::Arc::new(func)
    }

    /// `g(y::Int) = f(y) + f(y)` — two identical calls to `f` through a typed
    /// Int parameter.
    fn g_calls_f_twice() -> crate::ir::core::Function {
        let mut func = func_with(
            &["y"],
            vec![
                assign("s", call_expr("f", vec![var_expr("y")])),
                assign("t", call_expr("f", vec![var_expr("y")])),
                ret(binop(BinaryOp::Add, var_expr("s"), var_expr("t"))),
            ],
        );
        func.name = "g".to_string();
        func.params[0].type_annotation = Some(crate::types::JuliaType::Int64);
        func
    }

    fn program_of(
        functions: Vec<std::sync::Arc<crate::ir::core::Function>>,
    ) -> crate::ir::core::Program {
        crate::ir::core::Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            functions,
            base_function_count: 0,
            structs: vec![],
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: super::super::test_util::block(vec![]),
        }
    }

    #[test]
    fn ssa_cse_uses_by_method_summary_at_resolved_call_site_issue_9495() {
        use crate::compile::effects::propagation::infer_program_effects;
        use crate::types::JuliaType;

        // pure f(x::Int)=x+1 shadowed by impure f(x::Float64)=println(x).
        let program = program_of(vec![
            f_method(
                Some(JuliaType::Int64),
                binop(BinaryOp::Add, var_expr("x"), int_lit(1)),
            ),
            f_method(
                Some(JuliaType::Float64),
                call_expr("println", vec![var_expr("x")]),
            ),
        ]);
        let by_name = infer_program_effects(&program);
        // The name-level merge is poisoned by the impure sibling.
        assert!(!by_name["f"].is_foldable());
        let resolver =
            StaticDispatchResolver::build(&program, &by_name).expect("multi-method f qualifies");

        let caller = g_calls_f_twice();

        // by_name only: the impure-merged summary blocks CSE — both calls stay.
        let mut by_name_only = build(&caller);
        assert!(!cse_pure_calls_scoped(
            &mut by_name_only,
            &by_name,
            &BTreeSet::new()
        ));
        assert_eq!(count_calls(&by_name_only, "f"), 2);

        // resolver-aware: the Int call resolves to the pure method — CSE fires.
        let mut resolved = build(&caller);
        assert!(cse_pure_calls_scoped_inner(
            &mut resolved,
            &by_name,
            Some(&resolver),
            &BTreeSet::new()
        ));
        assert_eq!(count_calls(&resolved, "f"), 1);
        assert_eq!(super::super::verify(&resolved), Ok(()));
    }

    #[test]
    fn ssa_dce_removes_dead_resolved_pure_call_issue_9495() {
        use crate::compile::effects::propagation::infer_program_effects;
        use crate::types::JuliaType;

        // Same pure/impure pair. `h(y::Int) = (f(y); y)` — the f(y) result is
        // dead. The impure name-level merge keeps it; the resolved pure summary
        // makes it removable.
        let program = program_of(vec![
            f_method(
                Some(JuliaType::Int64),
                binop(BinaryOp::Add, var_expr("x"), int_lit(1)),
            ),
            f_method(
                Some(JuliaType::Float64),
                call_expr("println", vec![var_expr("x")]),
            ),
        ]);
        let by_name = infer_program_effects(&program);
        let resolver =
            StaticDispatchResolver::build(&program, &by_name).expect("multi-method f qualifies");

        let caller = {
            let mut func = func_with(
                &["y"],
                vec![
                    Stmt::Expr {
                        expr: call_expr("f", vec![var_expr("y")]),
                        span: zero_span(),
                    },
                    ret(var_expr("y")),
                ],
            );
            func.name = "h".to_string();
            func.params[0].type_annotation = Some(JuliaType::Int64);
            func
        };

        // by_name only: the dead call is kept (impure merge is not removable).
        let mut by_name_only = build(&caller);
        assert!(!eliminate_dead_defs_scoped(
            &mut by_name_only,
            &by_name,
            &BTreeSet::new()
        ));
        assert_eq!(count_calls(&by_name_only, "f"), 1);

        // resolver-aware: the resolved pure summary makes the dead call removable.
        let mut resolved = build(&caller);
        assert!(eliminate_dead_defs_scoped_inner(
            &mut resolved,
            &by_name,
            Some(&resolver),
            &BTreeSet::new()
        ));
        assert_eq!(count_calls(&resolved, "f"), 0);
        assert_eq!(super::super::verify(&resolved), Ok(()));
    }

    #[test]
    fn ssa_cse_bails_on_ambiguous_resolved_call_site_issue_9495() {
        use crate::compile::effects::propagation::infer_program_effects;
        use crate::types::JuliaType;

        // pure f(x::Int)=x+1 and untyped f(x)=println(x) (matches Any). An Int
        // call matches BOTH; the resolver bails, so the by-name merge (not
        // foldable) blocks CSE — the negative control for soundness.
        let program = program_of(vec![
            f_method(
                Some(JuliaType::Int64),
                binop(BinaryOp::Add, var_expr("x"), int_lit(1)),
            ),
            f_method(None, call_expr("println", vec![var_expr("x")])),
        ]);
        let by_name = infer_program_effects(&program);
        let resolver =
            StaticDispatchResolver::build(&program, &by_name).expect("multi-method f qualifies");

        let mut resolved = build(&g_calls_f_twice());
        assert!(!cse_pure_calls_scoped_inner(
            &mut resolved,
            &by_name,
            Some(&resolver),
            &BTreeSet::new()
        ));
        assert_eq!(count_calls(&resolved, "f"), 2);
    }

    // -----------------------------------------------------------------------
    // locally-bound callee scoping (Issue #8799)
    // -----------------------------------------------------------------------

    #[test]
    fn ssa_dce_scoped_keeps_call_through_locally_rebound_name_issue_8799() {
        // abs = println; abs(-5); return 0 — the call result is unused, but
        // the callee name is rebound locally by plain assignment: the curated
        // builtin summary for `abs` (pure, nothrow) must not be applied, so
        // the call survives and later forces the bytecode lowering's
        // per-function legacy fallback ("call through a locally bound name").
        let func = func_with(
            &[],
            vec![
                assign("abs", var_expr("println")),
                Stmt::Expr {
                    expr: call_expr("abs", vec![int_lit(-5)]),
                    span: zero_span(),
                },
                ret(int_lit(0)),
            ],
        );

        // Without the scoped set the misattributed summary deletes the call —
        // the Issue #8799 hazard this API closes at the gate site.
        let mut unscoped = build(&func);
        eliminate_dead_defs(&mut unscoped, &HashMap::new());
        assert_eq!(count_calls(&unscoped, "abs"), 0);

        let mut ssa = build(&func);
        let locally_bound = BTreeSet::from(["abs".to_string()]);
        eliminate_dead_defs_scoped(&mut ssa, &HashMap::new(), &locally_bound);
        assert_eq!(count_calls(&ssa, "abs"), 1);
        assert_eq!(super::super::verify(&ssa), Ok(()));
    }

    #[test]
    fn ssa_cse_scoped_blocks_locally_rebound_callee_merge_issue_8799() {
        // f = g; x = f(a); y = f(a); return x + y — a *global* summary for
        // `f` says pure, but the local rebinding means the two calls may not
        // be the summarized function at all: the scoped pass must not merge.
        let func = func_with(
            &["a"],
            vec![
                assign("f", var_expr("g")),
                assign("x", call_expr("f", vec![var_expr("a")])),
                assign("y", call_expr("f", vec![var_expr("a")])),
                ret(binop(BinaryOp::Add, var_expr("x"), var_expr("y"))),
            ],
        );

        let mut unscoped = build(&func);
        assert!(cse_pure_calls(&mut unscoped, &pure_effects(&["f"])));
        assert_eq!(count_calls(&unscoped, "f"), 1);

        let mut ssa = build(&func);
        let locally_bound = BTreeSet::from(["f".to_string()]);
        assert!(!cse_pure_calls_scoped(
            &mut ssa,
            &pure_effects(&["f"]),
            &locally_bound
        ));
        assert_eq!(count_calls(&ssa, "f"), 2);
    }
}
