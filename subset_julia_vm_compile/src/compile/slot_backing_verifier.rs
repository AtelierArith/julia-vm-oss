//! Debug/test-mode invariant: every local read is dominated by a store.
//!
//! Prevention follow-up for Issue #10820 (root cause: #10819/#7556).
//! `CoreCompiler::store_local` used to treat `ValueType::Nothing` as a
//! compile-time singleton and elide its storage (`Pop` only) while still
//! marking the local initialized. When a later assignment widened that local
//! to an `Any`-backed slot in only one control-flow branch, the merged code's
//! `LoadSlot`/`LoadAny` was reachable via the non-assigning path with no
//! backing value, raising `UndefVarError` even though the Julia source had
//! executed `x = nothing`. The fix (#10819) materializes every `Nothing`
//! assignment; this module is the durable invariant check the same class of
//! bug will trip going forward, whenever *any* storage-elision optimization is
//! introduced.
//!
//! # What this checks
//!
//! Over a single function's compiled instruction stream, treat every
//! Load/Store instruction pair keyed by the same local (either the
//! pre-slotization name-keyed family — `LoadAny`/`StoreAny`, `LoadI64`/
//! `StoreI64`, ... — or the post-slotization index-keyed family —
//! `LoadSlot`/`StoreSlot` and the typed `LoadSlotXxx`/`StoreSlotXxx` family)
//! as a definite-assignment dataflow problem: a read is valid only if a store
//! to the same local reaches it on *every* predecessor path from the function
//! entry (dominance). This is the forward "must" dataflow — meet =
//! intersection, transfer = union with the block's own stores — which is
//! exactly equivalent to dominance for "does every path from entry to this
//! use pass through a definition".
//!
//! `LoadGlobalAny`/`StoreGlobalAny` (module frame-0 bindings declared
//! `global x`) are intentionally excluded: a global's backing value can
//! legitimately predate the current call (set by a prior top-level statement
//! or another function), so "no store dominates this read *within this
//! function's own CFG*" is not a defect for globals the way it is for locals.
//!
//! # Scope note
//!
//! This is wired as a **test-only** pass (unit tests in this module plus,
//! optionally, ad hoc calls from other test code) — never from the
//! production compile/VM pipeline — per the #10820 brief: a dominance
//! verifier is valuable for catching the *next* storage-elision regression
//! early, but is not something every compile should pay for at runtime.

use std::collections::HashSet;

use crate::bytecode::Instr;
use crate::compile::cfg::{BasicBlock, ControlFlowGraph};

/// Identifies the local a Load/Store instruction touches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocalRef {
    /// Pre-slotization / dynamically-typed local, keyed by variable name.
    Name(String),
    /// Post-slotization local, keyed by frame slot index.
    Slot(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Access {
    Load,
    Store,
}

/// A `Load` of `local` at `instr` (in basic block `block`) is reachable
/// without a dominating `Store` on every predecessor path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotBackingViolation {
    pub instr: usize,
    pub block: usize,
    pub local: LocalRef,
}

/// Classify a single instruction as a Load or Store of a `LocalRef`, or
/// `None` if it does not touch a local subject to this invariant.
fn classify(instr: &Instr) -> Option<(LocalRef, Access)> {
    use Instr::*;
    let name_pair = match instr {
        LoadStr(n) => Some((n, Access::Load)),
        StoreStr(n) => Some((n, Access::Store)),
        LoadI64(n) => Some((n, Access::Load)),
        StoreI64(n) => Some((n, Access::Store)),
        LoadF64(n) => Some((n, Access::Load)),
        StoreF64(n) => Some((n, Access::Store)),
        LoadF32(n) => Some((n, Access::Load)),
        StoreF32(n) => Some((n, Access::Store)),
        LoadF16(n) => Some((n, Access::Load)),
        StoreF16(n) => Some((n, Access::Store)),
        LoadBool(n) => Some((n, Access::Load)),
        StoreBool(n) => Some((n, Access::Store)),
        LoadAny(n) | ProbeRuntimeBinding(n) => Some((n, Access::Load)),
        StoreAny(n) => Some((n, Access::Store)),
        LoadArray(n) => Some((n, Access::Load)),
        StoreArray(n) => Some((n, Access::Store)),
        LoadRange(n) => Some((n, Access::Load)),
        StoreRange(n) => Some((n, Access::Store)),
        LoadStruct(n) => Some((n, Access::Load)),
        StoreStruct(n) => Some((n, Access::Store)),
        LoadRng(n) => Some((n, Access::Load)),
        StoreRng(n) => Some((n, Access::Store)),
        LoadTuple(n) => Some((n, Access::Load)),
        StoreTuple(n) => Some((n, Access::Store)),
        LoadNamedTuple(n) => Some((n, Access::Load)),
        StoreNamedTuple(n) => Some((n, Access::Store)),
        LoadDict(n) => Some((n, Access::Load)),
        StoreDict(n) => Some((n, Access::Store)),
        LoadSet(n) => Some((n, Access::Load)),
        StoreSet(n) => Some((n, Access::Store)),
        LoadMemory(n) => Some((n, Access::Load)),
        StoreMemory(n) => Some((n, Access::Store)),
        _ => None,
    };
    if let Some((name, access)) = name_pair {
        return Some((LocalRef::Name(name.clone()), access));
    }

    let slot_pair = match instr {
        LoadSlot(i) => Some((*i, Access::Load)),
        StoreSlot(i) => Some((*i, Access::Store)),
        LoadSlotI64(i) => Some((*i, Access::Load)),
        StoreSlotI64(i) => Some((*i, Access::Store)),
        LoadSlotF64(i) => Some((*i, Access::Load)),
        StoreSlotF64(i) => Some((*i, Access::Store)),
        LoadSlotBool(i) => Some((*i, Access::Load)),
        StoreSlotBool(i) => Some((*i, Access::Store)),
        LoadSlotF32(i) => Some((*i, Access::Load)),
        StoreSlotF32(i) => Some((*i, Access::Store)),
        LoadSlotF16(i) => Some((*i, Access::Load)),
        StoreSlotF16(i) => Some((*i, Access::Store)),
        LoadSlotStr(i) => Some((*i, Access::Load)),
        StoreSlotStr(i) => Some((*i, Access::Store)),
        LoadSlotChar(i) => Some((*i, Access::Load)),
        StoreSlotChar(i) => Some((*i, Access::Store)),
        LoadSlotNarrowInt(i) => Some((*i, Access::Load)),
        StoreSlotNarrowInt(i) => Some((*i, Access::Store)),
        LoadSlotNothing(i) => Some((*i, Access::Load)),
        StoreSlotNothing(i) => Some((*i, Access::Store)),
        LoadSlotArray(i) => Some((*i, Access::Load)),
        StoreSlotArray(i) => Some((*i, Access::Store)),
        LoadSlotTuple(i) => Some((*i, Access::Load)),
        StoreSlotTuple(i) => Some((*i, Access::Store)),
        LoadSlotNamedTuple(i) => Some((*i, Access::Load)),
        StoreSlotNamedTuple(i) => Some((*i, Access::Store)),
        LoadSlotDict(i) => Some((*i, Access::Load)),
        StoreSlotDict(i) => Some((*i, Access::Store)),
        LoadSlotSet(i) => Some((*i, Access::Load)),
        StoreSlotSet(i) => Some((*i, Access::Store)),
        LoadSlotStruct(i) => Some((*i, Access::Load)),
        StoreSlotStruct(i) => Some((*i, Access::Store)),
        LoadSlotRange(i) => Some((*i, Access::Load)),
        StoreSlotRange(i) => Some((*i, Access::Store)),
        LoadSlotRng(i) => Some((*i, Access::Load)),
        StoreSlotRng(i) => Some((*i, Access::Store)),
        LoadSlotGenerator(i) => Some((*i, Access::Load)),
        StoreSlotGenerator(i) => Some((*i, Access::Store)),
        LoadSlotSymbol(i) => Some((*i, Access::Load)),
        StoreSlotSymbol(i) => Some((*i, Access::Store)),
        _ => None,
    };
    slot_pair.map(|(idx, access)| (LocalRef::Slot(idx), access))
}

/// Verify that every local read in `code` (a single function's compiled
/// instruction stream, entry at instruction 0) is dominated by a store on
/// every reachable predecessor path, given the locals already backed on
/// entry (typically the function's own parameters).
///
/// Returns one [`SlotBackingViolation`] per offending read, in instruction
/// order. An empty result means the invariant holds.
pub fn verify_slot_backing(
    code: &[Instr],
    initially_backed: &HashSet<LocalRef>,
) -> Vec<SlotBackingViolation> {
    verify_slot_backing_range(code, 0, 0..code.len(), initially_backed)
}

/// Same dominance check as [`verify_slot_backing`], generalized to a REAL
/// `CompiledProgram.code` array holding many functions concatenated
/// together: `entry_instr` is the instruction where the function under test
/// begins (its `FunctionInfo::code_start`), and only reads whose instruction
/// index falls in `report_range` (that function's `[code_start, code_end)`)
/// are reported.
///
/// Building the CFG over `code` in full (rather than a per-function
/// subslice) is required: a compiled program's `Jump`/`JumpIfZero`/
/// `PushHandler` targets are absolute indices into the *whole* shared array
/// (see `relocate_jumps` in `pipeline_ctx.rs`, which shifts every such target
/// when code is prefixed), so slicing first would silently corrupt every
/// target that used to point outside the slice. Functions are naturally
/// disconnected components of that CFG — every function body ends in a
/// `Return*` instruction, which contributes no CFG successor — so treating
/// one function's `code_start` as the dataflow's entry and restricting
/// reports to its own instruction range is sound: other functions' blocks
/// are visited by the fixpoint (harmlessly, as unreachable-from-this-entry)
/// but never contribute to or receive a report.
pub fn verify_slot_backing_range(
    code: &[Instr],
    entry_instr: usize,
    report_range: std::ops::Range<usize>,
    initially_backed: &HashSet<LocalRef>,
) -> Vec<SlotBackingViolation> {
    let cfg = ControlFlowGraph::build(code);
    if cfg.blocks.is_empty() {
        return Vec::new();
    }
    let Some(entry_block) = cfg.block_for_instr(entry_instr) else {
        return Vec::new();
    };

    // Restrict everything below to the entry block's own reachable component
    // (forward reachability over `successors`). This matters in two ways
    // when `code` is a real, whole-program array holding many functions:
    //   - Correctness: without it, a block with no PROCESSED predecessor
    //     (e.g. any other function's entry, or genuine dead code after an
    //     unconditional return/jump within this function) would fall through
    //     `meet_predecessors` to the empty set, flagging every read inside it
    //     as a violation even though it can never execute. A read that can
    //     never execute cannot violate this invariant.
    //   - Performance: without it, the fixpoint and the final scan touch
    //     every block of the whole program (e.g. all of Base) for every
    //     function checked, instead of just that one function's own blocks.
    let reachable = reachable_block_ids(&cfg, entry_block);

    // Universal starting set for the fixpoint: every local this function
    // touches at all. Blocks other than the entry start "optimistically"
    // backed by everything and only shrink as real predecessor constraints
    // propagate in — a standard forward "must" dataflow (analogous to
    // available-expressions), guaranteed to converge because stores only add
    // facts (each iteration's sets are non-increasing, bounded below by the
    // empty set).
    let all_locals: HashSet<LocalRef> = code
        .iter()
        .filter_map(classify)
        .map(|(local, _)| local)
        .collect();

    let mut out: Vec<HashSet<LocalRef>> = vec![all_locals.clone(); cfg.blocks.len()];
    // The entry block's only guaranteed-backed set on first entry is
    // `initially_backed` — deliberately NOT intersected with any back-edge
    // predecessor (e.g. a `while true` loop that jumps back to its own
    // header): the worst case we must catch is precisely the very first pass
    // through, before any loop-body store has ever executed.
    out[entry_block] = compute_block_out(code, &cfg.blocks[entry_block], initially_backed);

    let mut changed = true;
    while changed {
        changed = false;
        for block in &cfg.blocks {
            if block.id == entry_block || !reachable.contains(&block.id) {
                continue;
            }
            let in_set = meet_predecessors(&out, &block.predecessors, &reachable);
            let new_out = compute_block_out(code, block, &in_set);
            if new_out != out[block.id] {
                out[block.id] = new_out;
                changed = true;
            }
        }
    }

    let mut violations = Vec::new();
    for block in &cfg.blocks {
        if !reachable.contains(&block.id) {
            continue;
        }
        // A block wholly outside the reporting window belongs to a
        // different function (or padding) and is never reported on. Every
        // function body ends in a `Return*`, which forces a leader (and
        // hence a block boundary) at the next instruction, so a real
        // `[code_start, code_end)` window never straddles this check.
        if block.end <= report_range.start || block.start >= report_range.end {
            continue;
        }
        let mut backed = if block.id == entry_block {
            initially_backed.clone()
        } else {
            meet_predecessors(&out, &block.predecessors, &reachable)
        };
        for (idx, instr) in code.iter().enumerate().take(block.end).skip(block.start) {
            if let Some((local, access)) = classify(instr) {
                match access {
                    Access::Load => {
                        if report_range.contains(&idx) && !backed.contains(&local) {
                            violations.push(SlotBackingViolation {
                                instr: idx,
                                block: block.id,
                                local,
                            });
                        }
                    }
                    Access::Store => {
                        backed.insert(local);
                    }
                }
            }
        }
    }
    violations
}

/// Every block id reachable from `entry_block` by following `successors`
/// forward (a plain BFS over the CFG), including `entry_block` itself.
fn reachable_block_ids(cfg: &ControlFlowGraph, entry_block: usize) -> HashSet<usize> {
    let mut seen = HashSet::from([entry_block]);
    let mut stack = vec![entry_block];
    while let Some(id) = stack.pop() {
        let Some(block) = cfg.blocks.get(id) else {
            continue;
        };
        for &succ in &block.successors {
            if seen.insert(succ) {
                stack.push(succ);
            }
        }
    }
    seen
}

fn compute_block_out(
    code: &[Instr],
    block: &BasicBlock,
    in_set: &HashSet<LocalRef>,
) -> HashSet<LocalRef> {
    let mut backed = in_set.clone();
    for instr in code.iter().take(block.end).skip(block.start) {
        if let Some((local, Access::Store)) = classify(instr) {
            backed.insert(local);
        }
    }
    backed
}

/// Intersection of every predecessor's OUT set, considering only
/// predecessors reachable from the entry (an edge from an unreachable block —
/// dead code, or simply a different function sharing this whole-program CFG —
/// can never actually be traversed, so it must not constrain what is
/// guaranteed backed). A block with no reachable predecessor at all is itself
/// unreachable from the entry in this model (this also covers a `catch`/
/// `finally` block whose try region cannot dominate it — see the
/// `PushHandler` edges in `cfg.rs`) — conservatively, nothing is guaranteed
/// backed there.
fn meet_predecessors(
    out: &[HashSet<LocalRef>],
    preds: &[usize],
    reachable: &HashSet<usize>,
) -> HashSet<LocalRef> {
    let mut iter = preds
        .iter()
        .filter(|p| reachable.contains(p))
        .map(|&p| &out[p]);
    match iter.next() {
        None => HashSet::new(),
        Some(first) => {
            let mut acc = first.clone();
            for other in iter {
                acc.retain(|item| other.contains(item));
            }
            acc
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn name(s: &str) -> LocalRef {
        LocalRef::Name(s.to_string())
    }

    fn backed(names: &[&str]) -> HashSet<LocalRef> {
        names.iter().map(|n| name(n)).collect()
    }

    /// Negative self-test: this is the exact pre-fix shape of Issue #10819 —
    /// `result = nothing` compiled to a bare `Pop` (no store at all), then a
    /// single `if` branch widens `result` to a real value. The merge point's
    /// `LoadAny("result")` is reachable via the non-assigning branch with no
    /// backing store, and the verifier must flag it for that reason.
    #[test]
    fn flags_branch_widen_without_predecessor_store_10819_shape() {
        let code = vec![
            Instr::PushNothing,                         // 0
            Instr::Pop, // 1: `result = nothing` — NOT stored (the bug)
            Instr::LoadBool("take_branch".to_string()), // 2
            Instr::JumpIfZero(6), // 3
            Instr::LoadI64("value".to_string()), // 4
            Instr::StoreAny("result".to_string()), // 5: only this branch stores
            Instr::LoadAny("result".to_string()), // 6: merge point read
            Instr::ReturnAny, // 7
        ];

        let violations = verify_slot_backing(&code, &backed(&["take_branch", "value"]));

        assert_eq!(
            violations,
            vec![SlotBackingViolation {
                instr: 6,
                block: 2,
                local: name("result"),
            }]
        );
    }

    /// Positive counterpart: the landed #10819 fix materializes the `Nothing`
    /// assignment (`StoreAny` instead of bare `Pop`), so both predecessors of
    /// the merge block now store `result` and the read is properly dominated.
    #[test]
    fn passes_branch_widen_with_materialized_nothing_store() {
        let code = vec![
            Instr::PushNothing,                         // 0
            Instr::StoreAny("result".to_string()),      // 1: fixed — now stored
            Instr::LoadBool("take_branch".to_string()), // 2
            Instr::JumpIfZero(6),                       // 3
            Instr::LoadI64("value".to_string()),        // 4
            Instr::StoreAny("result".to_string()),      // 5
            Instr::LoadAny("result".to_string()),       // 6
            Instr::ReturnAny,                           // 7
        ];

        let violations = verify_slot_backing(&code, &backed(&["take_branch", "value"]));
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    /// try/catch extension of the #10819 matrix (positive shape): a store
    /// strictly before `PushHandler` dominates the catch block, because no
    /// exception can occur before the handler is even installed.
    #[test]
    fn passes_try_catch_read_of_pre_try_store() {
        let code = vec![
            Instr::PushI64(1),                 // 0
            Instr::StoreI64("x".to_string()),  // 1: x set before try
            Instr::PushHandler(Some(6), None), // 2
            Instr::PushI64(2),                 // 3
            Instr::StoreI64("y".to_string()),  // 4: only set inside try
            Instr::Jump(7),                    // 5: skip catch on normal completion
            Instr::LoadI64("x".to_string()),   // 6: catch reads x
            Instr::ReturnI64,                  // 7
        ];

        let violations = verify_slot_backing(&code, &HashSet::new());
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    /// try/catch extension (negative shape): `y` is stored only inside the
    /// protected try body, strictly after `PushHandler`. An exception raised
    /// before that store can still transfer to the catch block, so the read
    /// of `y` there must NOT be considered dominated.
    #[test]
    fn flags_try_catch_read_of_try_body_only_store() {
        let code = vec![
            Instr::PushHandler(Some(4), None), // 0
            Instr::PushI64(1),                 // 1
            Instr::StoreI64("y".to_string()),  // 2: only set inside try, after the handler
            Instr::Jump(5),                    // 3
            Instr::LoadI64("y".to_string()),   // 4: catch reads y
            Instr::ReturnI64,                  // 5
        ];

        let violations = verify_slot_backing(&code, &HashSet::new());
        assert_eq!(
            violations,
            vec![SlotBackingViolation {
                instr: 4,
                block: 2,
                local: LocalRef::Name("y".to_string()),
            }]
        );
    }

    /// Zero-iteration loop extension of the #10819 matrix (negative shape):
    /// `x` is stored only inside a `while` body that may run zero times, then
    /// read after the loop. The header's first (possibly only) evaluation
    /// must not be treated as backed by a body that never ran.
    #[test]
    fn flags_zero_iteration_loop_widen() {
        let code = vec![
            Instr::LoadBool("cond".to_string()), // 0: loop header
            Instr::JumpIfZero(5),                // 1: exit if false
            Instr::PushI64(1),                   // 2
            Instr::StoreI64("x".to_string()),    // 3: only set inside the loop body
            Instr::Jump(0),                      // 4: back-edge to header
            Instr::LoadI64("x".to_string()),     // 5: after the loop
            Instr::ReturnI64,                    // 6
        ];

        let violations = verify_slot_backing(&code, &backed(&["cond"]));
        assert_eq!(
            violations,
            vec![SlotBackingViolation {
                instr: 5,
                block: 2,
                local: LocalRef::Name("x".to_string()),
            }]
        );
    }

    /// Zero-iteration loop extension (positive shape): `x` is stored before
    /// the loop as well, so it is backed on entry regardless of how many
    /// times (including zero) the loop body runs.
    #[test]
    fn passes_zero_iteration_loop_with_pre_loop_store() {
        let code = vec![
            Instr::PushI64(0),                   // 0
            Instr::StoreI64("x".to_string()),    // 1: set before the loop
            Instr::LoadBool("cond".to_string()), // 2: loop header
            Instr::JumpIfZero(7),                // 3
            Instr::PushI64(1),                   // 4
            Instr::StoreI64("x".to_string()),    // 5
            Instr::Jump(2),                      // 6: back-edge to header
            Instr::LoadI64("x".to_string()),     // 7: after the loop
            Instr::ReturnI64,                    // 8
        ];

        let violations = verify_slot_backing(&code, &backed(&["cond"]));
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    /// A parameter is backed from function entry without any explicit store.
    #[test]
    fn parameters_are_backed_without_explicit_store() {
        let code = vec![Instr::LoadI64("param".to_string()), Instr::ReturnI64];
        let violations = verify_slot_backing(&code, &backed(&["param"]));
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
    }

    /// A read with no store anywhere on any path is flagged even in a
    /// straight-line (branch-free) function body.
    #[test]
    fn flags_straight_line_read_without_any_store() {
        let code = vec![Instr::LoadAny("ghost".to_string()), Instr::ReturnAny];
        let violations = verify_slot_backing(&code, &HashSet::new());
        assert_eq!(
            violations,
            vec![SlotBackingViolation {
                instr: 0,
                block: 0,
                local: name("ghost"),
            }]
        );
    }

    // === Real-pipeline tests ===
    //
    // Everything above exercises `verify_slot_backing` against hand-built
    // `Instr` sequences chosen to model specific shapes precisely. That
    // proves the dataflow logic is correct, but not that `classify()` and
    // `initially_backed` line up with what `CoreCompiler` actually emits for
    // a real function once the full pipeline (parse -> lower -> compile ->
    // peephole -> slotize) has run. The tests below close that gap: they
    // compile the ACTUAL #10820 fixture shapes through
    // `crate::pipeline::parse_and_lower` + `crate::compile::cache::compile_with_cache`
    // and run the verifier over the real, post-slotization `CompiledProgram.code`.

    /// Cold-compiling Base/prelude the first time in a test process (as
    /// happens here when this module's tests run in isolation, with no
    /// earlier test in the same binary to warm the cache) recurses deep
    /// enough in an unoptimized debug build to overflow the default thread
    /// stack. Match the rest of the suite's convention
    /// (`subset_julia_vm/tests/fixture_tests.rs::FIXTURE_TEST_STACK_SIZE` and
    /// friends) and run on a thread with a larger stack.
    const REAL_PIPELINE_TEST_STACK_SIZE: usize = 16 * 1024 * 1024;

    fn compiled_functions_have_no_violations(src: &'static str, function_names: &'static [&str]) {
        std::thread::Builder::new()
            .stack_size(REAL_PIPELINE_TEST_STACK_SIZE)
            .spawn(move || compiled_functions_have_no_violations_inner(src, function_names))
            .expect("spawn real-pipeline test thread")
            .join()
            .expect("real-pipeline test thread panicked");
    }

    fn compiled_functions_have_no_violations_inner(src: &str, function_names: &[&str]) {
        let program = crate::pipeline::parse_and_lower(src).expect("pipeline error");
        let compiled =
            crate::compile::cache::compile_with_cache(&program).expect("compile must succeed");

        for &fn_name in function_names {
            let func_info = compiled
                .functions
                .iter()
                .rev()
                .find(|f| f.name == fn_name)
                .unwrap_or_else(|| panic!("compiled function `{fn_name}` not found"));
            assert!(
                func_info.code_start < func_info.code_end,
                "function `{fn_name}` has an empty code range"
            );

            let mut initially_backed: HashSet<LocalRef> = func_info
                .param_slots
                .iter()
                .map(|&slot| LocalRef::Slot(slot))
                .collect();
            initially_backed.extend(func_info.kwparams.iter().map(|kw| LocalRef::Slot(kw.slot)));

            let violations = verify_slot_backing_range(
                &compiled.code,
                func_info.code_start,
                func_info.code_start..func_info.code_end,
                &initially_backed,
            );
            assert!(
                violations.is_empty(),
                "function `{fn_name}` has slot-backing violations in its REAL compiled \
                 bytecode: {violations:?}"
            );
        }
    }

    /// The exact functions from the #10820 fixture
    /// (`nothing_initialized_trycatch_loop_widen_10820.jl`), run through the
    /// full pipeline: every local read (now `LoadSlot`/typed `LoadSlotXxx`
    /// post-slotization) must be dominated by a store in the REAL compiled
    /// bytecode, not just in a hand-built stand-in.
    #[test]
    fn real_compiled_10820_functions_satisfy_slot_backing() {
        let src = r#"
function trycatch_widen_10820(should_throw, value)
    result = nothing
    try
        if should_throw
            error("boom")
        end
        result = value
    catch
    end
    result
end

function trycatch_catch_widen_10820(should_throw, value)
    result = nothing
    try
        if should_throw
            error("boom")
        end
    catch
        result = value
    end
    result
end

function loop_zero_widen_10820(iterations, value)
    result = nothing
    for _ in 1:iterations
        result = value
    end
    result
end

function while_zero_widen_10820(take_branch, value)
    result = nothing
    i = 0
    while take_branch && i == 0
        result = value
        i += 1
    end
    result
end
"#;
        compiled_functions_have_no_violations(
            src,
            &[
                "trycatch_widen_10820",
                "trycatch_catch_widen_10820",
                "loop_zero_widen_10820",
                "while_zero_widen_10820",
            ],
        );
    }

    /// The original #10819 shapes (plain `if`-branch widening, including
    /// flat destructuring), run through the full pipeline the same way.
    #[test]
    fn real_compiled_10819_functions_satisfy_slot_backing() {
        let src = r#"
function destructure_branch_10819(take_branch, pair)
    result = nothing
    if take_branch
        ignored, result = pair
    end
    result
end

function scalar_branch_10819(take_branch, value)
    result = nothing
    if take_branch
        result = value
    end
    result
end
"#;
        compiled_functions_have_no_violations(
            src,
            &["destructure_branch_10819", "scalar_branch_10819"],
        );
    }
}
