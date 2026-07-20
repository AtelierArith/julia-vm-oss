//! SSA → stack bytecode lowering behind the `SJULIA_SSA_PIPELINE=1` gate
//! (Issue #8552).
//!
//! This is the first backend of the SSA layer: eligible user function bodies
//! go `Core IR → build_function → ssa_ir::opt passes → lower` instead of the
//! legacy `CoreCompiler::compile_function_body` walk. The lowering is
//! backend-shaped (block scheduling, phi placement, and value materialization
//! live here), but per-operation instruction selection is delegated to the
//! existing `CoreCompiler` emitters so calls, builtins and operators compile
//! byte-for-byte through the same dispatch machinery as the legacy path — no
//! new `Instr` variants, and the existing slotize/peephole passes finish the
//! function exactly as before.
//!
//! # Value materialization
//!
//! SSA definitions whose lifetime is stack-shaped — used exactly once, by the
//! next consumer in the same block, with the intervening statements folding
//! into the same operand tree — are rebuilt into nested Core IR expression
//! trees, so their values flow on the operand stack exactly like the legacy
//! compilation of the original nested expression. Everything else is
//! *spilled*: the definition is emitted as an assignment to a synthetic local
//! (`#ssaN`) and later uses read that local, with the existing
//! `vm/slot.rs` slotization assigning the frame slot. Phi nodes become slot
//! writes on their incoming edges: copies are emitted at the end of `Jump`
//! predecessors and on branch-edge trampolines for critical `Branch` edges;
//! interfering parallel copies (loop-carried swaps) are staged through
//! `#ssatmpN` temporaries.
//!
//! # Per-function fallback
//!
//! Anything the slice cannot prove equivalent falls back to the legacy path
//! for that one function, *before any bytecode is emitted* (the eligibility
//! and planning phases do not mutate the compiler). Fallback reasons include
//! `SsaBuildError`, opaque barriers (`for`/`try`/mutation/nested functions —
//! see `docs/vm/SSA_IR.md`), maybe-undefined phis (legacy `UndefVarError`
//! parity needs variable names SSA erased), calls through locally rebound
//! names, module-valued globals, short-circuit operands that SSA
//! construction evaluated eagerly, and runtime-specialized functions that
//! store locals (the VM specializer slotizes against this body's slot-name
//! table, which the SSA renaming would break — Issue #8440).
//! `SJULIA_SSA_PIPELINE_LOG=1` logs the decision per function.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::bytecode::{Instr, ValueType};
use crate::compile::effects::propagation::FuncId;
use crate::compile::effects::Effects;
use crate::ir::core::{BinaryOp, Block, Expr, Function, Stmt};

use super::super::constants::is_stdlib_module;
use super::super::core_compiler::CoreCompiler;
use super::super::inference::collect_local_types_with_mixed_tracking;
use super::super::stmt::{
    can_convert_type, collect_declared_globals, should_return_as_expected_type,
};
use super::super::types::CResult;
use super::build::build_function;
use super::model::{SsaFunction, SsaOp};
use super::opt::optimize_scoped_resolved;
use super::plan::{self, SharedCopyPlan, SharedFunctionPlan, SharedRootPlan, SharedTermPlan};
use super::scan;
use crate::compile::effects::static_dispatch::StaticDispatchResolver;

/// `SJULIA_SSA_PIPELINE_LOG` diagnostics sink. The crate denies
/// `clippy::print_stderr`; like the register VM gate (Issue #8558), explicit
/// opt-in debug logging writes through `std::io::stderr()` directly.
macro_rules! gate_log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

/// Per-thread counters for the SSA pipeline gate (compilation is
/// single-threaded per program; tests read these to assert the gated path
/// actually engaged). Only ever written when the gate is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SsaPipelineStats {
    /// Function bodies emitted through SSA lowering.
    pub lowered: u64,
    /// Gate-checked function bodies that fell back to the legacy path.
    pub fallbacks: u64,
}

thread_local! {
    static STATS: std::cell::Cell<SsaPipelineStats> =
        const { std::cell::Cell::new(SsaPipelineStats { lowered: 0, fallbacks: 0 }) };
}

/// Read and reset this thread's SSA pipeline counters.
pub fn take_ssa_pipeline_stats() -> SsaPipelineStats {
    STATS.with(|stats| stats.replace(SsaPipelineStats::default()))
}

fn note(lowered: bool) {
    STATS.with(|stats| {
        let mut s = stats.get();
        if lowered {
            s.lowered += 1;
        } else {
            s.fallbacks += 1;
        }
        stats.set(s);
    });
}

/// Whether the SSA pipeline is enabled. ON by default; set `SJULIA_SSA_PIPELINE=0`
/// (or `false`) to disable and force the legacy Core-IR path. Read once per
/// program compile by `compile_functions` (not cached process-wide so tests can
/// toggle the gate between compiles).
pub(in crate::compile) fn ssa_pipeline_enabled() -> bool {
    std::env::var("SJULIA_SSA_PIPELINE")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

fn log_enabled() -> bool {
    std::env::var_os("SJULIA_SSA_PIPELINE_LOG").is_some()
}

/// Outcome of the planning phases: either a clean per-function fallback (with
/// a human-readable reason) or a real compile error to propagate.
enum LowerError {
    Fallback(String),
    Compile(crate::compile::CompileError),
}

impl From<crate::compile::CompileError> for LowerError {
    fn from(e: crate::compile::CompileError) -> Self {
        Self::Compile(e)
    }
}

fn fallback<T>(reason: impl Into<String>) -> Result<T, LowerError> {
    Err(LowerError::Fallback(reason.into()))
}

/// How the function's tail was written in the source, deciding which legacy
/// return emission the lowering reproduces (`compile_function_body` converts
/// a trailing implicit expression via `emit_type_conversion`, but never an
/// explicit `return`).
#[derive(Clone, Copy, PartialEq)]
enum TailMode {
    /// Body ends in an explicit `return`; every `Return` terminator uses the
    /// explicit emission.
    Explicit,
    /// Body ends in a trailing value expression; `Return` terminators use the
    /// implicit emission (with I64↔F64 conversion toward the declared type).
    ImplicitExpr,
}

/// Try to compile `func`'s body through the SSA pipeline.
///
/// `effects` carries the body-derived per-function summaries of Issue #8441
/// (`infer_program_effects`), computed once per gated program compile by the
/// gate site; the passes fall back to the curated builtin name table for
/// callees without a summary. `resolver` (Issue #9495) optionally carries the
/// per-method summaries the DCE/CSE gates consult at statically-resolved call
/// sites, so a pure `f(::Int)` shadowed by an impure sibling is still
/// foldable/removable; `None` keeps every call on the name-level merge.
///
/// `runtime_specialized` marks functions registered in `spec_func_mapping`:
/// their call sites emit `CallSpecialize`, and the VM's
/// `install_specialized_body` slotizes the runtime-specialized bytecode
/// against **this generic body's slot-name table**. When such a body stores
/// locals, the SSA lowering would rename them to `#ssaN` slots, the
/// specialized body's source-named stores would miss the table, and every
/// access would degrade to name-based instructions (measured 5× on the
/// calc_pi loop) — so those functions fall back (Issue #8440).
///
/// Returns `Ok(Some(plan))` when the body was fully emitted (the caller must
/// skip the legacy `compile_function_body`), `Ok(None)` on a clean
/// per-function fallback with nothing emitted, and `Err` only for real compile
/// errors.
pub(in crate::compile) fn lower_function_body_via_ssa(
    compiler: &mut CoreCompiler<'_>,
    func: &Function,
    return_type: ValueType,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    runtime_specialized: bool,
) -> CResult<Option<SharedFunctionPlan>> {
    match try_lower(
        compiler,
        func,
        return_type,
        effects,
        resolver,
        runtime_specialized,
    ) {
        Ok(plan) => {
            note(true);
            if log_enabled() {
                gate_log!("[ssa-pipeline] {}: lowered via SSA", func.name);
            }
            Ok(Some(plan))
        }
        Err(LowerError::Fallback(reason)) => {
            note(false);
            if log_enabled() {
                gate_log!("[ssa-pipeline] {}: legacy path ({reason})", func.name);
            }
            Ok(None)
        }
        Err(LowerError::Compile(e)) => Err(e),
    }
}

fn try_lower(
    compiler: &mut CoreCompiler<'_>,
    func: &Function,
    return_type: ValueType,
    effects: &HashMap<FuncId, Effects>,
    resolver: Option<&StaticDispatchResolver>,
    runtime_specialized: bool,
) -> Result<SharedFunctionPlan, LowerError> {
    let tail_mode = eligibility_precheck(compiler, func, &return_type)?;

    // Names the body binds locally: SSA erased their identity (values flow
    // through defs), so a *call* through such a name must neither use the
    // name-keyed effect summary of an unrelated global/builtin (Issue #8799 —
    // the passes run before `scan_lowerable` and could delete the very call
    // that forces the fallback) nor reach emitted code (`scan_lowerable`).
    let mut locally_bound = BTreeSet::new();
    scan::block_write_names(&func.body, &mut locally_bound);
    let mut globals = BTreeSet::new();
    scan::collect_global_decls(&func.body, &mut globals);
    for name in &globals {
        locally_bound.remove(name);
    }

    // Runtime-specialized functions (untyped params, `CallSpecialize` call
    // sites) hand this body's slot-name table to the VM specializer
    // (`install_specialized_body`); a body that stores locals would publish
    // `#ssaN` names instead of the source names the specialized bytecode
    // stores to, degrading it to name-based instructions. Bodies without
    // local stores publish a params-only table on both paths, so they are
    // safe to lower (see `lower_function_body_via_ssa` docs, Issue #8440).
    if runtime_specialized && !locally_bound.is_empty() {
        return fallback(
            "runtime-specialized function stores locals (specializer slot-name table)",
        );
    }

    let mut ssa = match build_function(func) {
        Ok(ssa) => ssa,
        Err(e) => return fallback(format!("SSA build: {e}")),
    };
    optimize_scoped_resolved(&mut ssa, effects, resolver, &locally_bound);

    scan_lowerable(compiler, &locally_bound, &ssa)?;

    let convert_gate = numeric_convert_gate(compiler, func);
    let plan = plan::plan_function(&ssa, func.span, convert_gate)
        .map_err(|e| LowerError::Fallback(e.to_string()))?;

    // From here on the compiler is mutated; all fallback decisions above are
    // final, so a partial emission can no longer be left behind.
    emit_plan(compiler, func, &ssa, &plan, tail_mode, return_type)?;
    Ok(plan)
}

/// Structural numeric-conversion rewrite gate for this function (Issue
/// #9803): a bare `Int64(x)` / `Float64(x)` call may be rewritten to
/// `Expr::Convert` only when the name is PROVEN to resolve to the builtin
/// constructor — the same decision the stack compiler's
/// `compile_generic_dispatch_call` makes when it routes a call to
/// `compile_builtin_call` instead of user-method dispatch. Concretely, the
/// name must have no reachable method table (module-owned, bare, or
/// `Base.`-qualified — a program defining `Float64(::MyIrrational{:tau})`
/// registers a bare "Float64" table, so its calls keep full dispatch;
/// dispatch fixture `dispatch/symbol_type_param_dispatch.jl`, Issue #633) and
/// no function binder may shadow it (a parameter or where-clause type param
/// named `Float64` makes the call a callable-variable call, not a
/// constructor call — note the pre-existing stack compiler bug Issue #10146:
/// it currently mis-routes that shadowed call to the builtin too; this gate
/// keeps the plan neutral so the eventual fix lands in one place). Body-local
/// rebinds are already covered upstream of the plan: `scan_lowerable` falls
/// back on any call through a locally bound name.
fn numeric_convert_gate(compiler: &CoreCompiler<'_>, func: &Function) -> plan::NumericConvertGate {
    let resolves_to_builtin = |name: &str| {
        !func.params.iter().any(|param| param.name == name)
            && !func.type_params.iter().any(|tp| tp.name == name)
            && compiler.module_owned_function_table_name(name).is_none()
            && !compiler.method_tables.contains_key(name)
            && !compiler.method_tables.contains_key(&format!("Base.{name}"))
    };
    plan::NumericConvertGate {
        int64: resolves_to_builtin("Int64"),
        float64: resolves_to_builtin("Float64"),
    }
}

// ─── Eligibility ────────────────────────────────────────────────────────────

fn eligibility_precheck(
    compiler: &CoreCompiler<'_>,
    func: &Function,
    return_type: &ValueType,
) -> Result<TailMode, LowerError> {
    if !compiler.captured_vars.is_empty() {
        return fallback("closure with captured variables");
    }
    if !func.kwparams.is_empty() {
        return fallback("keyword parameters");
    }
    // `&&`/`||` in statement position (result discarded): the SSA builder
    // evaluates both operands unconditionally, so DCE can remove the condition
    // guard while keeping the right operand's side effects — the short-circuit
    // semantics would be silently lost. Example: `x <= 0 && throw(Err)` at
    // statement level would always throw regardless of `x` (Issue #8832).
    if block_has_discarded_shortcircuit(&func.body) {
        return fallback("&&/|| in statement position (short-circuit semantics lost in SSA)");
    }
    let tail_mode = match func.body.stmts.last() {
        Some(Stmt::Return { .. }) => TailMode::Explicit,
        Some(Stmt::Expr { .. }) => TailMode::ImplicitExpr,
        // A statement that always exits through an explicit `return` on every
        // path (e.g. `if`/`else` where both branches return) is effectively an
        // explicit tail: the function never falls through to an implicit return.
        // The SSA builder places `Return` terminators for each arm; no join
        // block is reached and no implicit return value is needed.
        Some(stmt) if stmt_always_returns(stmt) => TailMode::Explicit,
        Some(_) => return fallback("unsupported tail statement (implicit default return)"),
        None => return fallback("empty body"),
    };
    // A function mixing explicit `return`s with an implicit tail expression
    // would need per-return implicit/explicit emission; when the declared
    // return type admits the legacy I64↔F64 tail conversion the two emissions
    // differ, so fall back (`docs/vm/SSA_IR.md`, Issue #8552).
    if tail_mode == TailMode::ImplicitExpr
        && matches!(return_type, ValueType::I64 | ValueType::F64)
        && block_contains_return(&func.body)
    {
        return fallback("mixed implicit tail and explicit returns with convertible return type");
    }
    Ok(tail_mode)
}

/// Returns `true` when every execution path through `stmt` ends in an
/// explicit `return`. The scan is structural (no flow-sensitive analysis):
/// only constructs that the SSA builder decomposes are checked; opaque
/// barriers return `false` and naturally trigger the "opaque barrier"
/// fallback via `scan_lowerable`.
fn stmt_always_returns(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::Block(block) => block.stmts.last().is_some_and(stmt_always_returns),
        // if/else where BOTH branches always return: no fall-through to the
        // join block, so the function's implicit tail is never reached.
        Stmt::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            then_branch.stmts.last().is_some_and(stmt_always_returns)
                && else_branch.stmts.last().is_some_and(stmt_always_returns)
        }
        // An `if` without `else`, a `while`, or any opaque barrier: cannot
        // prove always-returns statically.
        _ => false,
    }
}

fn block_contains_return(block: &Block) -> bool {
    block.stmts.iter().any(stmt_contains_return)
}

fn stmt_contains_return(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::Block(block) => block_contains_return(block),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_contains_return(then_branch)
                || else_branch.as_ref().is_some_and(block_contains_return)
        }
        Stmt::While { body, .. }
        | Stmt::For { body, .. }
        | Stmt::ForEach { body, .. }
        | Stmt::ForEachTuple { body, .. } => block_contains_return(body),
        Stmt::Try {
            try_block,
            catch_block,
            else_block,
            finally_block,
            ..
        } => {
            block_contains_return(try_block)
                || catch_block.as_ref().is_some_and(block_contains_return)
                || else_block.as_ref().is_some_and(block_contains_return)
                || finally_block.as_ref().is_some_and(block_contains_return)
        }
        Stmt::Timed { body, .. } | Stmt::TestSet { body, .. } => block_contains_return(body),
        // Nested function bodies have their own return scope.
        _ => false,
    }
}

/// Returns `true` when the function body contains `&&` or `||` in a statement
/// position where the result is discarded.
///
/// In SSA form the builder evaluates both operands unconditionally. When the
/// result is unused, DCE keeps any side-effectful right operand (e.g. a
/// `throw(...)`) while removing the condition check — breaking the short-circuit
/// guard. The fix is a per-function fallback before `build_function` (Issue #8832).
fn block_has_discarded_shortcircuit(block: &Block) -> bool {
    block.stmts.iter().any(stmt_has_discarded_shortcircuit)
}

fn stmt_has_discarded_shortcircuit(stmt: &Stmt) -> bool {
    match stmt {
        // Result of `&&`/`||` expression is discarded at statement level.
        Stmt::Expr { expr, .. } => expr_is_shortcircuit(expr),
        Stmt::Block(block) => block_has_discarded_shortcircuit(block),
        // SSA-decomposed control flow: recurse into arms.
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => {
            block_has_discarded_shortcircuit(then_branch)
                || else_branch
                    .as_ref()
                    .is_some_and(block_has_discarded_shortcircuit)
        }
        Stmt::While { body, .. } => block_has_discarded_shortcircuit(body),
        // Everything else is an opaque barrier in SSA construction and already
        // triggers the "opaque barrier construct" fallback via `scan_lowerable`;
        // no need to recurse into nested function bodies (they have their own
        // compilation call) or into constructs that never reach SSA lowering.
        _ => false,
    }
}

/// Returns `true` when `expr` is a top-level `&&` or `||` binary op.
fn expr_is_shortcircuit(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::BinaryOp {
            op: BinaryOp::And | BinaryOp::Or,
            ..
        }
    )
}

/// Post-optimization scan for constructs this slice cannot lower faithfully.
///
/// `written` is the locally-bound name set computed by `try_lower` (body
/// write names minus `global` declarations): a *call* through such a name
/// would resolve against a stale or missing binding (`docs/vm/SSA_IR.md`
/// closure-local limitation).
fn scan_lowerable(
    compiler: &CoreCompiler<'_>,
    written: &BTreeSet<String>,
    ssa: &SsaFunction,
) -> Result<(), LowerError> {
    if ssa.entry.0 != 0 {
        return fallback("entry block is not first");
    }

    for block in &ssa.blocks {
        for stmt in &block.stmts {
            match &stmt.op {
                SsaOp::Opaque { .. } | SsaOp::OpaqueStmt { .. } | SsaOp::BarrierReload { .. } => {
                    return fallback("opaque barrier construct");
                }
                SsaOp::Phi(phi) => {
                    if phi.values.iter().any(Option::is_none) {
                        // The legacy path raises `UndefVarError` with the
                        // source variable name; SSA lost that name.
                        return fallback("maybe-undefined phi");
                    }
                }
                SsaOp::Call {
                    module, function, ..
                } => {
                    if written.contains(function) {
                        return fallback("call through a locally bound name");
                    }
                    if let Some(module) = module {
                        if written.contains(module) {
                            return fallback("module-qualified call through a local binding");
                        }
                    }
                }
                SsaOp::LoadGlobal { name } => {
                    // Module values feed the legacy compiler's name-keyed
                    // alias tracking (`S = Statistics; S.mean`); a synthetic
                    // spill name would miss those tables.
                    if is_stdlib_module(name)
                        || compiler.module_functions.contains_key(name)
                        || compiler.module_exports.contains_key(name)
                    {
                        return fallback("module-valued global read");
                    }
                }
                SsaOp::Unary { .. } | SsaOp::Binary { .. } | SsaOp::Builtin { .. } => {}
                SsaOp::StoreGlobal { .. } => {}
            }
        }
    }
    Ok(())
}

// ─── Branch-type propagation ─────────────────────────────────────────────────

/// Compute, for each block, whether it is uniquely dominated by a `Branch`
/// block and, if so, which predecessor and polarity (then/else) it lives on.
///
/// `Some((pred_idx, is_then))` means block `B` has exactly one predecessor
/// `pred_idx` that terminates with a `TermPlan::Branch`, and `B` is the
/// `is_then ? then_target : else_target` of that branch.  For join blocks
/// (multiple predecessors) and blocks whose unique predecessor ends in a
/// `Jump`, the entry is `None`.
///
/// This information is used in [`emit_plan`] to apply branch-type narrowings
/// (Issue #9085): the same `isa`/`typeof` guard facts that the legacy compiler
/// overlays via `apply_then_narrowings`/`apply_else_narrowings` are now also
/// applied before emitting each uniquely-dominated SSA block, so that redundant
/// inner `isa` checks fold to `PushBool(true)` and arithmetic in
/// `typeof`-guarded branches specialises to typed instructions.
fn compute_block_narrowing_info(plan: &SharedFunctionPlan) -> Vec<Option<(usize, bool)>> {
    let n = plan.blocks().len();
    let mut pred_count = vec![0u32; n];
    // Tentative: may be overwritten when a second predecessor claims the slot.
    let mut entry: Vec<Option<(usize, bool)>> = vec![None; n];

    for (pred_idx, block_plan) in plan.blocks().iter().enumerate() {
        match block_plan.terminator() {
            SharedTermPlan::Jump { target, .. } => {
                pred_count[*target as usize] += 1;
                // Jump predecessors don't carry branch narrowings.
            }
            SharedTermPlan::Branch {
                then_target,
                else_target,
                ..
            } => {
                let ti = *then_target as usize;
                let ei = *else_target as usize;
                pred_count[ti] += 1;
                pred_count[ei] += 1;
                // Record tentative narrowing source; overwritten later if
                // pred_count ends up > 1.
                entry[ti] = Some((pred_idx, true));
                entry[ei] = Some((pred_idx, false));
            }
            SharedTermPlan::Return { .. } => {}
        }
    }

    // Clear entries for join blocks (pred_count != 1): narrowing is only sound
    // when a block has exactly one predecessor (which proved the guard).
    for (block_idx, e) in entry.iter_mut().enumerate() {
        if pred_count[block_idx] != 1 {
            *e = None;
        }
    }

    entry
}

// ─── Emission ───────────────────────────────────────────────────────────────

fn emit_plan(
    compiler: &mut CoreCompiler<'_>,
    func: &Function,
    ssa: &SsaFunction,
    plan: &SharedFunctionPlan,
    tail_mode: TailMode,
    return_type: ValueType,
) -> Result<(), LowerError> {
    // Mirror the `compile_function_body` prologue: `global x` declarations
    // apply to the whole scope regardless of position (Issues #5548, #5549).
    // Bodies with nested function definitions never reach emission (opaque
    // barrier fallback), so the sibling-closure prescan has nothing to do.
    if compiler.strict_undefined_check {
        collect_declared_globals(&func.body, &mut compiler.declared_globals);
    }

    // Slot-type the reconstructed statement stream with the same pre-scan the
    // legacy path runs on the source body, so synthetic `#ssaN` locals get
    // the identical widening / mixed-type treatment (Issue #6601 contract).
    let typing_stmts = collect_typing_stmts(plan);
    let protected: HashSet<String> = func
        .params
        .iter()
        .map(|p| p.name.clone())
        .chain(func.kwparams.iter().map(|k| k.name.clone()))
        .collect();
    collect_local_types_with_mixed_tracking(
        &typing_stmts,
        &mut compiler.locals,
        &protected,
        &compiler.shared_ctx.struct_table,
        &compiler.shared_ctx.global_types,
        &mut compiler.mixed_type_vars,
    );

    // Branch-type narrowing info for the SSA pipeline (Issue #9085).
    // For each block, records whether it is uniquely dominated by a Branch
    // block and, if so, which predecessor and polarity to apply narrowings from.
    let block_narrowing = compute_block_narrowing_info(plan);

    let base = compiler.here();
    let mut starts: Vec<usize> = Vec::with_capacity(plan.blocks().len());
    // Forward jumps to patch once every block offset is known.
    let mut pending: Vec<(usize, u32)> = Vec::new();

    for (block_idx, block_plan) in plan.blocks().iter().enumerate() {
        starts.push(compiler.here());

        // Apply branch-type narrowings for uniquely dominated blocks (Issue
        // #9085).  When a block is reached exclusively via one arm of a Branch
        // whose condition is an `isa`/`typeof` guard, overlay the concrete type
        // onto `compiler.locals` so that inner `isa` re-checks fold to
        // `PushBool(true)` and arithmetic specialises to typed instructions —
        // the same optimizations the legacy path performs via `apply_then_narrowings`
        // / `apply_else_narrowings` in `compile_if_stmt`.
        let narrowing_restore = match block_narrowing[block_idx] {
            Some((pred_idx, is_then)) => {
                // Safety: pred_idx < plan.blocks.len() by construction.
                match plan.blocks()[pred_idx].terminator() {
                    SharedTermPlan::Branch { cond, .. } => {
                        if is_then {
                            compiler.apply_then_narrowings(cond)
                        } else {
                            compiler.apply_else_narrowings(cond)
                        }
                    }
                    // Should not happen (we only record Branch predecessors),
                    // but be defensive.
                    _ => vec![],
                }
            }
            None => vec![],
        };

        for root in block_plan.roots() {
            emit_root(compiler, root)?;
        }
        let next_block = (block_idx + 1) as u32;
        match block_plan.terminator() {
            SharedTermPlan::Return { expr } => {
                emit_return(compiler, expr.as_ref(), tail_mode, &return_type)?;
            }
            SharedTermPlan::Jump { target, copies } => {
                emit_copies(compiler, copies)?;
                if *target != next_block {
                    pending.push((compiler.here(), *target));
                    compiler.emit(Instr::Jump(usize::MAX));
                }
            }
            SharedTermPlan::Branch {
                cond,
                then_target,
                else_target,
                then_copies,
                else_copies,
            } => {
                let false_jumps = compiler.compile_condition_false_jumps(cond)?;
                emit_copies(compiler, then_copies)?;
                // The else trampoline (when present) sits on the fallthrough
                // path, so the then edge must jump over it explicitly.
                if *then_target != next_block || !else_copies.is_empty() {
                    pending.push((compiler.here(), *then_target));
                    compiler.emit(Instr::Jump(usize::MAX));
                }
                if else_copies.is_empty() {
                    for at in false_jumps {
                        pending.push((at, *else_target));
                    }
                } else {
                    let trampoline = compiler.here();
                    for at in false_jumps {
                        compiler.patch_jump(at, trampoline);
                    }
                    emit_copies(compiler, else_copies)?;
                    pending.push((compiler.here(), *else_target));
                    compiler.emit(Instr::Jump(usize::MAX));
                }
            }
        }

        // Restore branch-type narrowings after this block's roots and
        // terminator have been emitted (Issue #9085).  The restore is a no-op
        // when no narrowings were applied (empty vec).
        compiler.restore_then_narrowings(narrowing_restore);
    }

    for (at, target) in pending {
        let Some(&start) = starts.get(target as usize) else {
            return fallback("jump to unplanned block");
        };
        compiler.patch_jump(at, start);
    }

    debug_assert!(
        compiler.here() >= base,
        "SSA lowering must only append code (Issue #8552)"
    );
    let _ = ssa;
    Ok(())
}

fn collect_typing_stmts(plan: &SharedFunctionPlan) -> Vec<Stmt> {
    let mut stmts = Vec::new();
    for block in plan.blocks() {
        for root in block.roots() {
            match root {
                SharedRootPlan::Assign { name, expr, span } => stmts.push(Stmt::Assign {
                    var: name.clone(),
                    value: expr.clone(),
                    span: *span,
                }),
                SharedRootPlan::Discard { expr, span } => stmts.push(Stmt::Expr {
                    expr: expr.clone(),
                    span: *span,
                }),
            }
        }
        let copies: &[SharedCopyPlan] = match block.terminator() {
            SharedTermPlan::Jump { copies, .. } => copies,
            SharedTermPlan::Branch { .. } | SharedTermPlan::Return { .. } => &[],
        };
        for copy in copies {
            stmts.push(copy_to_stmt(copy));
        }
        if let SharedTermPlan::Branch {
            then_copies,
            else_copies,
            ..
        } = block.terminator()
        {
            for copy in then_copies.iter().chain(else_copies) {
                stmts.push(copy_to_stmt(copy));
            }
        }
    }
    stmts
}

fn copy_to_stmt(copy: &SharedCopyPlan) -> Stmt {
    Stmt::Assign {
        var: copy.name.clone(),
        value: copy.expr.clone(),
        span: copy.span,
    }
}

fn emit_root(compiler: &mut CoreCompiler<'_>, root: &SharedRootPlan) -> Result<(), LowerError> {
    let stmt = match root {
        SharedRootPlan::Assign { name, expr, span } => Stmt::Assign {
            var: name.clone(),
            value: expr.clone(),
            span: *span,
        },
        SharedRootPlan::Discard { expr, span } => Stmt::Expr {
            expr: expr.clone(),
            span: *span,
        },
    };
    compiler.compile_stmt(&stmt)?;
    Ok(())
}

fn emit_copies(
    compiler: &mut CoreCompiler<'_>,
    copies: &[SharedCopyPlan],
) -> Result<(), LowerError> {
    for copy in copies {
        let stmt = copy_to_stmt(copy);
        compiler.compile_stmt(&stmt)?;
    }
    Ok(())
}

/// Emit a `Return` terminator with the same instruction choice as the legacy
/// `compile_function_body` tail handling (explicit `return` vs. trailing
/// implicit expression — the latter converts I64↔F64 toward the declared
/// return type).
fn emit_return(
    compiler: &mut CoreCompiler<'_>,
    expr: Option<&Expr>,
    tail_mode: TailMode,
    return_type: &ValueType,
) -> Result<(), LowerError> {
    let Some(expr) = expr else {
        compiler.emit(Instr::ReturnNothing);
        return Ok(());
    };
    let actual_ty = compiler.compile_expr(expr)?;
    match tail_mode {
        TailMode::Explicit => {
            if should_return_as_expected_type(&actual_ty, return_type) {
                compiler.emit_return_for_type(return_type.clone());
            } else {
                compiler.emit_return_for_type(actual_ty);
            }
        }
        TailMode::ImplicitExpr => {
            if actual_ty != *return_type && can_convert_type(actual_ty.clone(), return_type.clone())
            {
                compiler.emit_type_conversion(actual_ty, return_type.clone());
                compiler.emit_return_for_type(return_type.clone());
            } else if should_return_as_expected_type(&actual_ty, return_type) {
                compiler.emit_return_for_type(return_type.clone());
            } else {
                compiler.emit_return_for_type(actual_ty);
            }
        }
    }
    Ok(())
}
