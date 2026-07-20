//! Predecoded executable blocks for hot VM loops.
//!
//! This layer is derived from the canonical `Instr` bytecode in
//! `CompiledProgram`. It is intentionally conservative: a block only runs when
//! the bytecode shape and runtime slot values match the typed fast path exactly;
//! otherwise execution falls back to the regular interpreter at the same IP.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::intrinsics::Intrinsic;
use crate::rng::RngLike;

use super::value::{new_array_ref, ArrayData, ArrayElementType, ArrayRef, ArrayValue, StrRef};
use super::{profiler, FunctionInfo, Instr, Value, ValueType, Vm};

const NO_BLOCK: u32 = u32::MAX;
pub(crate) const NO_EXECUTABLE_IP: usize = usize::MAX;

#[derive(Debug, Default)]
pub(crate) struct ExecutableProgram {
    block_by_ip: Vec<u32>,
    blocks: Vec<ExecutableBlock>,
    block_ips: Vec<usize>,
}

impl ExecutableProgram {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn from_bytecode(
        code: &[Instr],
        functions: &[Rc<FunctionInfo>],
        base_function_count: usize,
    ) -> Self {
        let mut executable = Self {
            block_by_ip: vec![NO_BLOCK; code.len()],
            blocks: Vec::new(),
            block_ips: Vec::new(),
        };
        for function in functions {
            if function.code_start >= function.code_end || function.code_end > code.len() {
                continue;
            }
            executable.predecode_range(
                code,
                functions,
                base_function_count,
                function.code_start,
                function.code_end,
            );
        }
        executable.block_ips.sort_unstable();
        executable.block_ips.dedup();
        executable
    }

    pub(crate) fn append_bytecode(
        &mut self,
        code: &[Instr],
        functions: &[Rc<FunctionInfo>],
        base_function_count: usize,
        start: usize,
        end: usize,
    ) {
        if end > self.block_by_ip.len() {
            self.block_by_ip.resize(end, NO_BLOCK);
        }
        self.predecode_range(code, functions, base_function_count, start, end);
        self.block_ips.sort_unstable();
        self.block_ips.dedup();
    }

    fn predecode_range(
        &mut self,
        code: &[Instr],
        functions: &[Rc<FunctionInfo>],
        base_function_count: usize,
        start: usize,
        end: usize,
    ) {
        let mut ip = start;
        while ip < end {
            // Run the recognizer pipeline (Issue #6829): the first registered
            // recognizer that matches this `ip` produces the executable block.
            if let Some(block) = LOOP_RECOGNIZERS
                .iter()
                .find_map(|recognize| recognize(code, functions, ip, end, base_function_count))
            {
                self.insert_block(ip, block);
            }
            ip += 1;
        }
    }

    fn insert_block(&mut self, ip: usize, block: ExecutableBlock) {
        if ip >= self.block_by_ip.len() || self.block_by_ip[ip] != NO_BLOCK {
            return;
        }
        let block_idx = self.blocks.len();
        if block_idx >= NO_BLOCK as usize {
            return;
        }
        self.blocks.push(block);
        self.block_by_ip[ip] = block_idx as u32;
        self.block_ips.push(ip);
    }

    #[inline]
    fn block_at(&self, ip: usize) -> Option<&ExecutableBlock> {
        let block_idx = *self.block_by_ip.get(ip)?;
        if block_idx == NO_BLOCK {
            return None;
        }
        self.blocks.get(block_idx as usize)
    }

    #[inline]
    pub(crate) fn next_ip_from(&self, ip: usize) -> usize {
        let idx = self.block_ips.partition_point(|&block_ip| block_ip < ip);
        self.block_ips.get(idx).copied().unwrap_or(NO_EXECUTABLE_IP)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.blocks.len()
    }

    #[cfg(test)]
    fn has_typed_loop(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block, ExecutableBlock::Typed(_)))
    }
}

#[derive(Debug, Clone)]
enum ExecutableBlock {
    Typed(TypedLoopBlock),
}

/// A predecode *recognizer*: the pattern-matcher and validator stage of the
/// hot-region pipeline (Issue #6829). Given the bytecode and a candidate region
/// header `ip` (bounded by the function/region `end`), it returns a
/// pre-validated, ready-to-execute [`ExecutableBlock`] — a typed IR that the
/// generic per-kind executor runs — when the region matches its shape, or `None`
/// to let the next recognizer try the same `ip`.
///
/// Recognizers run only at program install (`from_bytecode` / `append_bytecode`),
/// never on the execution hot path, so the registry is a plain ordered list:
/// teaching the VM a new optimized shape means appending one recognizer to
/// [`LOOP_RECOGNIZERS`] instead of editing the predecode control flow. The
/// executor side is already generic per block kind — `TypedLoopBlock` in
/// particular carries a `TypedLoopOp` IR rather than hand-coded logic, so loops
/// that fit the typed-loop shape need no new executor at all.
type LoopRecognizer =
    fn(&[Instr], &[Rc<FunctionInfo>], usize, usize, usize) -> Option<ExecutableBlock>;

fn recognize_typed_loop(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    ip: usize,
    end: usize,
    base_function_count: usize,
) -> Option<ExecutableBlock> {
    try_predecode_typed_loop(code, functions, ip, end, base_function_count)
        .map(ExecutableBlock::Typed)
}

/// Ordered predecode recognizer registry (Issue #6829). The first recognizer to
/// match a given `ip` wins. The Euclidean-modulo special case
/// (`while b != 0; (a, b) = (b, a % b); end`) was retired in favor of the
/// general typed-loop path (Issue #10310): `TypedLoopOp` already covers
/// `ModI64`/`LoadModI64Slot`, so the coprime-gcd loop is recognized by
/// `recognize_typed_loop` like any other typed integer loop.
const LOOP_RECOGNIZERS: &[LoopRecognizer] = &[recognize_typed_loop];

#[derive(Debug, Clone)]
pub(crate) enum ExecutableBlockResult {
    NotExecuted,
    Continue,
    Exit(Value),
}

#[derive(Debug, Clone)]
struct TypedLoopBlock {
    exit_ip: usize,
    array_slots: Vec<TypedLoopSlot>,
    f64_slots: Vec<TypedLoopSlot>,
    i64_slots: Vec<TypedLoopSlot>,
    // Issue #10559: `String` local slots read/written inside the loop
    // (`LoadSlotStr`/`StoreSlotStr`). `Value::Str` is `Rc<str>` (Issue #8630),
    // so loading/storing a slot is a refcount bump, not a deep copy — the only
    // allocation a string typed-loop op performs is the one inherent to the
    // operation itself (e.g. `ConcatStr`'s byte concatenation).
    str_slots: Vec<TypedLoopSlot>,
    /// Compile-time string literal pool for `TypedLoopOp::PushStrConst` (Issue
    /// #10559). Interned once at predecode; referenced by index so
    /// `TypedLoopOp` stays `Copy`.
    str_consts: Vec<StrRef>,
    ops: Vec<TypedLoopOp>,
    /// Frame-less predecoded I64 callees referenced by `TypedLoopOp::CallI64Function`
    /// (Issue #10309). Callee locals live in a separate block, so they do not
    /// count against the loop's own slot caps.
    i64_callees: Vec<I64FunctionBlock>,
    /// Frame-less predecoded F64 callees referenced by `TypedLoopOp::CallF64Function`.
    f64_callees: Vec<F64FunctionBlock>,
    /// Runtime-resolved I64 specialize callees referenced by
    /// `TypedLoopOp::CallSpecializeI64Function` (Issue #10439). Each entry is a
    /// `(spec_func_index, arg_count)` pair naming a `CallSpecializeI64Slots`
    /// site inside the loop. Unlike `i64_callees`, the callee body is NOT
    /// predecoded here: an untyped callee's I64 body is a *runtime*
    /// specialization (appended lazily on first call), so it may not exist when
    /// this loop is predecoded. `execute_typed_loop_block` resolves each entry
    /// against the live `specialization_i64_cache` immediately before running
    /// the typed ops; a miss (callee not yet specialized, or not I64-decodable)
    /// bails the whole block to the generic interpreter, which is the source of
    /// truth and populates the cache for the next entry.
    specialize_callees: Vec<(usize, usize)>,
    /// Float64 mirror of `specialize_callees` (Issue #10491): each entry names
    /// a `CallSpecializeF64Slots` site inside the loop, resolved per block
    /// execution against the live `specialization_f64_cache` by
    /// `TypedLoopOp::CallSpecializeF64Function`.
    specialize_f64_callees: Vec<(usize, usize)>,
    /// Issue #10567 (round 2): `spec_func_index` for each narrow
    /// `f(complex_arg, i64_arg)` `CallSpecialize` site inlined by
    /// `TypedLoopOp::CallSpecializeComplexI64Function`, resolved per block
    /// execution against the live `specialization_mixed_cache`. Arity is
    /// always 2 (fixed by the recognizer's narrow shape), so unlike the I64/
    /// F64 mirrors there is no need to also record an argument count.
    specialize_complex_i64_callees: Vec<usize>,
    /// Issue #10542: frame-less predecoded mixed-type callees (pure-I64
    /// params/return, internal F64 locals) referenced by
    /// `TypedLoopOp::CallTypedI64Function`.
    typed_i64_callees: Vec<TypedScalarFunctionBlock>,
    /// Issue #10542: frame-less predecoded mixed-type callees (pure-F64
    /// params/return, internal I64 locals — e.g. an F64 math helper with an
    /// I64 loop counter) referenced by `TypedLoopOp::CallTypedF64Function`.
    typed_f64_callees: Vec<TypedScalarFunctionBlock>,
    /// Issue #10565: `certify_typed_ops_trusted(&ops)`, computed ONCE here at
    /// predecode instead of on every block entry. Hot kernels re-enter their
    /// typed loop constantly — Mandelbrot re-enters its escape loop once per
    /// pixel (~2.25M times for the 1500x1500 benchmark) — and an O(ops) scan
    /// per entry measured as a ~8% REGRESSION there, swallowing the win the
    /// unchecked executor buys inside the loop. Only the #10516 inliner's
    /// rewritten stream still needs a per-entry certification (it is a
    /// different op list, and it is built per entry); everything else reads
    /// this flag.
    ///
    /// NOTE (#10566(c) merge): the former `arrays_read_only` field (#10104) is
    /// retired — `IndexStore*` now writes a per-local transactional buffer, so
    /// store loops no longer need the read-only snapshot-bridge restriction.
    ops_trusted: bool,
}

/// One local slot of a frame-less scalar function block (Issue #10427). The
/// slot/param model is identical for every scalar element type, so it is not
/// parameterized. `F64FunctionSlot` / `I64FunctionSlot` alias this.
#[derive(Debug, Clone)]
pub struct ScalarFunctionSlot {
    pub slot: usize,
    pub param_index: Option<usize>,
}

/// A frame-less predecoded scalar function block (Issue #10427), generic over
/// the operand element type `S` (`i64` or `f64`). Replaces the previously
/// duplicated `I64FunctionBlock` / `F64FunctionBlock`, which are now aliases.
#[derive(Debug, Clone)]
pub struct ScalarFunctionBlock<S> {
    pub slots: Vec<ScalarFunctionSlot>,
    pub ops: Vec<ScalarFunctionOp<S>>,
    pub callees: Vec<ScalarFunctionBlock<S>>,
}

pub(crate) type I64FunctionBlock = ScalarFunctionBlock<i64>;
pub type F64FunctionBlock = ScalarFunctionBlock<f64>;
pub type F64FunctionSlot = ScalarFunctionSlot;

/// A resolved `CallSpecializeF64Slots` callee body (Issue #10491). Unlike the
/// I64 mirror, an all-`F64`-argument untyped helper very often carries an I64
/// loop counter (`k = 0; while k < 4; ...`), which a pure
/// [`F64FunctionBlock`] cannot represent — those bodies resolve to the
/// mixed-type frame-less [`TypedScalarFunctionBlock`] (Issue #9693) instead.
/// The pure-F64 form stays as the cheaper fast case.
#[derive(Clone)]
pub(crate) enum ResolvedSpecF64Callee {
    F64(F64FunctionBlock),
    Typed(TypedScalarFunctionBlock),
}

#[derive(Debug, Clone)]
struct TypedLoopSlot {
    slot: usize,
    live_in: bool,
    written_in_loop: bool,
    /// Issue #10566(c): true for an ARRAY slot that is the target of at least
    /// one `IndexStoreTyped` in this block (set by `mark_array_slot_stored`).
    /// Unused for f64/i64/str slots. A stored array local is resolved at
    /// block entry through a private transactional buffer (`ArrayWriteOrigin`)
    /// instead of the frame's shared array, and committed back to its origin
    /// only on a non-`Bail` outcome — never written back via the generic
    /// per-local frame loop `execute_typed_loop_block` uses for read-only
    /// locals.
    stored: bool,
}

#[derive(Debug, Clone, Copy)]
enum TypedLoopOp {
    LoadArraySlot(usize),
    IndexStoreI64,
    IndexStoreF64,
    // Issue #10566(b): the runtime counterpart of an elided identity-rebind
    // `StoreSlotArray(s)` (`LoadSlotArray(s); ...; IndexStoreTyped(1);
    // StoreSlotArray(s)`, same slot `s`). Eliding the store means NO op
    // re-binds the typed local — `s`'s buffer already holds the update — but
    // the array VALUE that `IndexStoreTyped` pushed back onto the array mini
    // stack still needs to come off it (`array_stack` is a real runtime
    // `Vec`, not just the predecoder's linear depth count); skipping that
    // pop here leaked one entry per loop iteration and overflowed
    // `TYPED_LOOP_STACK_CAP` on any loop with more than a few iterations.
    // Pops one array and discards it; no other effect.
    DropArray,
    // Issue #10104: typed 1-D array element reads (`x[i]`) inside a typed loop.
    // Pop an i64 index and a guarded 1-D numeric array off the typed stacks and
    // push the loaded element onto the matching typed stack. The element type is
    // fixed at predecode from the array param's declared `Vector{T}` type; the
    // executor re-checks the runtime element type and bails on any mismatch or
    // out-of-bounds access so the generic interpreter reproduces the exact
    // `BoundsError` / dispatch semantics. Originally only emitted for
    // read-only-array loops (no `IndexStore*`/`RandF64`); as of Issue
    // #10566(c) it may also appear alongside `IndexStore*` in the SAME block
    // (`y[i] = x[i] + 1`-shaped map/copy loops), because `IndexStore*` writes
    // a discardable transactional buffer rather than the array heap in place
    // — a bail here still re-runs the whole block from the header without
    // double-applying any observable side effect (`RandF64` is the one op
    // that still rules that out).
    IndexLoadF64,
    IndexLoadI64,
    PushF64(f64),
    RandF64,
    DupF64,
    LoadF64Slot(usize),
    StoreF64Slot(usize),
    LoadSquareF64Slot(usize),
    LoadAddF64Slot(usize),
    LoadSubF64Slot(usize),
    LoadMulF64Slot(usize),
    // Issue #8183: fused `load slot; /` (numerator on the stack, divisor in the
    // slot) and the unfused `/` and unary `-` for Float64 ODE / map bodies.
    LoadDivF64Slot(usize),
    // Issue #9126: fused `slot[dst] = slot[lhs] + slot[rhs]` and the I64→F64
    // converting rhs form. Operands are typed-loop local indices (dst, lhs, rhs).
    AddF64Slots(usize, usize, usize),
    AddF64I64Slots(usize, usize, usize),
    AddF64,
    SubF64,
    MulF64,
    DivF64,
    NegF64,
    PushI64(i64),
    DupI64,
    ToF64,
    LoadI64Slot(usize),
    LoadI64SlotToF64(usize),
    StoreI64Slot(usize),
    AddI64,
    SubI64,
    MulI64,
    // Issue #8183: integer modulo (`%`) and the fused `load slot; <op>` forms
    // used by LCG-style iterated maps (e.g. `1103515245 * seed`).
    ModI64,
    LoadAddI64Slot(usize),
    LoadSubI64Slot(usize),
    LoadMulI64Slot(usize),
    LoadModI64Slot(usize),
    IncI64Slot(usize),
    DecI64Slot(usize),
    AddConstI64Slot(usize, i64),
    // Issue #9654: fused `push slot[local] + delta` (the escape-return `k - 1`
    // shape) and an early `return <i64>` from inside the loop. The return
    // routes through the shared frame-return protocol, so loops whose only
    // exits are early returns (Mandelbrot escape kernels) stay on the native
    // path instead of falling back to per-instruction interpretation.
    LoadAddConstI64Slot(usize, i64),
    ReturnI64,
    // Issue #10309: frame-less call to a predecoded I64 function from inside a
    // typed loop. The callee is stored in `TypedLoopBlock::i64_callees`; arguments
    // are popped from the i64 stack and the return value is pushed back.
    CallI64Function(usize, usize),
    // Issue #10309 follow-up: frame-less call to a predecoded F64 function from inside a typed loop.
    CallF64Function(usize, usize),
    // Issue #10542: frame-less call to a callee whose declared params/return
    // are pure I64 but whose BODY mixes types (e.g. an F64-param callee is
    // symmetric via `CallTypedF64Function` below; this variant is the I64
    // mirror — pure-I64 params/return with an internal F64 local). The pure
    // `try_predecode_i64_function` decoder rejects such bodies outright,
    // which used to reject the WHOLE caller loop; falling back to the
    // mixed-type `TypedScalarFunctionBlock` (Issue #9693) lets the caller
    // loop stay native. Callee stored in `TypedLoopBlock::typed_i64_callees`;
    // arguments are popped from the i64 stack (all-I64 by the same shape gate
    // as `CallI64Function`) and the I64 return value is pushed back.
    CallTypedI64Function(usize, usize),
    // Issue #10542: F64 mirror of `CallTypedI64Function` — a callee with
    // pure-F64 declared params/return whose body mixes an I64 local (the
    // extremely common "F64 math + I64 loop counter" helper shape). Callee
    // stored in `TypedLoopBlock::typed_f64_callees`; arguments are popped
    // from the f64 stack and the F64 return value is pushed back.
    CallTypedF64Function(usize, usize),
    // Issue #10439: frame-less call to an *untyped* callee reached through a
    // `CallSpecializeI64Slots` site inside a typed loop. Unlike `CallI64Function`,
    // the callee body is not stored in the block: the first operand indexes a
    // runtime-resolved `I64FunctionBlock` slice built per block execution from
    // the live specialization cache (see `TypedLoopBlock::specialize_callees`).
    // The second operand is the argument count. If the slice has no entry for
    // the index (callee not yet specialized / not I64-decodable) the op bails,
    // and the block re-runs on the generic interpreter.
    CallSpecializeI64Function(usize, usize),
    // Issue #10491: Float64 mirror of `CallSpecializeI64Function` — a
    // frame-less call to an *untyped* callee reached through a
    // `CallSpecializeF64Slots` site inside a typed loop. The first operand
    // indexes the per-execution `resolved_specialize_f64` slice built from the
    // live `specialization_f64_cache`; the second is the argument count. A
    // resolution or execution miss bails the block to the generic interpreter.
    CallSpecializeF64Function(usize, usize),
    // Issue #10516: emitted ONLY by the block-entry callee inliner
    // (`try_inline_i64_callees_into_typed_ops`), never by the recognizer.
    // Marks an inlined callee's non-param local as uninitialized before the
    // spliced body runs, so a load-before-store path inside the inlined body
    // bails exactly like the frame-less callee's own `local_init` guard would
    // (instead of silently reading a stale value from a previous iteration).
    UninitI64Slot(usize),
    // Issue #9693: frame-less typed scalar function calls. A `ComplexF64`
    // argument is bound as a `(re, im)` pair in `TypedOpsState::complex_params`;
    // these ops execute the SROA param-hoist preamble shape
    // (`LoadSlotStruct(param); GetField(k); StoreSlotF64(d)`) against it.
    /// push the bound complex param onto the complex mini-stack
    PushComplexParam(usize),
    /// pop a complex, push its field (0 = re, 1 = im) as f64
    ComplexFieldF64(usize),
    /// fused `f64[dst] = complex_params[param].{re|im}` (predecode fusion)
    StoreComplexParamFieldF64(usize, usize, usize),
    /// Issue #10567 (round 2): pop two f64 mini-stack values (`im` then `re`,
    /// LIFO — matching the source constructor's `re` then `im` push order,
    /// same convention as `ReturnStructF64x2`) and push the `(re, im)` pair
    /// onto the complex mini-stack, WITHOUT allocating a boxed
    /// `Complex{Float64}` struct. Recognizes the loop-mode idiom
    /// `NewParametricStruct("Complex", 2)` immediately feeding a mixed-arg
    /// `CallSpecialize` site (e.g. `mandel_point(cr + ci*im, maxiter)`) —
    /// narrower than `PushComplexParam`, which only reads an already-bound
    /// function parameter.
    MaterializeComplexF64,
    // Issue #10567 (round 2): frame-less call to a runtime-specialized
    // untyped callee reached through a genuinely mixed-type `CallSpecialize`
    // site inside a typed loop — specifically the narrow two-argument shape
    // `f(complex_arg, i64_arg)` where the complex argument was just
    // materialized by `MaterializeComplexF64` (e.g.
    // `mandel_point(cr + ci*im, maxiter)`). Pops one i64 off the i64 mini
    // stack and one complex off the complex mini stack (in that
    // declaration-positional order — `TypedFunctionParamBinding` param 0 is
    // the complex, param 1 is the i64), resolves the callee against
    // `TypedLoopBlock::specialize_complex_i64_callees` (mirroring
    // `CallSpecializeF64Function`'s resolve-from-live-cache contract), and
    // pushes the returned i64. Any other mixed-arg shape (more/fewer
    // arguments, different types or order) is NOT recognized by this op and
    // keeps falling through to the generic interpreter.
    CallSpecializeComplexI64Function(usize),
    /// pop f64; return it from the enclosing frame / function block
    ReturnF64,
    /// Issue #10645: fused `NewStruct(type_id, 2); ReturnStruct` in function
    /// mode — pop two f64 mini-stack values (`im` then `re`, LIFO) and return
    /// a freshly built 2-`F64`-field struct (e.g. `Complex{Float64}(r, i)`)
    /// tagged with `type_id`. `type_id` is the struct id the compiler already
    /// resolved statically into `NewStruct` itself (a concrete constructor
    /// call like `Complex{Float64}(r, i)`, as opposed to the dynamic
    /// `Complex{typeof(r)}(r, i)` form), so this op carries no reference to
    /// `struct_defs`; the instance's `Rc<str>` name is looked up once per
    /// call from the `type_id -> name` registry (Issue #9198 S4) that the VM
    /// keeps in sync with `struct_defs`. Not Complex-specific: sound for any
    /// 2-field struct whose fields are declared in `(re, im)`-like order,
    /// same as `NewStruct`'s own runtime semantics.
    ReturnStructF64x2(usize),
    // Issue #9654: predecode-time op fusion (`fuse_typed_loop_ops`) — general
    // superinstructions over the op list, each stack-effect-equivalent to the
    // window it replaces. Cuts the per-iteration dispatch count of dense
    // Float64 loop bodies (Mandelbrot ~23 → ~12 ops/iteration).
    /// push `f64[a] * f64[b]`
    PushMulF64Slots(usize, usize),
    /// push `f64[a]² + f64[b]²` (norm/escape guards, Monte-Carlo circles)
    PushSumSquaresF64Slots(usize, usize),
    /// push `f64[a]² − f64[b]²` (complex square real part)
    PushDiffSquaresF64Slots(usize, usize),
    /// `f64[dst] = f64[src]`
    CopyF64Slots(usize, usize),
    /// `i64[dst] = i64[src]`
    CopyI64Slots(usize, usize),
    /// pop lhs; `f64[dst] = lhs + f64[src]`
    AddF64SlotStore(usize, usize),
    /// Issue #10532: fused Complex{Float64} recurrence `z = z*z + c`.
    /// Updates the unboxed slot pair `(z_re, z_im)` in place from the
    /// addend pair `(c_re, c_im)`, matching the pure-Julia method body for
    /// `Base.:*` followed by `Base.:+`.
    ComplexMulAddAssign {
        z_re: usize,
        z_im: usize,
        c_re: usize,
        c_im: usize,
    },
    /// pop lhs; jump when NOT `lhs <rel> const` (ordered)
    JumpIfNotF64Const(F64Relation, f64, TypedLoopTarget),
    CmpI64(I64Relation),
    CmpF64(F64Relation),
    // Issue #10559: String slot reads/writes + accumulation inside a typed
    // loop. `Value::Str` is `Rc<str>` (Issue #8630), so `LoadStrSlot`/
    // `StoreStrSlot`/`PushStrConst` are refcount-bump cheap — only
    // `ConcatStr`'s byte concatenation allocates, matching the cost the
    // generic interpreter's `StringConcat`/`ConcatStrings` pays for the same
    // `*` expression. `EqStr` is allocation-free (byte comparison).
    /// push a copy of the string local (Rc clone)
    LoadStrSlot(usize),
    /// pop string, store into the string local
    StoreStrSlot(usize),
    /// push a compile-time string literal (index into `TypedLoopBlock::str_consts`)
    PushStrConst(usize),
    /// pop N strings off the string mini-stack (bottom-to-top order), concatenate,
    /// push the result — mirrors `Instr::StringConcat`/`Instr::ConcatStrings` for
    /// the all-`String`-operand case (recognizer-guaranteed; the interpreter's
    /// general show/format path is unreachable here since every value that ever
    /// reaches the string mini-stack was produced by `LoadStrSlot`/`PushStrConst`/
    /// `ConcatStr` itself).
    ConcatStr(usize),
    /// pop 2 strings, push their byte equality onto the bool mini-stack
    EqStr,
    /// pop 1 string, push `length(s)` (Unicode codepoint count — Julia's
    /// `String` is byte-indexed UTF-8, so this is `chars().count()`, NOT
    /// `str.len()`/byte length; see `Instr::CallBuiltin(BuiltinId::Length, 1)`
    /// on `Value::Str` in `builtins_collections.rs`, which this mirrors).
    StrLen,
    JumpIfZero(TypedLoopTarget),
    JumpIfI64(I64Relation, TypedLoopTarget),
    JumpIfI64Slots(usize, usize, I64Relation, TypedLoopTarget),
    AddConstI64SlotAndJumpIfLe(usize, i64, usize, TypedLoopTarget),
    JumpIfF64(F64Relation, TypedLoopTarget),
    JumpIfNotF64(F64Relation, TypedLoopTarget),
    Jump(TypedLoopTarget),
}

/// Transactionality facts used when deciding whether a typed loop may safely
/// restart from its header on `TypedOpsOutcome::Bail` (Issue #10814).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TypedLoopOpEffects {
    bail_capable: bool,
    out_of_buffer_effect: bool,
}

impl TypedLoopOpEffects {
    const NONE: Self = Self {
        bail_capable: false,
        out_of_buffer_effect: false,
    };
    const BAIL_CAPABLE: Self = Self {
        bail_capable: true,
        out_of_buffer_effect: false,
    };
    const OUT_OF_BUFFER_EFFECT: Self = Self {
        bail_capable: false,
        out_of_buffer_effect: true,
    };
}

impl TypedLoopOp {
    /// Classify every typed-loop op in one exhaustive match. Intentionally no
    /// wildcard arm: adding a variant without deciding both transactionality
    /// facts must fail compilation instead of silently weakening the guard.
    fn effects(&self) -> TypedLoopOpEffects {
        match self {
            Self::ModI64
            | Self::LoadModI64Slot(_)
            | Self::IndexLoadF64
            | Self::IndexLoadI64
            // Issue #10566(c): stores can bail on bounds/type data, but mutate
            // only a discardable ArrayWriteOrigin buffer until clean commit.
            | Self::IndexStoreF64
            | Self::IndexStoreI64
            | Self::CallI64Function(..)
            | Self::CallF64Function(..)
            | Self::CallSpecializeI64Function(..)
            | Self::CallSpecializeF64Function(..)
            | Self::CallTypedI64Function(..)
            | Self::CallTypedF64Function(..)
            | Self::CallSpecializeComplexI64Function(..)
            // Issue #10559: operand length can make concatenation bail for
            // overflow or the VM memory budget before generic OOM reporting.
            | Self::ConcatStr(_) => TypedLoopOpEffects::BAIL_CAPABLE,
            Self::RandF64 => TypedLoopOpEffects::OUT_OF_BUFFER_EFFECT,
            Self::LoadArraySlot(_)
            | Self::DropArray
            | Self::PushF64(_)
            | Self::DupF64
            | Self::LoadF64Slot(_)
            | Self::StoreF64Slot(_)
            | Self::LoadSquareF64Slot(_)
            | Self::LoadAddF64Slot(_)
            | Self::LoadSubF64Slot(_)
            | Self::LoadMulF64Slot(_)
            | Self::LoadDivF64Slot(_)
            | Self::AddF64Slots(..)
            | Self::AddF64I64Slots(..)
            | Self::AddF64
            | Self::SubF64
            | Self::MulF64
            | Self::DivF64
            | Self::NegF64
            | Self::PushI64(_)
            | Self::DupI64
            | Self::ToF64
            | Self::LoadI64Slot(_)
            | Self::LoadI64SlotToF64(_)
            | Self::StoreI64Slot(_)
            | Self::AddI64
            | Self::SubI64
            | Self::MulI64
            | Self::LoadAddI64Slot(_)
            | Self::LoadSubI64Slot(_)
            | Self::LoadMulI64Slot(_)
            | Self::IncI64Slot(_)
            | Self::DecI64Slot(_)
            | Self::AddConstI64Slot(..)
            | Self::LoadAddConstI64Slot(..)
            | Self::ReturnI64
            // Issue #10536: ordinary uninitialized-local loads are not generic
            // restart points after an effect. run_typed_ops_core returns
            // UndefLocal directly, so they deliberately carry no bail fact.
            | Self::UninitI64Slot(_)
            | Self::PushComplexParam(_)
            | Self::ComplexFieldF64(_)
            | Self::StoreComplexParamFieldF64(..)
            | Self::MaterializeComplexF64
            | Self::ReturnF64
            | Self::ReturnStructF64x2(_)
            | Self::PushMulF64Slots(..)
            | Self::PushSumSquaresF64Slots(..)
            | Self::PushDiffSquaresF64Slots(..)
            | Self::CopyF64Slots(..)
            | Self::CopyI64Slots(..)
            | Self::AddF64SlotStore(..)
            | Self::ComplexMulAddAssign { .. }
            | Self::JumpIfNotF64Const(..)
            | Self::CmpI64(_)
            | Self::CmpF64(_)
            | Self::LoadStrSlot(_)
            | Self::StoreStrSlot(_)
            | Self::PushStrConst(_)
            | Self::EqStr
            | Self::StrLen
            | Self::JumpIfZero(_)
            | Self::JumpIfI64(..)
            | Self::JumpIfI64Slots(..)
            | Self::AddConstI64SlotAndJumpIfLe(..)
            | Self::JumpIfF64(..)
            | Self::JumpIfNotF64(..)
            | Self::Jump(_) => TypedLoopOpEffects::NONE,
        }
    }
}

/// One op of a frame-less scalar function block (Issue #10427), generic over
/// the operand element type `S` (`i64` or `f64`). This is the union of the ops
/// the i64 and f64 predecoders emit; each predecoder emits only the subset
/// meaningful for its type (e.g. `Div`/`Neg` are f64-only, `Rem`/`IncSlot`/…
/// are i64-only). Replaces the previously duplicated `I64FunctionOp` /
/// `F64FunctionOp`, which are now aliases.
#[derive(Debug, Clone, Copy)]
pub enum ScalarFunctionOp<S> {
    Push(S),
    LoadSlot(usize),
    StoreSlot(usize),
    Add,
    Sub,
    Mul,
    /// f64-only (`/`).
    Div,
    /// f64-only (unary `-`).
    Neg,
    Abs,
    /// i64-only (`%`).
    Rem,
    LoadAddSlot(usize),
    LoadSubSlot(usize),
    LoadMulSlot(usize),
    /// f64-only fused `pop; / slot`.
    LoadDivSlot(usize),
    /// i64-only fused `pop; % slot`.
    LoadRemSlot(usize),
    /// i64-only `slot += pop` (loop induction).
    IncSlot(usize),
    /// i64-only `slot -= pop` (loop induction).
    DecSlot(usize),
    /// i64-only `slot += const`.
    AddConstSlot(usize, S),
    /// i64-only fused `slot += const; jump if slot <= slot[stop]`.
    AddConstSlotAndJumpIfLe(usize, S, usize, usize),
    Call(usize, usize),
    Cmp(ScalarRelation),
    JumpIfZero(usize),
    JumpIf(ScalarRelation, usize),
    JumpIfSlots(usize, usize, ScalarRelation, usize),
    Jump(usize),
    Return,
}

pub type F64FunctionOp = ScalarFunctionOp<f64>;

/// Local execution state for the shared typed-op core (Issues #9654/#9693).
#[derive(Clone)]
struct TypedOpsState {
    array_locals: Vec<Option<ArrayRef>>,
    array_init: Vec<bool>,
    /// Issue #10104: array locals sourced from a read-only snapshot of a general
    /// `Vector{T}` struct (not the frame's ExprArgs carrier). These must never be
    /// written back to the frame.
    array_snapshot_only: Vec<bool>,
    f64_locals: [f64; TYPED_LOOP_SLOT_CAP],
    i64_locals: [i64; TYPED_LOOP_SLOT_CAP],
    f64_init: [bool; TYPED_LOOP_SLOT_CAP],
    i64_init: [bool; TYPED_LOOP_SLOT_CAP],
    /// Bound `(re, im)` pairs of ComplexF64 call arguments (function blocks).
    complex_params: [(f64, f64); TYPED_FUNCTION_COMPLEX_PARAM_CAP],
    // Issue #10559: String locals. `Value::Str` is `Rc<str>` (Issue #8630), so
    // unlike `array_locals` there is no snapshot-vs-live distinction to track —
    // a slot clone is always a cheap refcount bump either way.
    str_locals: Vec<Option<StrRef>>,
    str_init: Vec<bool>,
}

impl TypedOpsState {
    fn new(array_count: usize, str_count: usize) -> Self {
        Self {
            array_locals: vec![None; array_count],
            array_init: vec![false; array_count],
            array_snapshot_only: vec![false; array_count],
            f64_locals: [0.0; TYPED_LOOP_SLOT_CAP],
            i64_locals: [0_i64; TYPED_LOOP_SLOT_CAP],
            f64_init: [false; TYPED_LOOP_SLOT_CAP],
            i64_init: [false; TYPED_LOOP_SLOT_CAP],
            complex_params: [(0.0, 0.0); TYPED_FUNCTION_COMPLEX_PARAM_CAP],
            str_locals: vec![None; str_count],
            str_init: vec![false; str_count],
        }
    }

    /// Reset this state to match `other` without allocating new Vec storage.
    /// Used by the broadcast typed kernel to avoid a per-element allocation
    /// that the normal direct-call path does not pay.
    fn reset_from(&mut self, other: &Self) {
        self.array_locals.clone_from(&other.array_locals);
        self.array_init.clone_from(&other.array_init);
        self.array_snapshot_only
            .clone_from(&other.array_snapshot_only);
        self.f64_locals.copy_from_slice(&other.f64_locals);
        self.i64_locals.copy_from_slice(&other.i64_locals);
        self.f64_init.copy_from_slice(&other.f64_init);
        self.i64_init.copy_from_slice(&other.i64_init);
        self.complex_params.copy_from_slice(&other.complex_params);
        self.str_locals.clone_from(&other.str_locals);
        self.str_init.clone_from(&other.str_init);
    }
}

/// Outcome of the shared typed-op core.
enum TypedOpsOutcome {
    /// Fell past the last op or took an `Exit` target (loop mode resumes the
    /// interpreter at `exit_ip`; function mode treats this as a bail — a
    /// well-formed function block always leaves through a `Return*` op).
    Completed,
    /// A `Return*` op fired with this value.
    EarlyReturn(Value),
    /// A guard failed; the caller must discard the local state and fall back.
    Bail,
    /// Issue #10536: a load of an uninitialized loop local (`Load*Slot`/...
    /// with `!*_init[local]`) fired AFTER the out-of-buffer `RandF64` effect
    /// had already been applied this entry. Unlike `Bail`,
    /// the caller must NOT discard state and re-run the block from the
    /// header — the generic re-run would re-apply the already-applied side
    /// effect (e.g. re-draw `rand()`), shifting the observable RNG stream.
    /// The caller raises the matching `UndefVarError` for `local` directly,
    /// exactly as the generic interpreter would have on this path, while
    /// keeping every other local write already committed this entry. Only
    /// ever produced by the frame-backed loop path: the frame-less scalar
    /// function/broadcast blocks that also call `run_typed_ops_core` never
    /// contain `RandF64` (and exclude array ops separately), so
    /// `side_effect_applied` stays `false` there and this variant is
    /// unreachable for them.
    UndefLocal { kind: TypedLocalKind, local: usize },
}

/// Which typed-loop-local table `TypedOpsOutcome::UndefLocal::local` indexes
/// into — needed to resolve the local back to its frame slot (`TypedLoopBlock::
/// f64_slots`/`i64_slots`/`array_slots`) for the `UndefVarError` variable name
/// (Issue #10536).
#[derive(Debug, Clone, Copy)]
enum TypedLocalKind {
    F64,
    I64,
    Array,
    // Issue #10559 x #10536: String loop locals. An uninit String load can fire
    // after an already-applied side effect (`RandF64`; `IndexStore*` no longer
    // counts as of Issue #10566(c) — its writes land in a discardable
    // transactional buffer, not the frame, so a bail after one is safe to
    // re-run generically) — the #10504 mixing guard does NOT reject that
    // combination, because `LoadStrSlot` is not bail-capable — so it must
    // resolve to a real `UndefVarError` rather than re-running the block and
    // double-applying the side effect.
    Str,
}

/// Issue #10566(c): where a typed-loop STORED array local's private
/// transactional buffer (`TypedOpsState::array_locals[local]`, resolved at
/// block entry and mutated in place by `IndexStoreI64`/`IndexStoreF64`) is
/// committed back on a non-`Bail` outcome (clean completion, `EarlyReturn`, or
/// a propagated `VmError` — see `commit_typed_loop_array_buffers`). A `Bail`
/// discards the buffer without ever consulting the origin, leaving the
/// original array untouched for the generic interpreter's re-run.
#[derive(Debug, Clone)]
enum ArrayWriteOrigin {
    /// An ExprArgs-native carrier: this is the SAME `Rc<RefCell<ArrayValue>>`
    /// the frame slot already holds (a clone of the frame's own handle, not a
    /// fresh allocation). Commit overwrites its `RefCell` contents in place
    /// from the buffer, preserving the Rc's identity so any other live alias
    /// of this same array observes the write.
    Native(ArrayRef),
    /// A MemoryRef-backed `Vector{T}` struct, named by its `StructRef` index.
    /// Commit re-resolves the struct's backing `Memory` at write-back time
    /// (`write_back_numeric_vector_buffer`) and writes elementwise, in place —
    /// never reallocating, so any other alias of that `Memory` observes the
    /// write.
    StructVector(usize),
}

/// Issue #10566(c): BACKING-STORAGE identity of a live-in array local, used
/// ONLY for the block-entry aliasing check — never as the actual write-back
/// target (`ArrayWriteOrigin` is). Two live-in locals (at least one STORED)
/// whose identities OVERLAP mean the same underlying elements reached the
/// block through two different locals; the transactional-buffer model does not
/// observe cross-local writes mid-block the way a real alias would (and two
/// stored buffers over one storage would clobber each other at commit), so the
/// whole block rejects and falls back to the generic interpreter's per-element
/// dispatch, which handles aliasing correctly.
///
/// This is the STORAGE `Rc` plus the element window, NOT the `StructRef` index
/// of the `Array` wrapper: distinct wrapper structs (reshape, a `MemoryRef` at
/// another offset, a second binding over the same `Memory`) share one backing
/// `Memory`, and the write-back commits *through that `Memory`* — keying the
/// check on `StructRef` would miss them and silently lose stores (found in
/// adversarial review; see `Vm::numeric_vector_storage_id`). A native
/// `ArrayRef` local and a struct wrapper built over that same native carrier
/// collide through the shared `native` pointer space for the same reason.
type ArrayIdentity = super::builtins_arrays::VectorStorageId;

/// A whole-function frame-less block (Issue #9693): a small typed function
/// (scalar i64/f64/ComplexF64 params, scalar body, all exits via `Return*`)
/// predecoded into typed ops and executed directly from call arguments — no
/// frame, no argument slot binding, no per-instruction dispatch, no return
/// routing. Generalizes the `I64FunctionBlock` precedent to the typed-loop op
/// set (early returns included), so escape kernels like
/// `mandel_point(c::ComplexF64, maxiter)` run their entire body natively.
#[derive(Debug, Clone)]
pub(crate) struct TypedScalarFunctionBlock {
    params: Vec<TypedFunctionParamBinding>,
    ops: Vec<TypedLoopOp>,
    /// Frame-less predecoded I64 callees referenced by `TypedLoopOp::CallI64Function`
    /// (Issue #10309).
    i64_callees: Vec<I64FunctionBlock>,
    /// Frame-less predecoded F64 callees referenced by `TypedLoopOp::CallF64Function`.
    f64_callees: Vec<F64FunctionBlock>,
    /// Issue #10542: frame-less predecoded mixed-type I64-shaped callees
    /// referenced by `TypedLoopOp::CallTypedI64Function`.
    typed_i64_callees: Vec<TypedScalarFunctionBlock>,
    /// Issue #10542: frame-less predecoded mixed-type F64-shaped callees
    /// referenced by `TypedLoopOp::CallTypedF64Function`.
    typed_f64_callees: Vec<TypedScalarFunctionBlock>,
    /// Issue #10565: `certify_typed_ops_trusted(&ops)`, computed once at
    /// predecode (see `TypedLoopBlock::ops_trusted`) so per-call and
    /// per-broadcast-element sites do not re-scan the op list.
    ops_trusted: bool,
}

/// How a call argument binds into `TypedOpsState`.
#[derive(Debug, Clone, Copy)]
enum TypedFunctionParamBinding {
    /// dense i64 local index
    I64(usize),
    /// dense f64 local index
    F64(usize),
    /// `complex_params` index; the argument must be a `Complex{Float64}`
    /// struct with two genuine `Value::F64` fields (Issue #9167 strictness)
    ComplexF64(usize),
    /// the body never reads this param before writing it (or at all)
    Unused,
}

#[derive(Debug, Clone, Copy)]
enum TypedLoopTarget {
    Op(usize),
    Exit,
    LoopBack,
}

/// Ordered/equality relation for the frame-less scalar function IR and the
/// typed-loop op set. I64 and F64 relations are byte-identical; since Issue
/// #10427 they share this one enum, with `I64Relation` / `F64Relation` kept as
/// aliases so the (many) existing typed-loop op sites and the `#[doc(hidden)]`
/// F64 test API keep compiling unchanged.
#[derive(Debug, Clone, Copy)]
pub enum ScalarRelation {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

pub(crate) type I64Relation = ScalarRelation;
pub type F64Relation = ScalarRelation;

/// A scalar element type (`i64` or `f64`) for the frame-less scalar function IR
/// (Issue #10427). Supplies the type-specific arithmetic, comparison, and
/// profiler-tag details that parameterize the otherwise-shared
/// `ScalarFunctionBlock` / `ScalarFunctionOp` / `ScalarFunctionBuilder` and the
/// single generic mini-interpreter (`execute_scalar_function_block`).
///
/// Type-specific semantics kept local here (per Issue #10427):
/// - I64: wrapping overflow, Euclidean-style checked modulo (`checked_rem`).
/// - F64: NaN-aware ordered comparison, total float division.
pub trait ScalarKind {
    /// The concrete operand type (`i64` or `f64`).
    type Scalar: Copy + Default + core::fmt::Debug;

    /// Profiler tag recorded when a whole block of this kind runs.
    const BLOCK_EVENT: &'static str;
    /// Profiler tag recorded on a frame-less nested call within a block.
    const NESTED_CALL_EVENT: &'static str;

    fn add(a: Self::Scalar, b: Self::Scalar) -> Self::Scalar;
    fn sub(a: Self::Scalar, b: Self::Scalar) -> Self::Scalar;
    fn mul(a: Self::Scalar, b: Self::Scalar) -> Self::Scalar;
    /// Division. Only ever emitted for `f64` blocks (integer `/` promotes to
    /// Float64 upstream, so an all-`i64` body never contains a `Div` op).
    fn div(a: Self::Scalar, b: Self::Scalar) -> Self::Scalar;
    /// Unary negation. Only ever emitted for `f64` blocks.
    fn neg(a: Self::Scalar) -> Self::Scalar;
    /// Absolute value (`abs`); I64 uses wrapping semantics, F64 uses `f64::abs`.
    fn abs(a: Self::Scalar) -> Self::Scalar;
    /// Checked remainder (`%`). Only ever emitted for `i64` blocks, where it
    /// bails (`None`) on division by zero / `i64::MIN % -1`.
    fn checked_rem(a: Self::Scalar, b: Self::Scalar) -> Option<Self::Scalar>;
    /// Evaluate a relation with this type's native semantics (I64 total order;
    /// F64 NaN-aware order).
    fn eval_relation(a: Self::Scalar, b: Self::Scalar, relation: ScalarRelation) -> bool;
}

/// Marker type driving the `i64` monomorphization of the scalar function IR.
pub(crate) struct I64Kind;
/// Marker type driving the `f64` monomorphization of the scalar function IR.
pub(crate) struct F64Kind;

impl ScalarKind for I64Kind {
    type Scalar = i64;
    const BLOCK_EVENT: &'static str = "ExecutableBlock::I64Function";
    const NESTED_CALL_EVENT: &'static str = "ExecutableBlock::I64FunctionNestedCall";
    #[inline(always)]
    fn add(a: i64, b: i64) -> i64 {
        a.wrapping_add(b)
    }
    #[inline(always)]
    fn sub(a: i64, b: i64) -> i64 {
        a.wrapping_sub(b)
    }
    #[inline(always)]
    fn mul(a: i64, b: i64) -> i64 {
        a.wrapping_mul(b)
    }
    #[inline(always)]
    fn div(a: i64, b: i64) -> i64 {
        // Never emitted for i64 blocks; kept total (no panic) for safety.
        if b == 0 {
            0
        } else {
            a.wrapping_div(b)
        }
    }
    #[inline(always)]
    fn neg(a: i64) -> i64 {
        // Never emitted for i64 blocks.
        a.wrapping_neg()
    }
    #[inline(always)]
    fn abs(a: i64) -> i64 {
        a.wrapping_abs()
    }
    #[inline(always)]
    fn checked_rem(a: i64, b: i64) -> Option<i64> {
        checked_i64_rem(a, b)
    }
    #[inline(always)]
    fn eval_relation(a: i64, b: i64, relation: ScalarRelation) -> bool {
        eval_i64_relation(a, b, relation)
    }
}

impl ScalarKind for F64Kind {
    type Scalar = f64;
    const BLOCK_EVENT: &'static str = "ExecutableBlock::F64Function";
    const NESTED_CALL_EVENT: &'static str = "ExecutableBlock::F64FunctionNestedCall";
    #[inline(always)]
    fn add(a: f64, b: f64) -> f64 {
        a + b
    }
    #[inline(always)]
    fn sub(a: f64, b: f64) -> f64 {
        a - b
    }
    #[inline(always)]
    fn mul(a: f64, b: f64) -> f64 {
        a * b
    }
    #[inline(always)]
    fn div(a: f64, b: f64) -> f64 {
        a / b
    }
    #[inline(always)]
    fn neg(a: f64) -> f64 {
        -a
    }
    #[inline(always)]
    fn abs(a: f64) -> f64 {
        a.abs()
    }
    #[inline(always)]
    fn checked_rem(a: f64, b: f64) -> Option<f64> {
        // Never emitted for f64 blocks.
        Some(a % b)
    }
    #[inline(always)]
    fn eval_relation(a: f64, b: f64, relation: ScalarRelation) -> bool {
        eval_f64_relation(a, b, relation)
    }
}

// Issue #8183: dense Float64 ODE / iterated-map bodies (Aizawa attractor ≈68
// ops, Barnsley fern ≈92 ops) are larger than the original 64-op window. Raised
// to 128 so these scalar hot loops are recognized as native typed loops. The
// cap only bounds the one-time predecode scan and the heap `ops` Vec; the
// per-iteration fixed stacks are sized by `TYPED_LOOP_STACK_CAP`.
const MAX_TYPED_LOOP_OPS: usize = 128;
const TYPED_LOOP_STACK_CAP: usize = 16;
// Issue #8183: the Aizawa ODE step keeps 16 live Float64 locals; raised from 16
// to 24 for headroom so such bodies clear the slot-count guard.
const TYPED_LOOP_SLOT_CAP: usize = 24;
// Issue #9693: frame-less typed scalar function blocks — max ComplexF64
// params per function and the in-flight complex mini-stack depth (the SROA
// preamble reads one field at a time, so 2 gives headroom).
const TYPED_FUNCTION_COMPLEX_PARAM_CAP: usize = 4;
const COMPLEX_MINI_STACK_CAP: usize = 2;
// Frame-less scalar function IR caps (Issue #10309 / #10426 / #10427). Shared
// by both the i64 and f64 monomorphizations; the `I64_*` / `F64_*` names are
// aliases retained for the type-specific predecode/dispatch sites.
const SCALAR_FUNCTION_SLOT_CAP: usize = 16;
const SCALAR_FUNCTION_CALLEE_CAP: usize = 8;
const MAX_SCALAR_FUNCTION_CALL_DEPTH: usize = 4;
const MAX_SCALAR_FUNCTION_OPS: usize = 128;
const I64_FUNCTION_SLOT_CAP: usize = SCALAR_FUNCTION_SLOT_CAP;
const I64_FUNCTION_CALLEE_CAP: usize = SCALAR_FUNCTION_CALLEE_CAP;
const MAX_I64_FUNCTION_CALL_DEPTH: usize = MAX_SCALAR_FUNCTION_CALL_DEPTH;
const MAX_I64_FUNCTION_OPS: usize = MAX_SCALAR_FUNCTION_OPS;
const F64_FUNCTION_SLOT_CAP: usize = SCALAR_FUNCTION_SLOT_CAP;
const MAX_F64_FUNCTION_OPS: usize = MAX_SCALAR_FUNCTION_OPS;
const F64_FUNCTION_CALLEE_CAP: usize = SCALAR_FUNCTION_CALLEE_CAP;
const MAX_F64_FUNCTION_CALL_DEPTH: usize = MAX_SCALAR_FUNCTION_CALL_DEPTH;

/// Why the typed-loop recognizer declined a loop-header candidate. Recorded at
/// the known bail points of [`try_predecode_typed_loop_range`] and surfaced (env
/// gated) by [`log_typed_loop_reject`] so the distribution of rejection reasons
/// across real Float64 hot loops can be measured (Issue #8193). Diagnostics only:
/// the recognizer's accept/reject decision is unchanged.
#[derive(Debug, Clone, Copy)]
enum TypedLoopReject {
    /// An instruction the typed-loop IR has no op for (the catch-all bail, plus
    /// the intentionally-skipped `StoreSlotArray` / multi-index store). The
    /// `usize` is the offending instruction's ip, so the log can name it.
    UnsupportedInstr(usize),
    /// Loop body longer than `MAX_TYPED_LOOP_OPS`.
    OpCountOverCap,
    /// More distinct array/f64/i64 slots than `TYPED_LOOP_SLOT_CAP`.
    SlotCountOverCap,
    /// No branch leaves the loop (`has_exit` stayed false).
    NoExit,
}

/// `SJULIA_TYPED_LOOP_DEBUG`: when set, the typed-loop recognizer prints one
/// `[typed-loop-reject]` / `[typed-loop-accept]` line per loop-header candidate
/// it considers, for measuring native-fast-path coverage (Issue #8193). Cached
/// once; checked only at predecode/install time, never on the execution hot path.
fn typed_loop_debug_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SJULIA_TYPED_LOOP_DEBUG").is_some())
}

/// Emit a typed-loop recognizer diagnostic line without `eprintln!`
/// (`#![deny(clippy::print_stderr)]` forbids it crate-wide — mirror
/// `dispatch_debug_log`'s `writeln!(stderr)` form).
fn typed_loop_debug_log(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{args}");
}

/// Short variant name of an instruction (payload dropped) for reject logs, e.g.
/// `LoadSlotF64(3)` -> `LoadSlotF64`, `CallDynamicBinaryBoth(..)` -> `CallDynamicBinaryBoth`.
fn instr_kind(instr: &Instr) -> String {
    let rendered = format!("{instr:?}");
    match rendered.find(['(', ' ', '{']) {
        Some(idx) => rendered[..idx].to_string(),
        None => rendered,
    }
}

/// Print (env gated) why a real loop header was not lowered to a typed loop.
fn log_typed_loop_reject(code: &[Instr], header_ip: usize, reject: Option<TypedLoopReject>) {
    if !typed_loop_debug_enabled() {
        return;
    }
    let reason = match reject {
        Some(TypedLoopReject::UnsupportedInstr(ip)) => match code.get(ip) {
            Some(instr) => format!("unsupported-instr:{}", instr_kind(instr)),
            None => "unsupported-instr:?".to_string(),
        },
        Some(TypedLoopReject::OpCountOverCap) => "op-count-over-cap".to_string(),
        Some(TypedLoopReject::SlotCountOverCap) => "slot-count-over-cap".to_string(),
        Some(TypedLoopReject::NoExit) => "no-exit".to_string(),
        None => "other-stack-or-target".to_string(),
    };
    typed_loop_debug_log(format_args!(
        "[typed-loop-reject] header_ip={header_ip} reason={reason}"
    ));
}

/// Fold per-op transactionality facts once for the recognizer guard. Array
/// stores are bail-capable but write a discardable `ArrayWriteOrigin` buffer;
/// `RandF64` is the sole current effect outside the buffered typed-loop state.
fn typed_loop_effects(ops: &[TypedLoopOp]) -> TypedLoopOpEffects {
    ops.iter().fold(TypedLoopOpEffects::NONE, |aggregate, op| {
        let effects = op.effects();
        TypedLoopOpEffects {
            bail_capable: aggregate.bail_capable || effects.bail_capable,
            out_of_buffer_effect: aggregate.out_of_buffer_effect || effects.out_of_buffer_effect,
        }
    })
}

/// Issue #10516: cap on the size of a callee body eligible for typed-loop
/// inlining, and on the total spliced op-list size. Small bodies (`mygcd`-like
/// helpers) are where the per-call overhead dominates; larger bodies keep the
/// frame-less call.
const INLINE_MAX_CALLEE_OPS: usize = 32;
const INLINE_MAX_RESULT_OPS: usize = 256;

/// Net i64-operand-stack effect `(pops, pushes)` of a typed-loop op, for the
/// linear depth simulation the inliner runs over an already-accepted block
/// (Issue #10516). `None` marks ops the simulation does not model (f64/bool/
/// array-only ops have `(0, 0)` i64 effect and are listed explicitly where
/// they touch the i64 stack).
fn typed_loop_op_i64_stack_effect(op: &TypedLoopOp) -> (usize, usize) {
    match op {
        TypedLoopOp::PushI64(_)
        | TypedLoopOp::LoadI64Slot(_)
        | TypedLoopOp::LoadAddConstI64Slot(_, _)
        // Issue #10559: `StrLen` pops from the STRING mini-stack and pushes an
        // i64, so its i64 effect is a net push. The other String ops
        // (`LoadStrSlot`/`StoreStrSlot`/`PushStrConst`/`ConcatStr`/`EqStr`)
        // never touch the i64 stack and correctly fall through to `(0, 0)`.
        // Getting this wrong under-counts the inliner's i64 depth simulation.
        | TypedLoopOp::StrLen => (0, 1),
        TypedLoopOp::DupI64 => (1, 2),
        TypedLoopOp::ToF64
        | TypedLoopOp::LoadI64SlotToF64(_)
        | TypedLoopOp::StoreI64Slot(_)
        | TypedLoopOp::IncI64Slot(_)
        | TypedLoopOp::DecI64Slot(_)
        | TypedLoopOp::ReturnI64 => (1, 0),
        TypedLoopOp::AddI64 | TypedLoopOp::SubI64 | TypedLoopOp::MulI64 | TypedLoopOp::ModI64 => {
            (2, 1)
        }
        TypedLoopOp::LoadAddI64Slot(_)
        | TypedLoopOp::LoadSubI64Slot(_)
        | TypedLoopOp::LoadMulI64Slot(_)
        | TypedLoopOp::LoadModI64Slot(_) => (1, 1),
        TypedLoopOp::CmpI64(_) | TypedLoopOp::JumpIfI64(_, _) => (2, 0),
        TypedLoopOp::IndexStoreI64 => (2, 0),
        TypedLoopOp::IndexLoadF64 => (1, 0),
        TypedLoopOp::IndexLoadI64 => (1, 1),
        TypedLoopOp::IndexStoreF64 => (1, 0),
        TypedLoopOp::AddF64I64Slots(_, _, _) => (0, 0),
        TypedLoopOp::CallI64Function(_, argc)
        | TypedLoopOp::CallSpecializeI64Function(_, argc)
        | TypedLoopOp::CallTypedI64Function(_, argc) => (*argc, 1),
        TypedLoopOp::CallF64Function(_, _)
        | TypedLoopOp::CallSpecializeF64Function(_, _)
        | TypedLoopOp::CallTypedF64Function(_, _) => (0, 0),
        // Issue #10567 (round 2): pops one i64 argument (`maxiter`) off the
        // i64 mini stack and pushes the i64 return value — the complex
        // argument comes from the SEPARATE complex mini stack, which this
        // i64-only depth model does not track. Must be listed explicitly:
        // the catch-all `_ => (0, 0)` below would otherwise silently
        // under-count this op's i64 effect and corrupt the #10516 inliner's
        // linear depth simulation for any OTHER call site inlined into the
        // same loop.
        TypedLoopOp::CallSpecializeComplexI64Function(_) => (1, 1),
        // Everything else never touches the i64 operand stack.
        _ => (0, 0),
    }
}

/// Net operand-stack effect of one typed-loop op across the four
/// fixed-capacity typed-loop stacks — `(pop, push)` for i64, f64, bool, and
/// the ComplexF64 mini-stack. Issue #10565: generalizes the i64-only
/// `typed_loop_op_i64_stack_effect` (#10516, used by the callee inliner) so
/// `certify_typed_ops_trusted` can prove the overflow/underflow invariants the
/// trusted executor relies on for EVERY stack whose checks it elides.
///
/// The match is deliberately EXHAUSTIVE — no `_` arm. A new `TypedLoopOp` must
/// fail to compile here rather than silently default to "no stack effect",
/// which would be a mis-certification and hence potential memory-unsafety.
///
/// The array stack (`Vec<ArrayRef>`) is intentionally NOT modelled: it keeps
/// its own `push_array_stack` / `pop_array_stack` checks unconditionally, in
/// both trusted and checked mode.
#[derive(Clone, Copy)]
struct TypedOpsStackEffect {
    i64_pop: usize,
    i64_push: usize,
    f64_pop: usize,
    f64_push: usize,
    bool_pop: usize,
    bool_push: usize,
    complex_pop: usize,
    complex_push: usize,
}

const TYPED_OPS_NO_STACK_EFFECT: TypedOpsStackEffect = TypedOpsStackEffect {
    i64_pop: 0,
    i64_push: 0,
    f64_pop: 0,
    f64_push: 0,
    bool_pop: 0,
    bool_push: 0,
    complex_pop: 0,
    complex_push: 0,
};

fn typed_loop_op_stack_effect(op: &TypedLoopOp) -> TypedOpsStackEffect {
    use TypedOpsStackEffect as E;
    let none = TYPED_OPS_NO_STACK_EFFECT;
    match op {
        // ---- array stack only (checked separately in both modes) ----
        // `DropArray` (#10566(b)) pops one runtime array-stack entry and
        // nothing else — like `LoadArraySlot`, it has no scalar-stack effect.
        TypedLoopOp::LoadArraySlot(_) | TypedLoopOp::DropArray => none,

        // ---- array element access ----
        TypedLoopOp::IndexStoreI64 => E { i64_pop: 2, ..none },
        TypedLoopOp::IndexStoreF64 => E {
            i64_pop: 1,
            f64_pop: 1,
            ..none
        },
        TypedLoopOp::IndexLoadF64 => E {
            i64_pop: 1,
            f64_push: 1,
            ..none
        },
        TypedLoopOp::IndexLoadI64 => E {
            i64_pop: 1,
            i64_push: 1,
            ..none
        },

        // ---- f64 stack ----
        TypedLoopOp::PushF64(_)
        | TypedLoopOp::RandF64
        | TypedLoopOp::LoadF64Slot(_)
        | TypedLoopOp::LoadSquareF64Slot(_)
        | TypedLoopOp::LoadI64SlotToF64(_)
        | TypedLoopOp::PushMulF64Slots(..)
        | TypedLoopOp::PushSumSquaresF64Slots(..)
        | TypedLoopOp::PushDiffSquaresF64Slots(..) => E {
            f64_push: 1,
            ..none
        },
        TypedLoopOp::DupF64 => E {
            f64_pop: 1,
            f64_push: 2,
            ..none
        },
        TypedLoopOp::StoreF64Slot(_)
        | TypedLoopOp::ReturnF64
        | TypedLoopOp::AddF64SlotStore(..)
        | TypedLoopOp::JumpIfNotF64Const(..) => E { f64_pop: 1, ..none },
        // Pops `im` then `re` off the f64 stack and early-returns the boxed
        // Complex; two pops, no pushes.
        TypedLoopOp::ReturnStructF64x2(_) => E { f64_pop: 2, ..none },
        TypedLoopOp::LoadAddF64Slot(_)
        | TypedLoopOp::LoadSubF64Slot(_)
        | TypedLoopOp::LoadMulF64Slot(_)
        | TypedLoopOp::LoadDivF64Slot(_)
        | TypedLoopOp::NegF64 => E {
            f64_pop: 1,
            f64_push: 1,
            ..none
        },
        TypedLoopOp::AddF64 | TypedLoopOp::SubF64 | TypedLoopOp::MulF64 | TypedLoopOp::DivF64 => {
            E {
                f64_pop: 2,
                f64_push: 1,
                ..none
            }
        }
        TypedLoopOp::JumpIfF64(..) | TypedLoopOp::JumpIfNotF64(..) => E { f64_pop: 2, ..none },
        TypedLoopOp::CmpF64(_) => E {
            f64_pop: 2,
            bool_push: 1,
            ..none
        },

        // ---- i64 stack ----
        TypedLoopOp::PushI64(_)
        | TypedLoopOp::LoadI64Slot(_)
        | TypedLoopOp::LoadAddConstI64Slot(..) => E {
            i64_push: 1,
            ..none
        },
        TypedLoopOp::DupI64 => E {
            i64_pop: 1,
            i64_push: 2,
            ..none
        },
        TypedLoopOp::StoreI64Slot(_)
        | TypedLoopOp::IncI64Slot(_)
        | TypedLoopOp::DecI64Slot(_)
        | TypedLoopOp::ReturnI64 => E { i64_pop: 1, ..none },
        TypedLoopOp::ToF64 => E {
            i64_pop: 1,
            f64_push: 1,
            ..none
        },
        TypedLoopOp::AddI64 | TypedLoopOp::SubI64 | TypedLoopOp::MulI64 | TypedLoopOp::ModI64 => {
            E {
                i64_pop: 2,
                i64_push: 1,
                ..none
            }
        }
        TypedLoopOp::LoadAddI64Slot(_)
        | TypedLoopOp::LoadSubI64Slot(_)
        | TypedLoopOp::LoadMulI64Slot(_)
        | TypedLoopOp::LoadModI64Slot(_) => E {
            i64_pop: 1,
            i64_push: 1,
            ..none
        },
        TypedLoopOp::JumpIfI64(..) => E { i64_pop: 2, ..none },
        TypedLoopOp::CmpI64(_) => E {
            i64_pop: 2,
            bool_push: 1,
            ..none
        },

        // ---- frame-less calls: `argc` operands consumed, one result pushed ----
        TypedLoopOp::CallI64Function(_, argc)
        | TypedLoopOp::CallSpecializeI64Function(_, argc)
        | TypedLoopOp::CallTypedI64Function(_, argc) => E {
            i64_pop: *argc,
            i64_push: 1,
            ..none
        },
        TypedLoopOp::CallF64Function(_, argc)
        | TypedLoopOp::CallSpecializeF64Function(_, argc)
        | TypedLoopOp::CallTypedF64Function(_, argc) => E {
            f64_pop: *argc,
            f64_push: 1,
            ..none
        },

        // ---- ComplexF64 mini-stack ----
        TypedLoopOp::PushComplexParam(_) => E {
            complex_push: 1,
            ..none
        },
        TypedLoopOp::ComplexFieldF64(_) => E {
            complex_pop: 1,
            f64_push: 1,
            ..none
        },
        // Issue #10567 (round 2): pops `im` then `re` off the f64 stack and
        // pushes the `(re, im)` pair onto the complex mini stack.
        TypedLoopOp::MaterializeComplexF64 => E {
            f64_pop: 2,
            complex_push: 1,
            ..none
        },
        // Issue #10567 (round 2): pops one i64 argument off the i64 stack and
        // one complex value off the complex mini stack, pushes the i64
        // return value.
        TypedLoopOp::CallSpecializeComplexI64Function(_) => E {
            i64_pop: 1,
            i64_push: 1,
            complex_pop: 1,
            ..none
        },

        // ---- bool stack ----
        TypedLoopOp::JumpIfZero(_) => E {
            bool_pop: 1,
            ..none
        },

        // ---- Issue #10559 String ops ----
        // The string operand stack is a `Vec<StrRef>` carrying its OWN explicit
        // capacity/underflow checks, which stay in force in BOTH modes (like the
        // array stack) — TRUSTED elides nothing there, so it is not modelled.
        // But two of these ops push onto stacks TRUSTED DOES leave unchecked,
        // and those pushes MUST be modelled or the depth walk understates them:
        TypedLoopOp::EqStr => E {
            bool_push: 1,
            ..none
        },
        TypedLoopOp::StrLen => E {
            i64_push: 1,
            ..none
        },
        // …the rest touch only the string stack / string locals.
        TypedLoopOp::LoadStrSlot(_)
        | TypedLoopOp::StoreStrSlot(_)
        | TypedLoopOp::PushStrConst(_)
        | TypedLoopOp::ConcatStr(_) => none,

        // ---- slot-only ops: no operand stack traffic at all ----
        TypedLoopOp::AddF64Slots(..)
        | TypedLoopOp::AddF64I64Slots(..)
        | TypedLoopOp::AddConstI64Slot(..)
        | TypedLoopOp::UninitI64Slot(_)
        | TypedLoopOp::StoreComplexParamFieldF64(..)
        | TypedLoopOp::CopyF64Slots(..)
        | TypedLoopOp::CopyI64Slots(..)
        | TypedLoopOp::ComplexMulAddAssign { .. }
        | TypedLoopOp::JumpIfI64Slots(..)
        | TypedLoopOp::AddConstI64SlotAndJumpIfLe(..)
        | TypedLoopOp::Jump(_) => none,
    }
}

/// The `TypedLoopTarget` an op can transfer control to, if any (Issue #10565).
/// Exhaustive for the same reason as `typed_loop_op_stack_effect`: a new
/// jumping op must not be able to slip past the certifier's target check.
fn typed_loop_op_jump_target(op: &TypedLoopOp) -> Option<TypedLoopTarget> {
    match *op {
        TypedLoopOp::JumpIfZero(target)
        | TypedLoopOp::JumpIfI64(_, target)
        | TypedLoopOp::JumpIfI64Slots(_, _, _, target)
        | TypedLoopOp::AddConstI64SlotAndJumpIfLe(_, _, _, target)
        | TypedLoopOp::JumpIfF64(_, target)
        | TypedLoopOp::JumpIfNotF64(_, target)
        | TypedLoopOp::JumpIfNotF64Const(_, _, target)
        | TypedLoopOp::Jump(target) => Some(target),

        TypedLoopOp::LoadArraySlot(_)
        | TypedLoopOp::ReturnStructF64x2(_)
        | TypedLoopOp::IndexStoreI64
        | TypedLoopOp::IndexStoreF64
        | TypedLoopOp::IndexLoadF64
        | TypedLoopOp::IndexLoadI64
        | TypedLoopOp::PushF64(_)
        | TypedLoopOp::RandF64
        | TypedLoopOp::DupF64
        | TypedLoopOp::LoadF64Slot(_)
        | TypedLoopOp::StoreF64Slot(_)
        | TypedLoopOp::LoadSquareF64Slot(_)
        | TypedLoopOp::LoadAddF64Slot(_)
        | TypedLoopOp::LoadSubF64Slot(_)
        | TypedLoopOp::LoadMulF64Slot(_)
        | TypedLoopOp::LoadDivF64Slot(_)
        | TypedLoopOp::AddF64Slots(..)
        | TypedLoopOp::AddF64I64Slots(..)
        | TypedLoopOp::AddF64
        | TypedLoopOp::SubF64
        | TypedLoopOp::MulF64
        | TypedLoopOp::DivF64
        | TypedLoopOp::NegF64
        | TypedLoopOp::PushI64(_)
        | TypedLoopOp::DupI64
        | TypedLoopOp::ToF64
        | TypedLoopOp::LoadI64Slot(_)
        | TypedLoopOp::LoadI64SlotToF64(_)
        | TypedLoopOp::StoreI64Slot(_)
        | TypedLoopOp::AddI64
        | TypedLoopOp::SubI64
        | TypedLoopOp::MulI64
        | TypedLoopOp::ModI64
        | TypedLoopOp::LoadAddI64Slot(_)
        | TypedLoopOp::LoadSubI64Slot(_)
        | TypedLoopOp::LoadMulI64Slot(_)
        | TypedLoopOp::LoadModI64Slot(_)
        | TypedLoopOp::IncI64Slot(_)
        | TypedLoopOp::DecI64Slot(_)
        | TypedLoopOp::AddConstI64Slot(..)
        | TypedLoopOp::LoadAddConstI64Slot(..)
        | TypedLoopOp::ReturnI64
        | TypedLoopOp::CallI64Function(..)
        | TypedLoopOp::CallF64Function(..)
        | TypedLoopOp::CallTypedI64Function(..)
        | TypedLoopOp::CallTypedF64Function(..)
        | TypedLoopOp::CallSpecializeI64Function(..)
        | TypedLoopOp::CallSpecializeF64Function(..)
        | TypedLoopOp::UninitI64Slot(_)
        | TypedLoopOp::PushComplexParam(_)
        | TypedLoopOp::ComplexFieldF64(_)
        | TypedLoopOp::MaterializeComplexF64
        | TypedLoopOp::CallSpecializeComplexI64Function(..)
        | TypedLoopOp::StoreComplexParamFieldF64(..)
        | TypedLoopOp::ReturnF64
        | TypedLoopOp::PushMulF64Slots(..)
        | TypedLoopOp::PushSumSquaresF64Slots(..)
        | TypedLoopOp::PushDiffSquaresF64Slots(..)
        | TypedLoopOp::CopyF64Slots(..)
        | TypedLoopOp::CopyI64Slots(..)
        | TypedLoopOp::AddF64SlotStore(..)
        | TypedLoopOp::ComplexMulAddAssign { .. }
        | TypedLoopOp::CmpI64(_)
        | TypedLoopOp::CmpF64(_)
        | TypedLoopOp::LoadStrSlot(_)
        | TypedLoopOp::StoreStrSlot(_)
        | TypedLoopOp::PushStrConst(_)
        | TypedLoopOp::ConcatStr(_)
        | TypedLoopOp::EqStr
        | TypedLoopOp::StrLen
        | TypedLoopOp::DropArray => None,
    }
}

/// Issue #10565: certify a typed-loop op sequence safe for the TRUSTED
/// (unchecked) executor. Generalizes the i64-only linear depth simulation the
/// #10516 inliner runs to all four fixed-capacity typed-loop stacks.
///
/// One physical-order walk records each op's ENTRY depth `d[i]` and derives its
/// exit depth from `typed_loop_op_stack_effect`. The stream is certified only
/// when:
///
/// 1. no stack underflows (a pop never runs below its own arity),
/// 2. no stack exceeds its fixed capacity, and
/// 3. **every intra-block jump agrees on depth with its target**: for a jump at
///    `s` targeting `Op(t)`, the depth after `s` consumes its own operands must
///    equal `d[t]`, on all four stacks.
///
/// (3) is the crux: it is what makes the physical-order walk a SOUND model of
/// every reachable state. Induction over reachable positions — op 0 is entered
/// at depth 0 = `d[0]`; a fall-through from `i` reaches `i+1` at exit-depth(`i`)
/// = `d[i+1]` by construction; a jump `s -> t` arrives at exit-depth(`s`), which
/// (3) forces to equal `d[t]`. So real depth == `d[i]` at every reachable op,
/// and (1)/(2) — proven over `d[]` — hold for every real execution.
///
/// Note what (3) is NOT: "all four stacks empty at every branch". That rule is
/// TOO STRONG, and it silently made this whole optimization a no-op. The #10516
/// inliner translates a spliced callee's `Return` into a `Jump` past the body
/// that deliberately LEAVES THE RETURN VALUE on the i64 stack (the call op's
/// stack contract), so the coprime-pi kernel's hottest block jumps at i64 depth
/// 1 — under the empty-stack rule it never certified (measured: 4999/4999 loop
/// entries fell back to the checked executor).
///
/// Depth agreement still rejects the memory-unsafe shapes. Counterexample
/// (regression-tested, `..._rejects_jump_into_nonzero_depth_point`):
///
/// ```text
/// 0: PushI64(1)
/// 1: PushI64(2)      <- d[1] = 1
/// 2: AddI64          (pops 2)
/// 3: StoreI64Slot(0)
/// 4: Jump(Op(1))     <- exit depth 0, but d[1] = 1  =>  REJECT
/// ```
///
/// Every linear depth is in range and every branch sits at depth 0, so a
/// branches-only rule would certify it — yet the back-edge re-enters op 1 with
/// an EMPTY i64 stack, so `AddI64` pops 2 off a stack holding 1: in the trusted
/// executor, `*sp -= 1` underflowing `usize` and a `get_unchecked` at a wild
/// index.
///
/// `Exit` and `LoopBack` targets need no depth constraint: `Exit` breaks out of
/// the op stream, and `LoopBack` re-enters `'loop_body`, which re-declares every
/// stack pointer at 0 = `d[0]`. Values left on a stack there are discarded,
/// exactly as in the checked executor.
///
/// Conservative and purely structural: it proves nothing about local-slot
/// indices or the array stack, which keep their checks in BOTH modes. Callers
/// must certify the EXACT slice they are about to execute — the #10516
/// entry-time inliner renumbers locals and rewrites the stream, so a
/// certification of `block.ops` does not carry over to `inlined_ops`.
fn certify_typed_ops_trusted(ops: &[TypedLoopOp]) -> bool {
    // Allocation-free by construction. This runs on EVERY typed-loop block
    // ENTRY, and hot kernels re-enter constantly (Mandelbrot re-enters its
    // escape loop once per pixel — ~2.25M times for the 1500x1500 benchmark).
    // A `Vec`-based version of this pass measured as a ~6% REGRESSION there,
    // purely from the two per-entry heap allocations. Op streams are capped, so
    // the entry depths live in a fixed stack array; a longer stream is simply
    // not certified.
    if ops.len() > INLINE_MAX_RESULT_OPS {
        return false;
    }
    // Per-op entry depth for the four stacks. `u8` is ample: every cap is far
    // below 255, and (2) rejects anything that would exceed one.
    let mut entry = [[0u8; 4]; INLINE_MAX_RESULT_OPS];

    const I: usize = 0;
    const F: usize = 1;
    const B: usize = 2;
    const C: usize = 3;
    let caps = [
        TYPED_LOOP_STACK_CAP,
        TYPED_LOOP_STACK_CAP,
        TYPED_LOOP_STACK_CAP,
        COMPLEX_MINI_STACK_CAP,
    ];

    // Pass 1: linear depth walk — proves (1) and (2), records `d[]`.
    let mut depth = [0usize; 4];
    for (index, op) in ops.iter().enumerate() {
        for k in [I, F, B, C] {
            entry[index][k] = depth[k] as u8;
        }
        let effect = typed_loop_op_stack_effect(op);
        let pops = [
            effect.i64_pop,
            effect.f64_pop,
            effect.bool_pop,
            effect.complex_pop,
        ];
        let pushes = [
            effect.i64_push,
            effect.f64_push,
            effect.bool_push,
            effect.complex_push,
        ];
        for k in [I, F, B, C] {
            // (1) underflow.
            let Some(after_pop) = depth[k].checked_sub(pops[k]) else {
                return false;
            };
            depth[k] = after_pop + pushes[k];
            // (2) over-cap.
            if depth[k] > caps[k] {
                return false;
            }
        }
    }

    // Pass 2 — (3): depth agreement at every intra-block jump. An out-of-range
    // `Op(t)` would in fact be safe on its own (the executor's `jump_to!` bails
    // on `target >= ops.len()` in BOTH modes), but we decline to certify it
    // anyway: refusing is always the safe answer, and it keeps this function's
    // contract free of any dependency on a check that lives in the executor.
    for (s, op) in ops.iter().enumerate() {
        let Some(TypedLoopTarget::Op(t)) = typed_loop_op_jump_target(op) else {
            continue;
        };
        if t >= ops.len() {
            return false;
        }
        let effect = typed_loop_op_stack_effect(op);
        let pops = [
            effect.i64_pop,
            effect.f64_pop,
            effect.bool_pop,
            effect.complex_pop,
        ];
        let pushes = [
            effect.i64_push,
            effect.f64_push,
            effect.bool_push,
            effect.complex_push,
        ];
        for k in [I, F, B, C] {
            // Pass 1 already proved this pop cannot underflow.
            let exit_k = entry[s][k] as usize - pops[k] + pushes[k];
            if exit_k != entry[t][k] as usize {
                return false;
            }
        }
    }
    true
}

/// Translate one `ScalarFunctionOp<i64>` of an inlinable callee body into the
/// equivalent `TypedLoopOp` (Issue #10516). `local_map[callee_local]` is the
/// fresh caller-local index; body-internal jump targets shift by
/// `body_start`; `Return` becomes a jump past the spliced region with the
/// return value left on the i64 stack — exactly the call op's stack contract.
fn translate_inlined_scalar_i64_op(
    op: &ScalarFunctionOp<i64>,
    local_map: &[usize],
    body_start: usize,
    end_target: usize,
) -> Option<TypedLoopOp> {
    Some(match *op {
        ScalarFunctionOp::Push(v) => TypedLoopOp::PushI64(v),
        ScalarFunctionOp::LoadSlot(l) => TypedLoopOp::LoadI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::StoreSlot(l) => TypedLoopOp::StoreI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::Add => TypedLoopOp::AddI64,
        ScalarFunctionOp::Sub => TypedLoopOp::SubI64,
        ScalarFunctionOp::Mul => TypedLoopOp::MulI64,
        ScalarFunctionOp::Rem => TypedLoopOp::ModI64,
        ScalarFunctionOp::LoadAddSlot(l) => TypedLoopOp::LoadAddI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::LoadSubSlot(l) => TypedLoopOp::LoadSubI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::LoadMulSlot(l) => TypedLoopOp::LoadMulI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::LoadRemSlot(l) => TypedLoopOp::LoadModI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::IncSlot(l) => TypedLoopOp::IncI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::DecSlot(l) => TypedLoopOp::DecI64Slot(*local_map.get(l)?),
        ScalarFunctionOp::AddConstSlot(l, d) => TypedLoopOp::AddConstI64Slot(*local_map.get(l)?, d),
        ScalarFunctionOp::AddConstSlotAndJumpIfLe(l, d, stop, t) => {
            TypedLoopOp::AddConstI64SlotAndJumpIfLe(
                *local_map.get(l)?,
                d,
                *local_map.get(stop)?,
                TypedLoopTarget::Op(body_start + t),
            )
        }
        ScalarFunctionOp::Cmp(rel) => TypedLoopOp::CmpI64(rel),
        ScalarFunctionOp::JumpIfZero(t) => {
            TypedLoopOp::JumpIfZero(TypedLoopTarget::Op(body_start + t))
        }
        ScalarFunctionOp::JumpIf(rel, t) => {
            TypedLoopOp::JumpIfI64(rel, TypedLoopTarget::Op(body_start + t))
        }
        ScalarFunctionOp::JumpIfSlots(a, b, rel, t) => TypedLoopOp::JumpIfI64Slots(
            *local_map.get(a)?,
            *local_map.get(b)?,
            rel,
            TypedLoopTarget::Op(body_start + t),
        ),
        ScalarFunctionOp::Jump(t) => TypedLoopOp::Jump(TypedLoopTarget::Op(body_start + t)),
        ScalarFunctionOp::Return => TypedLoopOp::Jump(TypedLoopTarget::Op(end_target)),
        // `Div`/`LoadDivSlot`/`Neg` are f64-only (never emitted by the i64
        // predecoder), `Abs` has no typed-loop op, and nested `Call`s are
        // excluded by the eligibility check. Reject defensively.
        ScalarFunctionOp::Div
        | ScalarFunctionOp::LoadDivSlot(_)
        | ScalarFunctionOp::Neg
        | ScalarFunctionOp::Abs
        | ScalarFunctionOp::Call(_, _) => return None,
    })
}

/// A small predecoded I64 callee is inlinable when its whole body translates
/// into typed-loop ops (Issue #10516): no nested callees, within the op cap,
/// and no untranslatable op.
fn i64_callee_is_inlinable(callee: &I64FunctionBlock, arg_count: usize) -> bool {
    callee.callees.is_empty()
        && callee.ops.len() <= INLINE_MAX_CALLEE_OPS
        && callee.slots.len() <= SCALAR_FUNCTION_SLOT_CAP
        && callee
            .slots
            .iter()
            .all(|slot| slot.param_index.map(|p| p < arg_count).unwrap_or(true))
        && !callee.ops.iter().any(|op| {
            matches!(
                op,
                ScalarFunctionOp::Call(_, _)
                    | ScalarFunctionOp::Div
                    | ScalarFunctionOp::LoadDivSlot(_)
                    | ScalarFunctionOp::Neg
                    | ScalarFunctionOp::Abs
            )
        })
}

/// Issue #10516 (Option A): splice small resolved I64 callee bodies directly
/// into a typed loop's op stream at block-entry time, eliminating the
/// per-call argument copy, local (re)initialization, and nested mini-
/// interpreter dispatch of `execute_i64_function_block` — ~25M calls per
/// `calc_pi_n5000` run. Entry-time (rather than predecode-time) splicing
/// covers BOTH `CallI64Function` (predecoded callee) and
/// `CallSpecializeI64Function` (runtime-resolved callee) sites, and inherits
/// the specialize path's cache-invalidation contract for free: the callee is
/// re-resolved from the live cache on every entry, so nothing stale is ever
/// cached inside the block.
///
/// Returns `None` when nothing was inlined (no sites, or none eligible) —
/// the caller then runs the original op slice with zero overhead. Sites that
/// fail eligibility (body too big, untranslatable op, local/stack budget)
/// keep their call op; the mixed form is fine.
///
/// Correctness invariants:
/// - A site is inlined only when the linear i64-depth simulation puts exactly
///   `arg_count` values on the i64 stack at the call op (the recognizer emits
///   `LoadI64Slot` runs directly before each site, so this is the common
///   shape). The spliced prologue consumes them into fresh caller locals, so
///   the body starts at depth 0 — the same depth its standalone predecode
///   validated against `TYPED_LOOP_STACK_CAP`.
/// - Fresh locals live above the caller's own i64 locals and below
///   `TYPED_LOOP_SLOT_CAP`; they are never written back (not in
///   `block.i64_slots`).
/// - Each splice starts with `UninitI64Slot` for every non-param callee
///   local, reproducing the frame-less executor's per-call `local_init`
///   reset: a load-before-store path bails exactly like the un-inlined call.
/// - The #10504 side-effect guard already rejected any block mixing these
///   call sites with the out-of-buffer `RandF64` effect at recognize time, so
///   a bail from an inlined body never double-applies that effect. Array stores
///   are safe here because Issue #10566(c) made them transactional.
fn try_inline_i64_callees_into_typed_ops(
    ops: &[TypedLoopOp],
    i64_callees: &[I64FunctionBlock],
    specialize_callees: &[I64FunctionBlock],
    i64_local_count: usize,
) -> Option<Vec<TypedLoopOp>> {
    // Cheap pre-scan: any call site at all?
    if !ops.iter().any(|op| {
        matches!(
            op,
            TypedLoopOp::CallI64Function(_, _) | TypedLoopOp::CallSpecializeI64Function(_, _)
        )
    }) {
        return None;
    }

    // Linear i64-depth simulation. The recognizer enforced empty stacks at
    // every branch, so depth is well-defined along the linear order; bail out
    // conservatively on anything inconsistent.
    let mut depth_at = vec![0usize; ops.len()];
    let mut depth = 0usize;
    for (index, op) in ops.iter().enumerate() {
        depth_at[index] = depth;
        let (pops, pushes) = typed_loop_op_i64_stack_effect(op);
        depth = depth.checked_sub(pops)?.checked_add(pushes)?;
        if depth > TYPED_LOOP_STACK_CAP {
            return None;
        }
        let is_branch = matches!(
            op,
            TypedLoopOp::JumpIfZero(_)
                | TypedLoopOp::JumpIfI64(_, _)
                | TypedLoopOp::JumpIfI64Slots(_, _, _, _)
                | TypedLoopOp::AddConstI64SlotAndJumpIfLe(_, _, _, _)
                | TypedLoopOp::JumpIfF64(_, _)
                | TypedLoopOp::JumpIfNotF64(_, _)
                | TypedLoopOp::JumpIfNotF64Const(_, _, _)
                | TypedLoopOp::Jump(_)
                | TypedLoopOp::ReturnI64
                | TypedLoopOp::ReturnF64
        );
        if is_branch && depth != 0 {
            // Branch with a non-empty simulated stack: recognizer invariant
            // says this cannot happen; do not inline.
            return None;
        }
    }

    // Plan the splices: (site index, callee, arg_count, fresh local base).
    let mut next_free_local = i64_local_count;
    let mut plans: Vec<(usize, &I64FunctionBlock, usize, usize)> = Vec::new();
    let mut new_len = ops.len();
    for (index, op) in ops.iter().enumerate() {
        let (callee, arg_count) = match op {
            TypedLoopOp::CallI64Function(callee_index, argc) => {
                (i64_callees.get(*callee_index), *argc)
            }
            TypedLoopOp::CallSpecializeI64Function(scratch_index, argc) => {
                (specialize_callees.get(*scratch_index), *argc)
            }
            _ => continue,
        };
        let Some(callee) = callee else { continue };
        if !i64_callee_is_inlinable(callee, arg_count) || depth_at[index] != arg_count {
            continue;
        }
        if next_free_local + callee.slots.len() > TYPED_LOOP_SLOT_CAP {
            continue;
        }
        let uninit_count = callee
            .slots
            .iter()
            .filter(|slot| slot.param_index.is_none())
            .count();
        // splice = uninit ops + one param store per arg + body
        new_len = new_len - 1 + uninit_count + arg_count + callee.ops.len();
        if new_len > INLINE_MAX_RESULT_OPS {
            return None;
        }
        plans.push((index, callee, arg_count, next_free_local));
        next_free_local += callee.slots.len();
    }
    if plans.is_empty() {
        return None;
    }

    // Emit: copy ops, splicing each planned site. `map[old] = new start`.
    let mut new_ops: Vec<TypedLoopOp> = Vec::with_capacity(new_len);
    let mut map = vec![0usize; ops.len() + 1];
    let mut copied: Vec<usize> = Vec::with_capacity(ops.len());
    let mut plan_iter = plans.iter().peekable();
    for (index, op) in ops.iter().enumerate() {
        map[index] = new_ops.len();
        if let Some(&&(site, callee, arg_count, local_base)) = plan_iter.peek() {
            if site == index {
                plan_iter.next();
                // Fresh-local map for the callee's dense locals.
                let local_map: Vec<usize> =
                    (0..callee.slots.len()).map(|l| local_base + l).collect();
                // Per-call `local_init` reset for non-param locals.
                for (l, slot) in callee.slots.iter().enumerate() {
                    if slot.param_index.is_none() {
                        new_ops.push(TypedLoopOp::UninitI64Slot(local_map[l]));
                    }
                }
                // Bind the on-stack args into the callee's param locals
                // (pop order: last arg first).
                let mut param_store_order: Vec<usize> = Vec::with_capacity(arg_count);
                for param in (0..arg_count).rev() {
                    let local = callee
                        .slots
                        .iter()
                        .position(|slot| slot.param_index == Some(param))
                        .map(|l| local_map[l]);
                    param_store_order.push(local?);
                }
                for local in param_store_order {
                    new_ops.push(TypedLoopOp::StoreI64Slot(local));
                }
                let body_start = new_ops.len();
                let end_target = body_start + callee.ops.len();
                for body_op in &callee.ops {
                    new_ops.push(translate_inlined_scalar_i64_op(
                        body_op, &local_map, body_start, end_target,
                    )?);
                }
                continue;
            }
        }
        copied.push(new_ops.len());
        new_ops.push(*op);
    }
    map[ops.len()] = new_ops.len();

    // Remap copied ops' jump targets to the new indices; spliced ops carry
    // final targets already.
    for &new_index in &copied {
        let target = match &mut new_ops[new_index] {
            TypedLoopOp::JumpIfZero(t)
            | TypedLoopOp::JumpIfI64(_, t)
            | TypedLoopOp::JumpIfI64Slots(_, _, _, t)
            | TypedLoopOp::AddConstI64SlotAndJumpIfLe(_, _, _, t)
            | TypedLoopOp::JumpIfF64(_, t)
            | TypedLoopOp::JumpIfNotF64(_, t)
            | TypedLoopOp::JumpIfNotF64Const(_, _, t)
            | TypedLoopOp::Jump(t) => t,
            _ => continue,
        };
        if let TypedLoopTarget::Op(old) = target {
            *target = TypedLoopTarget::Op(*map.get(*old)?);
        }
    }

    Some(new_ops)
}

fn try_predecode_typed_loop(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    header_ip: usize,
    function_end: usize,
    base_function_count: usize,
) -> Option<TypedLoopBlock> {
    let scan_end = function_end.min(header_ip.checked_add(MAX_TYPED_LOOP_OPS + 1)?);
    let mut reject: Option<TypedLoopReject> = None;
    let mut saw_back_edge = false;
    for jump_ip in header_ip + 1..scan_end {
        // Recognize both plain Jump back-edges and fused counted-loop
        // back-edges. The peephole fuser emits
        // AddConstI64SlotAndJumpIfLe(..., header_ip + 1) when the loop
        // header was fused to JumpIfGtI64Slots, so the fused back-edge
        // jumps to the body start (header_ip + 1), not the header itself.
        let is_back_edge = match code.get(jump_ip) {
            Some(Instr::Jump(target)) => *target == header_ip,
            Some(Instr::AddConstI64SlotAndJumpIfLe(_, _, _, target)) => *target == header_ip + 1,
            _ => false,
        };
        if is_back_edge {
            saw_back_edge = true;
            if let Some(block) = try_predecode_typed_loop_range(
                code,
                functions,
                header_ip,
                jump_ip + 1,
                base_function_count,
                &mut reject,
            ) {
                if typed_loop_debug_enabled() {
                    typed_loop_debug_log(format_args!(
                        "[typed-loop-accept] header_ip={header_ip} ops={}",
                        block.ops.len()
                    ));
                }
                return Some(block);
            }
        }
    }
    // Only real loop headers (a back-edge to `header_ip` exists) are typed-loop
    // candidates; forward-only regions are not and stay silent (Issue #8193).
    if saw_back_edge {
        log_typed_loop_reject(code, header_ip, reject);
    }
    None
}

fn try_predecode_typed_loop_range(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    header_ip: usize,
    end_ip: usize,
    base_function_count: usize,
    reject: &mut Option<TypedLoopReject>,
) -> Option<TypedLoopBlock> {
    try_predecode_typed_ops_range(
        code,
        functions,
        header_ip,
        end_ip,
        base_function_count,
        None,
        reject,
        true,
    )
    .map(|(block, _)| block)
}

/// Shared recognizer core for typed-loop blocks and (Issue #9693) whole
/// typed-function blocks. `function_params = Some(param_slots)` enables the
/// function-mode extras — ComplexF64 param decompose windows
/// (`LoadSlotStruct(param); GetField(k)`) and `ReturnF64` — and the second
/// tuple element lists the ComplexF64 param slots in op-operand order.
/// `allow_typed_fallback` (Issue #10542) gates whether a direct-call site's
/// callee may fall back to the mixed-type `TypedScalarFunctionBlock`
/// decoder when it fails the pure-typed (`I64`/`F64`-only) predecode; it is
/// `false` while already decoding a fallback callee's own body, bounding
/// that recursion to one extra level (see `typed_loop_i64_call_op` /
/// `typed_loop_f64_call_op`).
fn try_predecode_typed_ops_range(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    header_ip: usize,
    end_ip: usize,
    base_function_count: usize,
    function_params: Option<&[usize]>,
    reject: &mut Option<TypedLoopReject>,
    allow_typed_fallback: bool,
) -> Option<(TypedLoopBlock, Vec<usize>)> {
    if end_ip <= header_ip || end_ip > code.len() {
        return None;
    }
    if end_ip - header_ip > MAX_TYPED_LOOP_OPS {
        *reject = Some(TypedLoopReject::OpCountOverCap);
        return None;
    }

    // Issue #10309: when the body contains a nested loop header, keep the outer
    // loop on the generic interpreter so the inner loop can be picked up as its
    // own typed-loop block. A typed loop that software-emulates the inner loop
    // inside its op list is slower than letting the inner block run natively.
    // This applies to loop mode only; typed scalar functions always contain a
    // single loop body (Issue #9693).
    if function_params.is_none() {
        for inner_header in header_ip + 1..end_ip {
            for jump_ip in inner_header + 1..end_ip.min(inner_header + MAX_TYPED_LOOP_OPS + 1) {
                match code.get(jump_ip) {
                    Some(Instr::Jump(target)) if *target == inner_header => {
                        *reject = Some(TypedLoopReject::UnsupportedInstr(inner_header));
                        return None;
                    }
                    Some(Instr::AddConstI64SlotAndJumpIfLe(_, _, _, target))
                        if *target == inner_header + 1 =>
                    {
                        *reject = Some(TypedLoopReject::UnsupportedInstr(inner_header));
                        return None;
                    }
                    _ => {}
                }
            }
        }
    }

    let mut builder = TypedLoopBuilder::default();
    let mut has_exit = false;
    // Issue #10645: set by the `NewStruct(type_id, 2)` arm below when it is
    // immediately followed by `ReturnStruct` (the `Complex{Float64}(r, i);
    // return` shape); consumed by the very next iteration's `ReturnStruct`
    // arm. Never observed to persist past one iteration.
    let mut pending_struct_return: Option<usize> = None;
    // Issue #10104: lazily-resolved function owning `header_ip`, used only when a
    // typed `IndexLoad*` needs the indexed array param's element type. Outer
    // `Option` = "resolved yet?"; inner = "found a match".
    let mut enclosing_fn: Option<Option<&Rc<FunctionInfo>>> = None;
    let mut ip_to_first_op = Vec::with_capacity(end_ip - header_ip);
    for ip in header_ip..end_ip {
        ip_to_first_op.push(builder.ops.len());
        let instr = code.get(ip)?;
        match instr {
            // Issue #9693 (function mode): the SROA param-hoist preamble reads
            // a ComplexF64 param's fields. Only param slots qualify; anything
            // else keeps the catch-all reject below.
            Instr::LoadSlotStruct(slot)
                if function_params.is_some_and(|params| params.contains(slot)) =>
            {
                let Some(idx) = builder.complex_slot(*slot) else {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                };
                builder.push_complex()?;
                builder.ops.push(TypedLoopOp::PushComplexParam(idx));
            }
            // Issue #10567/#10704: the *runtime* specializer's param-hoist
            // preamble (`subset_julia_vm_vm/src/vm/specialize`) does not know
            // statically that an `::Any`-typed fallback parameter holds a
            // concrete `ComplexF64` at this specialization instantiation, so
            // it reads the param via the generic `LoadSlot` rather than
            // `LoadSlotStruct` — the two are otherwise the same "param-hoist
            // reads field `field` of param `slot`" shape. Recognize that
            // shape by peeking at the immediately-following instruction: a
            // genuine complex-decompose preamble always emits
            // `LoadSlot(param); GetField(0..=1)` back-to-back (mirroring
            // `real(p)` / `imag(p)`), so gate on that lookahead in addition to
            // "param slot" to avoid misreading an ordinary `::Any` scalar
            // param read as a complex decompose. A slot that legitimately
            // holds `Int64`/`Float64` never has `GetField` applied to the
            // value `LoadSlot` just pushed (those types have no fields), so
            // this cannot misfire on scalar params; a false-positive `Struct`
            // guess is still caught at bind time by
            // `bind_typed_function_param`, which bails (falls back to the
            // frame path) unless the argument is a genuine two-`F64`-field
            // `Complex{Float64}`.
            Instr::LoadSlot(slot)
                if function_params.is_some_and(|params| params.contains(slot))
                    && matches!(code.get(ip + 1), Some(Instr::GetField(0..=1))) =>
            {
                let Some(idx) = builder.complex_slot(*slot) else {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                };
                builder.push_complex()?;
                builder.ops.push(TypedLoopOp::PushComplexParam(idx));
            }
            Instr::GetField(field) if *field <= 1 && builder.complex_depth > 0 => {
                builder.pop_complex()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::ComplexFieldF64(*field));
            }
            // Issue #10567 (round 2): the runtime specializer's `Complex(re,
            // im)` constructor for an argument being passed to another
            // specializable function compiles to `NewParametricStruct
            // ("Complex", 2)` (it does not statically know the field types
            // resolve to `Float64` the way a static `NewStruct` site does —
            // see the `NewStruct`/`ReturnStruct` fusion above). Recognize the
            // NARROW idiom this call-site shape actually produces:
            // `NewParametricStruct("Complex", 2)` immediately followed by a
            // single `LoadSlotI64`, immediately followed by a 2-argument
            // `CallSpecialize`/`CallSpecializeInbounds` — i.e. exactly
            // `f(complex_expr, i64_local)`. The lookahead is intentionally
            // strict (three fixed, contiguous instructions) so this can only
            // ever fire for that one shape; any other use of the freshly
            // built complex value (stored, returned, a different arg count
            // or order, more instructions in between) falls through
            // unchanged to the catch-all reject below, exactly like the
            // `NewStruct`/`ReturnStruct` fusion's own lookahead.
            //
            // Adversarial-review note (codex): could the two `pop_f64()`
            // calls below grab an unrelated deeper f64 instead of this
            // constructor's own `(re, im)` arguments? No — constructor-call
            // codegen always compiles `Complex(re_expr, im_expr)`'s two
            // argument subexpressions IMMEDIATELY before the
            // `NewParametricStruct` instruction itself (standard "compile
            // each arg, then the call" codegen, the same universal pattern
            // `CallSpecialize`'s own arguments follow), so the two f64
            // values on top of the real stack at this exact instruction ARE
            // this constructor's own `re`/`im`, by construction — no
            // interleaving is possible without an intervening instruction
            // this recognizer would have to (and does) separately validate.
            Instr::NewParametricStruct(name, 2)
                if function_params.is_none()
                    && name == "Complex"
                    && matches!(code.get(ip + 1), Some(Instr::LoadSlotI64(_)))
                    && matches!(
                        code.get(ip + 2),
                        Some(Instr::CallSpecialize(_, 2))
                            | Some(Instr::CallSpecializeInbounds(_, 2))
                    ) =>
            {
                builder.pop_f64()?; // im
                builder.pop_f64()?; // re
                builder.push_complex()?;
                builder.ops.push(TypedLoopOp::MaterializeComplexF64);
            }
            // Issue #10567 (round 2): the call op ending the 3-instruction
            // window recognized above. The adjacency check below (ip-2 is the
            // `NewParametricStruct` that pushed the complex we are about to
            // pop, ip-1 is the `LoadSlotI64` that pushed the i64) is the
            // real, sufficient proof of the args' identity — it is exactly
            // the same lookahead the `NewParametricStruct` arm already did,
            // just re-derived on this side of the fixed 3-instruction
            // window. Deliberately NOT checked here: `builder.i64_depth`/
            // `f64_depth`/`complex_depth` exact VALUES — an unrelated i64 or
            // f64 value can legitimately be sitting deeper on its mini stack
            // at this point (e.g. `total` in `total += mandel_point(...)` is
            // loaded via the generic-slot arm's I64 guess *before* this
            // call's own arguments and stays on the i64 stack, waiting for
            // the post-call add), so an exact-depth check would reject the
            // very shape this op targets. Popping the top of each mini stack
            // (LIFO) is correct regardless of what else is beneath.
            Instr::CallSpecialize(spec_func_index, 2)
            | Instr::CallSpecializeInbounds(spec_func_index, 2)
                if function_params.is_none()
                    && matches!(code.get(ip.wrapping_sub(2)), Some(Instr::NewParametricStruct(n, 2)) if n == "Complex")
                    && matches!(code.get(ip.wrapping_sub(1)), Some(Instr::LoadSlotI64(_))) =>
            {
                if builder.specialize_complex_i64_callees.len() >= I64_FUNCTION_CALLEE_CAP {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                }
                let scratch_index = builder.specialize_complex_i64_callees.len();
                builder
                    .specialize_complex_i64_callees
                    .push(*spec_func_index);
                builder.pop_i64()?;
                builder.pop_complex()?;
                builder.push_i64()?;
                builder
                    .ops
                    .push(TypedLoopOp::CallSpecializeComplexI64Function(scratch_index));
            }
            Instr::ReturnF64 if function_params.is_some() => {
                builder.pop_f64()?;
                builder.ops.push(TypedLoopOp::ReturnF64);
                has_exit = true;
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            // Issue #10645: `Complex{Float64}(r, i)` (a concrete, non-dynamic
            // parametric constructor call) compiles to a static
            // `NewStruct(type_id, 2)` immediately followed by `ReturnStruct`
            // when it is the tail expression of a function. Recognize the
            // pair as one fused op so the generic `+`/`*` method bodies for
            // `Complex{Float64}` can predecode instead of rejecting on
            // `NewStruct`. The lookahead keeps this narrow: `pending_struct_return`
            // is only set when the immediately following instruction is
            // `ReturnStruct`, so any other use of a freshly built 2-field
            // struct (stored, passed on, etc.) keeps falling through to the
            // catch-all reject below, unchanged from before this Issue.
            Instr::NewStruct(type_id, 2)
                if function_params.is_some()
                    && matches!(code.get(ip + 1), Some(Instr::ReturnStruct))
                    && !code.iter().any(
                        |instr| matches!(instr, Instr::DefineEvalStruct(marker_id) if marker_id == type_id),
                    ) =>
            {
                builder.pop_f64()?; // im
                builder.pop_f64()?; // re
                pending_struct_return = Some(*type_id);
            }
            Instr::ReturnStruct if pending_struct_return.is_some() => {
                // Issue #10907: guarded by the arm's `is_some()` check above,
                // but this is an optimizer predecode pass where "the
                // invariant somehow didn't hold" should fall back to the
                // interpreter (`?` -> `None`) like every other rejection in
                // this function, not panic.
                let type_id = pending_struct_return.take()?;
                builder.ops.push(TypedLoopOp::ReturnStructF64x2(type_id));
                has_exit = true;
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::PushF64(value) => {
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::PushF64(*value));
            }
            Instr::RandF64 => {
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::RandF64);
            }
            Instr::DupF64 => {
                builder.pop_f64()?;
                builder.push_f64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::DupF64);
            }
            Instr::LoadSlotF64(slot) => {
                let local = builder.read_f64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadF64Slot(local));
            }
            Instr::LoadSlotArray(slot) => {
                let local = builder.read_array_slot(*slot);
                builder.push_array()?;
                // Issue #10104: remember the source slot so a following typed
                // `IndexLoad*` can resolve the array's declared element type.
                builder.array_slot_stack.push(*slot);
                builder.ops.push(TypedLoopOp::LoadArraySlot(local));
            }
            Instr::StoreSlotArray(slot) if builder.array_slot_stack.last() == Some(slot) => {
                // Workaround: skip typed executable StoreSlotArray for generic VM fallback (Issue #7538).
                // The regular VM StoreSlotArray path can fall back
                // to storing arbitrary Value payloads when a statically
                // array-typed slot receives a macro/runtime Expr array. The
                // typed executable array stack has no equivalent fallback, so
                // let the normal interpreter handle this instruction.
                //
                // Issue #10566(b): narrowed. Inside an already-accepted typed
                // block the array stack only ever holds `ArrayRef`s sourced
                // from `LoadSlotArray`/`IndexStoreTyped` — never the
                // macro/Expr payloads the #7538 reject above guards against.
                // The common shape is an identity rebind:
                // `LoadSlotArray(s); ...; IndexStoreTyped(1); StoreSlotArray(s)`
                // with the SAME slot `s` — `array_slot_stack.last() == Some(s)`
                // is exactly that provenance (see the `IndexStoreTyped`
                // invariant note above), so the store is a no-op: the typed
                // local for `s` already holds the updated value. Elide it.
                // A cross-slot store (`tmp = a`) is real aliasing between two
                // different typed locals — keep the #7538 reject for that
                // shape, since it is not this narrow rebind.
                builder.pop_array()?;
                builder.array_slot_stack.pop();
                // Issue #10566(b): still emit an op — the elided rebind
                // means no local write, but the array value pushed back
                // by `IndexStoreTyped` is a real runtime `array_stack`
                // entry that must come off (see `TypedLoopOp::DropArray`).
                builder.ops.push(TypedLoopOp::DropArray);
            }
            Instr::StoreSlotArray(_) => {
                *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                return None;
            }
            Instr::StoreSlotF64(slot) => {
                builder.pop_f64()?;
                let local = builder.write_f64_slot(*slot);
                builder.ops.push(TypedLoopOp::StoreF64Slot(local));
            }
            Instr::LoadSquareF64Slot(slot) => {
                let local = builder.read_f64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadSquareF64Slot(local));
            }
            Instr::LoadAddF64Slot(slot) => {
                builder.pop_f64()?;
                let local = builder.read_f64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadAddF64Slot(local));
            }
            Instr::LoadSubF64Slot(slot) => {
                builder.pop_f64()?;
                let local = builder.read_f64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadSubF64Slot(local));
            }
            Instr::LoadMulF64Slot(slot) => {
                builder.pop_f64()?;
                let local = builder.read_f64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadMulF64Slot(local));
            }
            Instr::AddF64 => {
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::AddF64);
            }
            Instr::SubF64 => {
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::SubF64);
            }
            Instr::MulF64 => {
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::MulF64);
            }
            // Issue #8183: Float64 division (`/`) and the fused `load slot; /`.
            Instr::DivF64 => {
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::DivF64);
            }
            Instr::LoadDivF64Slot(slot) => {
                builder.pop_f64()?;
                let local = builder.read_f64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadDivF64Slot(local));
            }
            // Issue #9126: fused `slot[dst] = slot[lhs] + slot[rhs]` (and its
            // I64→F64 converting rhs form). No net stack effect; reads lhs/rhs
            // and writes dst directly. `read_*_slot` maps the bytecode slot to
            // the typed-loop local before `write_f64_slot` so a self-accumulate
            // (`dst == lhs`) reads the current value first.
            Instr::AddF64Slots(dst, lhs, rhs) => {
                let lhs_local = builder.read_f64_slot(*lhs);
                let rhs_local = builder.read_f64_slot(*rhs);
                let dst_local = builder.write_f64_slot(*dst);
                builder
                    .ops
                    .push(TypedLoopOp::AddF64Slots(dst_local, lhs_local, rhs_local));
            }
            Instr::AddF64I64Slots(dst, lhs, rhs) => {
                let lhs_local = builder.read_f64_slot(*lhs);
                let rhs_local = builder.read_i64_slot(*rhs);
                let dst_local = builder.write_f64_slot(*dst);
                builder
                    .ops
                    .push(TypedLoopOp::AddF64I64Slots(dst_local, lhs_local, rhs_local));
            }
            // Issue #8183: unary Float64 negation (`-x`), emitted either as the
            // `NegF64` instruction or the `NegFloat` intrinsic.
            Instr::NegF64 | Instr::CallIntrinsic(Intrinsic::NegFloat) => {
                builder.pop_f64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::NegF64);
            }
            Instr::PushI64(value) => {
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::PushI64(*value));
            }
            Instr::DupI64 => {
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::DupI64);
            }
            Instr::ToF64 => {
                builder.pop_i64()?;
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::ToF64);
            }
            Instr::LoadSlot(slot) => {
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadI64Slot(local));
            }
            Instr::LoadSlotI64(slot) => {
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadI64Slot(local));
            }
            Instr::LoadSlotI64ToF64(slot) => {
                let local = builder.read_i64_slot(*slot);
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::LoadI64SlotToF64(local));
            }
            Instr::StoreSlotI64(slot) => {
                builder.pop_i64()?;
                let local = builder.write_i64_slot(*slot);
                builder.ops.push(TypedLoopOp::StoreI64Slot(local));
            }
            // Issue #10567 (round 2): symmetric write-side guess for the
            // bare `Instr::LoadSlot(slot) => { ... push_i64 ... }` read-side
            // guess two arms above (loop mode's decades-old "an untyped slot
            // read is treated as I64, backed by `frame.slot_i64()`'s real
            // tag check" idiom). An accumulator whose static type the
            // specializer could not prove (e.g. `total` in
            // `total += mandel_point(...)`, where `mandel_point`'s return
            // type is only known dynamically to THIS loop's own predecode,
            // not to the specializer's local type table) is written back via
            // the generic `Instr::StoreSlot` rather than `StoreSlotI64`.
            // `Frame::set_slot_i64` writes into the SAME `locals_slots`
            // tagged-`Value` array `Frame::set_slot_value` (the generic
            // store's target) does, so the write is byte-identical to what
            // an interpreter-driven `StoreSlot` of a genuine `Value::I64`
            // would have produced — indistinguishable to any later generic
            // read (`LoadAny`, or the read-side guess above). Loop mode only,
            // matching the read-side guess's own scope (a function-mode
            // typed scalar block's narrower contract does not carry the same
            // "everything reaching the i64 stack is i64-proven" invariant —
            // see the `DynamicAdd`/`Sub`/`Mul` arms just above, which this
            // pairs with). Same adversarial-review point as those arms
            // applies here too: `Instr::StoreSlot(var)` always immediately
            // follows the assignment's fully-compiled RHS value (standard
            // "compile RHS, then Store" codegen — nothing else can be
            // interleaved between an expression's last op and its own
            // Store), so `pop_i64()`'s single pop is provably the RHS result,
            // not an unrelated deeper stack entry.
            Instr::StoreSlot(slot) if function_params.is_none() => {
                builder.pop_i64()?;
                let local = builder.write_i64_slot(*slot);
                builder.ops.push(TypedLoopOp::StoreI64Slot(local));
            }
            // Issue #10559: String slot reads/writes + accumulation. Every
            // value that ever reaches the string mini-stack in this block was
            // produced by one of these three ops (the linear per-type depth
            // simulation below rejects the whole block — falling back to the
            // generic interpreter — the moment a `StringConcat`/`ConcatStrings`/
            // `EqStr` site would need to pop something that isn't a str-stack
            // push), so `ConcatStr`/`EqStr` never need to re-check the runtime
            // type of their operands.
            //
            // LOOP MODE ONLY (`function_params.is_none()`). The frame-less
            // `TypedScalarFunctionBlock` (Issue #9693) and the broadcast block
            // reuse this same recognizer core but keep only `ops` — they carry
            // no `str_slots` / `str_consts`, and their `TypedOpsState::new(0, 0)`
            // has a zero-length String slot vector. Emitting a String op into a
            // function-mode block would therefore index an empty vector. Gate
            // emission at the source rather than patching the executor.
            Instr::PushStr(s) if function_params.is_none() => {
                let idx = builder.str_const(StrRef::from(s.as_str()))?;
                builder.push_str()?;
                builder.ops.push(TypedLoopOp::PushStrConst(idx));
            }
            Instr::LoadSlotStr(slot) if function_params.is_none() => {
                let local = builder.read_str_slot(*slot);
                builder.push_str()?;
                builder.ops.push(TypedLoopOp::LoadStrSlot(local));
            }
            Instr::StoreSlotStr(slot) if function_params.is_none() => {
                builder.pop_str()?;
                let local = builder.write_str_slot(*slot);
                builder.ops.push(TypedLoopOp::StoreStrSlot(local));
            }
            // `*` on `String` operands lowers to `StringConcat`/`ConcatStrings`
            // (general N-ary "format + join" ops that also back string
            // interpolation). Only the all-`String`-operand case is fast-pathed
            // here; the depth-underflow reject above is what enforces that.
            Instr::StringConcat(n) | Instr::ConcatStrings(n) if function_params.is_none() => {
                if *n == 0 {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                }
                for _ in 0..*n {
                    builder.pop_str()?;
                }
                builder.push_str()?;
                builder.ops.push(TypedLoopOp::ConcatStr(*n));
            }
            Instr::EqStr if function_params.is_none() => {
                builder.pop_str()?;
                builder.pop_str()?;
                builder.push_bool()?;
                builder.ops.push(TypedLoopOp::EqStr);
            }
            // `length(s::String)` — recognized only when the argument is
            // guaranteed to come off the string mini-stack (same
            // depth-underflow safety net as `ConcatStr`/`EqStr` above); a
            // `CallBuiltin(Length, 1)` on any other operand type rejects the
            // block via `pop_str()` returning `None`.
            Instr::CallBuiltin(subset_julia_vm_bytecode::BuiltinId::Length, 1)
                if function_params.is_none() =>
            {
                builder.pop_str()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::StrLen);
            }
            Instr::AddI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::AddI64);
            }
            Instr::SubI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::SubI64);
            }
            Instr::MulI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::MulI64);
            }
            // Issue #10567 (round 2): `Instr::DynamicAdd`/`DynamicSub`/
            // `DynamicMul` are the specializer's untyped fallback for a
            // binary op whose operand types are not both statically known
            // (e.g. a *proven* runtime-specialized callee's return type is
            // not yet trusted by `Stmt::AddAssign`'s conservative
            // `expr_might_produce_any` pre-check — the exact shape
            // `total += mandel_point(cr + ci*im, maxiter)` compiles to,
            // since `total`'s static type in the specializer is "unknown").
            // In LOOP MODE, by the time the recognizer reaches this
            // instruction, `builder.i64_depth` counts only values already
            // PROVEN i64 by another typed-loop op earlier in this exact
            // linear scan — a bare generic `LoadSlot` guesses i64 but is
            // itself backed by a real `frame.slot_i64()` type check at
            // execution time (bails if wrong), and a `CallSpecialize*`
            // result is only ever pushed onto the i64 stack when the
            // resolved callee's return type is I64. Nothing that is not
            // provably i64 can ever reach the i64 mini stack in this model
            // (a String/Struct/Array-producing instruction has no i64-stack
            // arm and would reject the whole loop instead) — so whenever
            // `pop_i64()` succeeds twice here, `Instr::DynamicAdd`'s actual
            // runtime operands ARE exactly those two i64 values, and
            // treating it as `AddI64` (same for Sub/Mul) is sound. Function
            // mode is NOT covered: `try_predecode_typed_scalar_function`'s
            // narrower single-loop-body contract does not carry the same
            // "everything on the i64 stack is i64-proven" invariant this
            // relies on, so we stay conservative there and fall through to
            // the catch-all reject. `DynamicDiv` is deliberately excluded —
            // Julia's `/` always promotes to Float64 even for two I64
            // operands, which is not `DivI64`. `DynamicMod`/`DynamicIntDiv`/
            // `DynamicPow` are also excluded: their overflow/edge-case
            // semantics are not simple I64 op reuse.
            //
            // Adversarial-review note (codex, Issue #10567 round 2): "does
            // `builder.i64_depth >= 2` really mean the REAL interpreter
            // stack's top two entries are those two i64 pushes, or could an
            // interleaved non-i64 value (e.g. `LoadSlotI64(a); LoadSlotF64(b);
            // LoadSlotI64(c); DynamicAdd`, silently computing `a + c` instead
            // of the real `b + c`) slip through?" That interleaved shape is
            // NOT reachable here. `emit(Instr::DynamicAdd)` has exactly two
            // call sites in `vm/specialize/stmt.rs`
            // (`Stmt::Assign`'s self-referential-Any branch and
            // `Stmt::AddAssign`'s `expr_might_produce_any` branch), and BOTH
            // compile to the SAME fixed shape: `Load(var); compile_expr(value)
            // [a single self-contained subexpression]; DynamicAdd` — `var` is
            // always the FIRST operand, and Julia expression compilation
            // always leaves a self-contained subexpression's result as
            // exactly one value on its own correctly-tracked stack (an F64
            // subexpression nets +1 on `f64_depth`, not `i64_depth`; this is
            // the same "every AddF64/AddI64 blindly pops its own stack"
            // invariant every OTHER typed-loop op already relies on, not a
            // new assumption). So at any reachable `DynamicAdd`,
            // `i64_depth` is exactly `1 (var) + {0 or 1} (RHS, iff RHS itself
            // resolved to i64)` — never 2 from unrelated sources, and never 2
            // when RHS is actually F64/Complex/String (those push their OWN
            // stacks, leaving `i64_depth == 1`, so the second `pop_i64()?`
            // fails and the whole loop safely rejects instead of misfiring).
            // `dynamic_struct_binary_instr`'s OTHER `DynamicAdd` emission
            // site (`emit_binary_op`'s Struct-operand fallback) has the same
            // "compile_expr(left); compile_expr(right)" two-self-contained-
            // operands shape, so the same argument applies; a Struct-typed
            // operand has no i64-stack arm at all and would reject the loop
            // before reaching here.
            Instr::DynamicAdd if function_params.is_none() => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::AddI64);
            }
            Instr::DynamicSub if function_params.is_none() => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::SubI64);
            }
            Instr::DynamicMul if function_params.is_none() => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::MulI64);
            }
            // Issue #8183: integer modulo (`%`) and fused `load slot; <op>` forms.
            Instr::ModI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::ModI64);
            }
            Instr::LoadAddI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadAddI64Slot(local));
            }
            Instr::LoadSubI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadSubI64Slot(local));
            }
            Instr::LoadMulI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadMulI64Slot(local));
            }
            Instr::LoadModI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadModI64Slot(local));
            }
            Instr::IndexStoreTyped(n) => {
                if *n != 1 {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                }
                // Issue #10566(c): the array being stored into is always the
                // top of `array_slot_stack` at this point — `LoadSlotArray`
                // is the only op that ever pushes an array (in lockstep with
                // `array_depth`), and `IndexStoreTyped` itself pops+repushes
                // the array value stack net zero without touching
                // `array_slot_stack`, so the invariant
                // `array_slot_stack.len() == array_depth` (and top-of-stack
                // provenance) always holds for an accepted block. Mark the
                // provenance local STORED so block entry resolves it through
                // a private transactional buffer (`ArrayWriteOrigin`) instead
                // of the frame's shared array.
                let Some(&array_slot) = builder.array_slot_stack.last() else {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                };
                if builder.f64_depth > 0 {
                    builder.pop_f64()?;
                    builder.pop_i64()?;
                    builder.pop_array()?;
                    builder.push_array()?;
                    builder.ops.push(TypedLoopOp::IndexStoreF64);
                } else {
                    builder.pop_i64()?;
                    builder.pop_i64()?;
                    builder.pop_array()?;
                    builder.push_array()?;
                    builder.ops.push(TypedLoopOp::IndexStoreI64);
                }
                let local = builder.read_array_slot(array_slot);
                builder.mark_array_slot_stored(local);
            }
            // Issue #10104: typed 1-D array element read (`x[i]`). Recognized for
            // read-only-array reduction loops (sum / dot / mean / norm, …). The
            // element type is fixed here from the array param's declared
            // `Vector{T}`; the executor re-checks it at runtime. Only the typed
            // load variants (emitted for statically-numeric arrays) qualify.
            Instr::IndexLoadTypedInbounds(1) | Instr::IndexLoadTyped(1) => {
                let Some(&array_slot) = builder.array_slot_stack.last() else {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                };
                let func =
                    *enclosing_fn.get_or_insert_with(|| enclosing_function(functions, header_ip));
                let Some(elem) = func.and_then(|f| param_array_element_type(f, array_slot)) else {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                };
                // pop index (i64) and the array; push the element on its stack.
                builder.pop_i64()?;
                builder.pop_array()?;
                builder.array_slot_stack.pop();
                match elem {
                    ArrayElementType::F64 => {
                        builder.push_f64()?;
                        builder.ops.push(TypedLoopOp::IndexLoadF64);
                    }
                    ArrayElementType::I64 => {
                        builder.push_i64()?;
                        builder.ops.push(TypedLoopOp::IndexLoadI64);
                    }
                    _ => {
                        *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                        return None;
                    }
                }
            }
            Instr::IncVarI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.read_i64_slot(*slot);
                builder.mark_i64_slot_written(local);
                builder.ops.push(TypedLoopOp::IncI64Slot(local));
            }
            Instr::DecVarI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.read_i64_slot(*slot);
                builder.mark_i64_slot_written(local);
                builder.ops.push(TypedLoopOp::DecI64Slot(local));
            }
            Instr::AddConstI64Slot(slot, delta) => {
                let local = builder.read_i64_slot(*slot);
                builder.mark_i64_slot_written(local);
                builder
                    .ops
                    .push(TypedLoopOp::AddConstI64Slot(local, *delta));
            }
            // Issue #9654: `push slot + delta` (the peephole-fused escape
            // return value `k - 1`).
            Instr::LoadAddConstI64Slot(slot, delta) => {
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder
                    .ops
                    .push(TypedLoopOp::LoadAddConstI64Slot(local, *delta));
            }
            // Issue #9654: early `return <i64>` from inside the loop body. It
            // leaves the loop (an exit for the no-other-exit check) and, like
            // the jump ops, requires the simulated stacks to be empty so the
            // linear stack-effect scan stays consistent across control flow.
            Instr::ReturnI64 => {
                builder.pop_i64()?;
                builder.ops.push(TypedLoopOp::ReturnI64);
                has_exit = true;
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            // Issue #10309: frame-less I64 function calls inside typed loops.
            // The callee is predecoded into `builder.i64_callees`; arguments are read
            // from typed-loop locals onto the i64 stack.
            Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => {
                let arg_count = operands.slots.len();
                let op = typed_loop_i64_call_op(
                    code,
                    functions,
                    base_function_count,
                    operands.func_index,
                    arg_count,
                    &mut builder.i64_callees,
                    &mut builder.typed_i64_callees,
                    allow_typed_fallback,
                )?;
                for slot in &operands.slots {
                    let local = builder.read_i64_slot(*slot);
                    builder.push_i64()?;
                    builder.ops.push(TypedLoopOp::LoadI64Slot(local));
                }
                for _ in 0..arg_count {
                    builder.pop_i64()?;
                }
                builder.push_i64()?;
                builder.ops.push(op);
            }
            // Issue #10439: a `CallSpecializeI64Slots` site inside a typed loop
            // reaches an *untyped* callee whose I64 body is a runtime
            // specialization. We cannot predecode it here (it may not exist yet),
            // so we only record `(spec_func_index, arg_count)` and read the
            // argument slots onto the i64 stack; `execute_typed_loop_block`
            // resolves the callee body against the live specialization cache at
            // run time. Loop mode only (`function_params.is_none()`): typed scalar
            // *function* blocks execute frame-lessly without the caller's
            // `&mut self`, so they cannot resolve a runtime specialization and
            // keep rejecting these sites (they fall to the frame path unchanged).
            Instr::CallSpecializeI64Slots(operands)
            | Instr::CallSpecializeInboundsI64Slots(operands)
                if function_params.is_none() =>
            {
                let arg_count = operands.slots.len();
                // Bound the number of distinct specialize sites, mirroring the
                // predecoded-callee cap so the per-execution resolution vec stays
                // small.
                if arg_count == 0 || builder.specialize_callees.len() >= I64_FUNCTION_CALLEE_CAP {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                }
                let scratch_index = builder.specialize_callees.len();
                builder
                    .specialize_callees
                    .push((operands.spec_func_index, arg_count));
                for slot in &operands.slots {
                    let local = builder.read_i64_slot(*slot);
                    builder.push_i64()?;
                    builder.ops.push(TypedLoopOp::LoadI64Slot(local));
                }
                for _ in 0..arg_count {
                    builder.pop_i64()?;
                }
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::CallSpecializeI64Function(
                    scratch_index,
                    arg_count,
                ));
            }
            // Issue #10491: Float64 mirror of the `CallSpecializeI64Slots` arm
            // above — record the `(spec_func_index, arg_count)` site, read the
            // F64 argument slots onto the f64 stack, and resolve the callee's
            // frame-less F64 body against the live specialization cache at run
            // time. Loop mode only, for the same reason as the I64 arm.
            Instr::CallSpecializeF64Slots(operands)
            | Instr::CallSpecializeInboundsF64Slots(operands)
                if function_params.is_none() =>
            {
                let arg_count = operands.slots.len();
                if arg_count == 0 || builder.specialize_f64_callees.len() >= F64_FUNCTION_CALLEE_CAP
                {
                    *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                    return None;
                }
                let scratch_index = builder.specialize_f64_callees.len();
                builder
                    .specialize_f64_callees
                    .push((operands.spec_func_index, arg_count));
                for slot in &operands.slots {
                    let local = builder.read_f64_slot(*slot);
                    builder.push_f64()?;
                    builder.ops.push(TypedLoopOp::LoadF64Slot(local));
                }
                for _ in 0..arg_count {
                    builder.pop_f64()?;
                }
                builder.push_f64()?;
                builder.ops.push(TypedLoopOp::CallSpecializeF64Function(
                    scratch_index,
                    arg_count,
                ));
            }
            // Issue #10309 follow-up: frame-less F64 function calls inside typed
            // loops. The callee is predecoded into `builder.f64_callees`; arguments
            // are already on the f64 stack from preceding instructions.
            Instr::Call(target_index, arg_count)
            | Instr::CallInbounds(target_index, arg_count)
            | Instr::CallResolved(target_index, arg_count)
                if builder.f64_depth >= *arg_count =>
            {
                let op = typed_loop_f64_call_op(
                    code,
                    functions,
                    base_function_count,
                    *target_index,
                    *arg_count,
                    &mut builder.f64_callees,
                    &mut builder.typed_f64_callees,
                    allow_typed_fallback,
                )?;
                for _ in 0..*arg_count {
                    builder.pop_f64()?;
                }
                builder.push_f64()?;
                builder.ops.push(op);
            }
            Instr::EqI64
            | Instr::NeI64
            | Instr::LtI64
            | Instr::GtI64
            | Instr::LeI64
            | Instr::GeI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_bool()?;
                builder.ops.push(TypedLoopOp::CmpI64(i64_relation(instr)?));
            }
            Instr::EqF64
            | Instr::NeF64
            | Instr::LtF64
            | Instr::GtF64
            | Instr::LeF64
            | Instr::GeF64 => {
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder.push_bool()?;
                builder.ops.push(TypedLoopOp::CmpF64(f64_relation(instr)?));
            }
            Instr::JumpIfZero(target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                builder.pop_bool()?;
                builder.ops.push(TypedLoopOp::JumpIfZero(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfEqI64(target)
            | Instr::JumpIfNeI64(target)
            | Instr::JumpIfLtI64(target)
            | Instr::JumpIfGtI64(target)
            | Instr::JumpIfLeI64(target)
            | Instr::JumpIfGeI64(target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder
                    .ops
                    .push(TypedLoopOp::JumpIfI64(i64_relation(instr)?, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                let lhs_local = builder.read_i64_slot(*lhs_slot);
                let rhs_local = builder.read_i64_slot(*rhs_slot);
                builder.ops.push(TypedLoopOp::JumpIfI64Slots(
                    lhs_local,
                    rhs_local,
                    I64Relation::Gt,
                    target,
                ));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                let local = builder.read_i64_slot(*slot);
                let stop_local = builder.read_i64_slot(*stop_slot);
                builder.mark_i64_slot_written(local);
                builder.ops.push(TypedLoopOp::AddConstI64SlotAndJumpIfLe(
                    local, *delta, stop_local, target,
                ));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            // Fused slot-vs-constant compare-and-branch (Issue #10105). The
            // peephole pass folds `LoadSlotI64(slot); PushI64(konst); <cmp>I64;
            // JumpIfZero` — the canonical constant-bounded loop guard — into one
            // instruction. To keep such loops on the typed-loop fast path,
            // reconstruct the *identical* typed-loop IR the recognizer would have
            // emitted for the un-fused sequence: `LoadI64Slot` + `PushI64` +
            // `JumpIfI64`. `cmp` is already the branch predicate (the inverse of
            // the source comparison), matching the Pattern-3-fused directional
            // jump this maps to, so the reconstruction is byte-for-byte the
            // pre-fusion IR.
            Instr::JumpIfCmpI64SlotConst(slot, konst, cmp, target) => {
                let local = builder.read_i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::LoadI64Slot(local));
                builder.push_i64()?;
                builder.ops.push(TypedLoopOp::PushI64(*konst));
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                let relation = match cmp {
                    subset_julia_vm_bytecode::I64Cmp::Eq => I64Relation::Eq,
                    subset_julia_vm_bytecode::I64Cmp::Ne => I64Relation::Ne,
                    subset_julia_vm_bytecode::I64Cmp::Lt => I64Relation::Lt,
                    subset_julia_vm_bytecode::I64Cmp::Gt => I64Relation::Gt,
                    subset_julia_vm_bytecode::I64Cmp::Le => I64Relation::Le,
                    subset_julia_vm_bytecode::I64Cmp::Ge => I64Relation::Ge,
                };
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.ops.push(TypedLoopOp::JumpIfI64(relation, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfEqF64(target) | Instr::JumpIfNeF64(target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder
                    .ops
                    .push(TypedLoopOp::JumpIfF64(f64_relation(instr)?, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfNotLtF64(target)
            | Instr::JumpIfNotGtF64(target)
            | Instr::JumpIfNotLeF64(target)
            | Instr::JumpIfNotGeF64(target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                builder.pop_f64()?;
                builder.pop_f64()?;
                builder
                    .ops
                    .push(TypedLoopOp::JumpIfNotF64(not_f64_relation(instr)?, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::Jump(target) => {
                let target = typed_loop_target(header_ip, end_ip, *target, &mut has_exit)?;
                builder.ops.push(TypedLoopOp::Jump(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            _ => {
                *reject = Some(TypedLoopReject::UnsupportedInstr(ip));
                return None;
            }
        }
    }

    if !has_exit {
        *reject = Some(TypedLoopReject::NoExit);
        return None;
    }
    // Issue #10504 transactionality guard (generalizing the #10104 IndexLoad
    // guard and the #10439 specialize-call guard): a data-dependent BAIL
    // discards the buffered slot state and re-runs the WHOLE block on the
    // generic interpreter from the loop header. Frame slots are buffered and
    // written back only on clean completion, so re-running is safe for them —
    // but `RandF64` advances the RNG directly, so a bail after one would
    // double-apply the side effect (e.g. re-drawing every random number of the
    // aborted run; `typemin % -1` even bails SILENTLY — the generic re-run
    // yields 0 without an error). The scalar-function predecoder already
    // rejects `RandF64` for the same reason (PR #9733). Rather than prove no
    // bail can follow a side effect, reject any block that mixes a
    // bail-capable op with an in-place side-effecting op; such loops stay
    // fully on the generic interpreter, which is transactionally correct.
    // As of Issue #10566(c), `IndexStore*` is no longer in this class (its
    // write lands in a discardable transactional buffer — see
    // `ArrayWriteOrigin` — committed to the origin only alongside every other
    // buffered local), which is exactly what lets a block mix an array store
    // with a bail-capable `IndexLoad*` (`y[i] = x[i] + 1`-shaped map/copy
    // loops) or another bail-capable op. `RandF64` remains the one truly
    // irreversible op this guard exists for.
    let effects = typed_loop_effects(&builder.ops);
    if effects.out_of_buffer_effect && effects.bail_capable {
        *reject = Some(TypedLoopReject::UnsupportedInstr(header_ip));
        return None;
    }
    if builder.array_slots.len() > TYPED_LOOP_SLOT_CAP
        || builder.f64_slots.len() > TYPED_LOOP_SLOT_CAP
        || builder.i64_slots.len() > TYPED_LOOP_SLOT_CAP
        || builder.str_slots.len() > TYPED_LOOP_SLOT_CAP
    {
        *reject = Some(TypedLoopReject::SlotCountOverCap);
        return None;
    }
    remap_typed_loop_op_targets(&mut builder.ops, header_ip, &ip_to_first_op);
    // Issue #10565: the ops are final here (fusion applied), so certify once —
    // see `TypedLoopBlock::ops_trusted`.
    let ops = fuse_complex_mul_add_assign(fuse_typed_loop_ops(builder.ops));
    let ops_trusted = certify_typed_ops_trusted(&ops);
    Some((
        TypedLoopBlock {
            exit_ip: end_ip,
            array_slots: builder.array_slots,
            f64_slots: builder.f64_slots,
            i64_slots: builder.i64_slots,
            str_slots: builder.str_slots,
            str_consts: builder.str_consts,
            ops,
            i64_callees: builder.i64_callees,
            f64_callees: builder.f64_callees,
            specialize_callees: builder.specialize_callees,
            specialize_f64_callees: builder.specialize_f64_callees,
            specialize_complex_i64_callees: builder.specialize_complex_i64_callees,
            typed_i64_callees: builder.typed_i64_callees,
            typed_f64_callees: builder.typed_f64_callees,
            ops_trusted,
        },
        builder.complex_slots,
    ))
}

/// Remap every `TypedLoopTarget::Op` target from bytecode-relative indices to
/// final typed-op indices. `TypedLoopTarget::Op` values are encoded as
/// `target_ip - header_ip` during predecode; after instructions such as
/// `CallResolvedI64Slots` expand into multiple typed ops, those raw offsets no
/// longer match the op list. `ip_to_first_op[ip - header_ip]` gives the first
/// typed-op index emitted for that bytecode ip (Issue #10309).
fn remap_typed_loop_op_targets(
    ops: &mut [TypedLoopOp],
    header_ip: usize,
    ip_to_first_op: &[usize],
) {
    for op in ops {
        let target = match op {
            TypedLoopOp::JumpIfZero(t)
            | TypedLoopOp::JumpIfI64(_, t)
            | TypedLoopOp::JumpIfI64Slots(_, _, _, t)
            | TypedLoopOp::AddConstI64SlotAndJumpIfLe(_, _, _, t)
            | TypedLoopOp::JumpIfF64(_, t)
            | TypedLoopOp::JumpIfNotF64(_, t)
            | TypedLoopOp::JumpIfNotF64Const(_, _, t)
            | TypedLoopOp::Jump(t) => t,
            _ => continue,
        };
        if let TypedLoopTarget::Op(rel) = target {
            let rel = *rel;
            let target_ip = header_ip + rel;
            let idx = target_ip - header_ip;
            if idx < ip_to_first_op.len() {
                *target = TypedLoopTarget::Op(ip_to_first_op[idx]);
            }
        }
    }
}

/// Predecode-time peephole over the typed-loop op list (Issue #9654): fuse
/// common load/op/store windows into single 3-address superinstructions. Each
/// fusion is stack-effect-equivalent to the window it replaces, a window is
/// only fused when no jump lands on its interior ops, and every `Op(i)` jump
/// target is remapped to the fused indices afterwards. General: applies to
/// every typed loop (escape kernels, ODE steps, LCG maps, Monte-Carlo loops).
fn fuse_typed_loop_ops(ops: Vec<TypedLoopOp>) -> Vec<TypedLoopOp> {
    use TypedLoopOp as Op;

    fn op_target(op: &TypedLoopOp) -> Option<TypedLoopTarget> {
        match op {
            Op::JumpIfZero(t)
            | Op::JumpIfI64(_, t)
            | Op::JumpIfI64Slots(_, _, _, t)
            | Op::AddConstI64SlotAndJumpIfLe(_, _, _, t)
            | Op::JumpIfF64(_, t)
            | Op::JumpIfNotF64(_, t)
            | Op::JumpIfNotF64Const(_, _, t)
            | Op::Jump(t) => Some(*t),
            _ => None,
        }
    }

    // Op indices some jump lands on: a fused window must not swallow one.
    let mut is_target = vec![false; ops.len() + 1];
    for op in &ops {
        if let Some(TypedLoopTarget::Op(i)) = op_target(op) {
            if i < is_target.len() {
                is_target[i] = true;
            }
        }
    }

    let mut fused: Vec<Op> = Vec::with_capacity(ops.len());
    let mut index_map = vec![0usize; ops.len() + 1];
    let mut i = 0;
    while i < ops.len() {
        index_map[i] = fused.len();
        let free2 = i + 1 < ops.len() && !is_target[i + 1];
        let free3 = free2 && i + 2 < ops.len() && !is_target[i + 2];
        // Widest window first (maximal munch).
        let (op, consumed) = match (ops.get(i), ops.get(i + 1), ops.get(i + 2)) {
            (Some(Op::LoadSquareF64Slot(a)), Some(Op::LoadSquareF64Slot(b)), Some(Op::AddF64))
                if free3 =>
            {
                (Op::PushSumSquaresF64Slots(*a, *b), 3)
            }
            (Some(Op::LoadSquareF64Slot(a)), Some(Op::LoadSquareF64Slot(b)), Some(Op::SubF64))
                if free3 =>
            {
                (Op::PushDiffSquaresF64Slots(*a, *b), 3)
            }
            (Some(Op::LoadF64Slot(a)), Some(Op::LoadMulF64Slot(b)), _) if free2 => {
                (Op::PushMulF64Slots(*a, *b), 2)
            }
            (Some(Op::LoadF64Slot(src)), Some(Op::StoreF64Slot(dst)), _) if free2 => {
                (Op::CopyF64Slots(*dst, *src), 2)
            }
            (Some(Op::LoadI64Slot(src)), Some(Op::StoreI64Slot(dst)), _) if free2 => {
                (Op::CopyI64Slots(*dst, *src), 2)
            }
            (Some(Op::LoadAddF64Slot(src)), Some(Op::StoreF64Slot(dst)), _) if free2 => {
                (Op::AddF64SlotStore(*src, *dst), 2)
            }
            (Some(Op::PushF64(c)), Some(Op::JumpIfNotF64(rel, t)), _) if free2 => {
                (Op::JumpIfNotF64Const(*rel, *c, *t), 2)
            }
            // Issue #9693: fused ComplexF64 param field extraction
            // (the SROA param-hoist preamble of typed function blocks).
            (
                Some(Op::PushComplexParam(p)),
                Some(Op::ComplexFieldF64(f)),
                Some(Op::StoreF64Slot(d)),
            ) if free3 => (Op::StoreComplexParamFieldF64(*p, *f, *d), 3),
            (Some(op), _, _) => (*op, 1),
            (None, _, _) => break,
        };
        // Interior indices map to the fused op (nothing jumps there — checked).
        for k in 1..consumed {
            index_map[i + k] = fused.len();
        }
        fused.push(op);
        i += consumed;
    }
    index_map[ops.len()] = fused.len();

    // Remap `Op(i)` jump targets to the fused indices.
    for op in &mut fused {
        let target = match op {
            Op::JumpIfZero(t)
            | Op::JumpIfI64(_, t)
            | Op::JumpIfI64Slots(_, _, _, t)
            | Op::AddConstI64SlotAndJumpIfLe(_, _, _, t)
            | Op::JumpIfF64(_, t)
            | Op::JumpIfNotF64(_, t)
            | Op::JumpIfNotF64Const(_, _, t)
            | Op::Jump(t) => t,
            _ => continue,
        };
        if let TypedLoopTarget::Op(idx) = target {
            *target = TypedLoopTarget::Op(index_map[*idx]);
        }
    }
    fused
}

/// Issue #10532: predecode-time fusion for the SROA'd Complex{Float64}
/// recurrence `z = z*z + c`. After `fuse_typed_loop_ops` has reduced the
/// sub-expressions, the update becomes an 8-op window that this pass collapses
/// into a single `ComplexMulAddAssign` superinstruction.
fn fuse_complex_mul_add_assign(ops: Vec<TypedLoopOp>) -> Vec<TypedLoopOp> {
    use TypedLoopOp as Op;

    fn op_target(op: &TypedLoopOp) -> Option<TypedLoopTarget> {
        match op {
            Op::JumpIfZero(t)
            | Op::JumpIfI64(_, t)
            | Op::JumpIfI64Slots(_, _, _, t)
            | Op::AddConstI64SlotAndJumpIfLe(_, _, _, t)
            | Op::JumpIfF64(_, t)
            | Op::JumpIfNotF64(_, t)
            | Op::JumpIfNotF64Const(_, _, t)
            | Op::Jump(t) => Some(*t),
            _ => None,
        }
    }

    let mut is_target = vec![false; ops.len() + 1];
    for op in &ops {
        if let Some(TypedLoopTarget::Op(i)) = op_target(op) {
            if i < is_target.len() {
                is_target[i] = true;
            }
        }
    }

    let mut fused: Vec<Op> = Vec::with_capacity(ops.len());
    let mut index_map = vec![0usize; ops.len() + 1];
    let mut i = 0;
    while i < ops.len() {
        index_map[i] = fused.len();
        let free_interior = (1..8).all(|k| i + k >= ops.len() || !is_target[i + k]);
        let window = ops.get(i..i + 8);
        let matched = if free_interior {
            match window {
                Some(
                    [Op::PushDiffSquaresF64Slots(z_re, z_im), Op::AddF64SlotStore(c_re, t0_re), Op::PushMulF64Slots(m1_a, m1_b), Op::PushMulF64Slots(m2_a, m2_b), Op::AddF64, Op::AddF64SlotStore(c_im, t0_im), Op::CopyF64Slots(copy_z_re, copy_t0_re), Op::CopyF64Slots(copy_z_im, copy_t0_im)],
                ) if *z_re == *m1_a
                    && *z_im == *m1_b
                    && *z_im == *m2_a
                    && *z_re == *m2_b
                    && *z_re == *copy_z_re
                    && *z_im == *copy_z_im
                    && *t0_re == *copy_t0_re
                    && *t0_im == *copy_t0_im =>
                {
                    Some(Op::ComplexMulAddAssign {
                        z_re: *z_re,
                        z_im: *z_im,
                        c_re: *c_re,
                        c_im: *c_im,
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        if let Some(op) = matched {
            for k in 1..8 {
                index_map[i + k] = fused.len();
            }
            fused.push(op);
            i += 8;
        } else {
            fused.push(ops[i]);
            i += 1;
        }
    }
    index_map[ops.len()] = fused.len();

    for op in &mut fused {
        let target = match op {
            Op::JumpIfZero(t)
            | Op::JumpIfI64(_, t)
            | Op::JumpIfI64Slots(_, _, _, t)
            | Op::AddConstI64SlotAndJumpIfLe(_, _, _, t)
            | Op::JumpIfF64(_, t)
            | Op::JumpIfNotF64(_, t)
            | Op::JumpIfNotF64Const(_, _, t)
            | Op::Jump(t) => t,
            _ => continue,
        };
        if let TypedLoopTarget::Op(idx) = target {
            *target = TypedLoopTarget::Op(index_map[*idx]);
        }
    }
    fused
}

/// Predecode a whole function body into a frame-less typed scalar function
/// block (Issue #9693). Requirements: every instruction in the typed op set
/// (function mode: ComplexF64 param decompose windows and `ReturnF64`
/// allowed), scalar-only (no array slots), every live-in slot is a parameter,
/// and no jump leaves the body (all exits are `Return*` ops — an `Exit`
/// target would fall past the function's end).
pub(crate) fn try_predecode_typed_scalar_function(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    entry: usize,
    end: usize,
    base_function_count: usize,
    param_slots: &[usize],
) -> Option<TypedScalarFunctionBlock> {
    try_predecode_typed_scalar_function_inner(
        code,
        functions,
        entry,
        end,
        base_function_count,
        param_slots,
        true,
    )
}

/// Issue #10542: `allow_typed_fallback`-threading twin of
/// [`try_predecode_typed_scalar_function`]. The public function always
/// allows the fallback for the callee's OWN direct-call sites; the
/// direct-call recognizer (`typed_loop_i64_call_op` / `typed_loop_f64_call_op`)
/// calls this directly with `allow_typed_fallback = false` when it is itself
/// resolving a fallback callee, bounding the new mixed-type recursion to one
/// extra level.
fn try_predecode_typed_scalar_function_inner(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    entry: usize,
    end: usize,
    base_function_count: usize,
    param_slots: &[usize],
    allow_typed_fallback: bool,
) -> Option<TypedScalarFunctionBlock> {
    let mut reject = None;
    let (block, complex_slots) = try_predecode_typed_ops_range(
        code,
        functions,
        entry,
        end,
        base_function_count,
        Some(param_slots),
        &mut reject,
        allow_typed_fallback,
    )?;
    if !block.array_slots.is_empty() {
        return None;
    }
    // Reject side-effecting bodies (PR #9733 review): `RandF64` advances the
    // RNG, and every caller treats a `Bail` as "safe to re-run the frame
    // path" — a body that consumes RNG values and *then* bails (e.g. at
    // `checked_i64_rem` or an uninit-slot guard) would make the fallback
    // observe a different random sequence than a pure frame execution.
    // Rather than proving no bail can follow the side effect, keep function
    // blocks effect-free. (Array stores are already excluded above.)
    if block
        .ops
        .iter()
        .any(|op| matches!(op, TypedLoopOp::RandF64))
    {
        return None;
    }
    if block.ops.iter().any(|op| {
        matches!(
            op,
            TypedLoopOp::JumpIfZero(TypedLoopTarget::Exit)
                | TypedLoopOp::JumpIfI64(_, TypedLoopTarget::Exit)
                | TypedLoopOp::JumpIfI64Slots(_, _, _, TypedLoopTarget::Exit)
                | TypedLoopOp::AddConstI64SlotAndJumpIfLe(_, _, _, TypedLoopTarget::Exit)
                | TypedLoopOp::JumpIfF64(_, TypedLoopTarget::Exit)
                | TypedLoopOp::JumpIfNotF64(_, TypedLoopTarget::Exit)
                | TypedLoopOp::JumpIfNotF64Const(_, _, TypedLoopTarget::Exit)
                | TypedLoopOp::Jump(TypedLoopTarget::Exit)
        )
    }) {
        return None;
    }

    // Bind each parameter: a slot read via the complex windows is a ComplexF64
    // param; a live-in scalar slot binds by its typed local; a slot the body
    // only writes (or never touches) is Unused. Ambiguous usage bails.
    let mut params = Vec::with_capacity(param_slots.len());
    for slot in param_slots {
        let complex = complex_slots.iter().position(|s| s == slot);
        let f64_local = block.f64_slots.iter().position(|t| t.slot == *slot);
        let i64_local = block.i64_slots.iter().position(|t| t.slot == *slot);
        let binding = match (complex, f64_local, i64_local) {
            (Some(c), None, None) => TypedFunctionParamBinding::ComplexF64(c),
            (None, Some(local), None) => {
                if block.f64_slots[local].live_in {
                    TypedFunctionParamBinding::F64(local)
                } else {
                    TypedFunctionParamBinding::Unused
                }
            }
            (None, None, Some(local)) => {
                if block.i64_slots[local].live_in {
                    TypedFunctionParamBinding::I64(local)
                } else {
                    TypedFunctionParamBinding::Unused
                }
            }
            (None, None, None) => TypedFunctionParamBinding::Unused,
            _ => return None,
        };
        params.push(binding);
    }
    // Any other live-in slot would read a local the frame path considers
    // undefined — the block cannot reproduce UndefVarError, so bail.
    for t in block.f64_slots.iter().chain(block.i64_slots.iter()) {
        if t.live_in && !param_slots.contains(&t.slot) {
            return None;
        }
    }
    // Issue #10559: `TypedScalarFunctionBlock` keeps only `ops` — it carries no
    // String slot/const state, and its `TypedOpsState::new(0, 0)` has empty
    // String vectors. The recognizer already gates every String op on
    // `function_params.is_none()`, so this is unreachable; keep it as a
    // structural backstop so a future String op that forgets the gate degrades
    // to the generic interpreter instead of indexing an empty vector.
    if !block.str_slots.is_empty() || !block.str_consts.is_empty() {
        return None;
    }

    let ops_trusted = certify_typed_ops_trusted(&block.ops);
    Some(TypedScalarFunctionBlock {
        params,
        ops: block.ops,
        i64_callees: block.i64_callees,
        f64_callees: block.f64_callees,
        typed_i64_callees: block.typed_i64_callees,
        typed_f64_callees: block.typed_f64_callees,
        ops_trusted,
    })
}

fn typed_loop_target(
    header_ip: usize,
    end_ip: usize,
    target_ip: usize,
    has_exit: &mut bool,
) -> Option<TypedLoopTarget> {
    if target_ip == end_ip {
        *has_exit = true;
        return Some(TypedLoopTarget::Exit);
    }
    if target_ip == header_ip {
        return Some(TypedLoopTarget::LoopBack);
    }
    if target_ip > header_ip && target_ip < end_ip {
        return Some(TypedLoopTarget::Op(target_ip - header_ip));
    }
    None
}

/// Remap every op-index target in an I64 function op list from bytecode-relative
/// indices to final I64 op indices. Mirrors [`remap_typed_loop_op_targets`] for
/// the frame-less I64 function IR (Issue #10309).
/// Remap every op-index target in a scalar function op list from
/// bytecode-relative indices to final op indices (Issue #10427; generic over
/// the operand element type `S`). Shared by the i64 and f64 predecoders.
fn remap_scalar_function_op_targets<S>(
    ops: &mut [ScalarFunctionOp<S>],
    entry_ip: usize,
    ip_to_first_op: &[usize],
) {
    for op in ops {
        let target = match op {
            ScalarFunctionOp::JumpIfZero(t)
            | ScalarFunctionOp::JumpIf(_, t)
            | ScalarFunctionOp::JumpIfSlots(_, _, _, t)
            | ScalarFunctionOp::AddConstSlotAndJumpIfLe(_, _, _, t)
            | ScalarFunctionOp::Jump(t) => t,
            _ => continue,
        };
        let target_ip = entry_ip + *target;
        let idx = target_ip - entry_ip;
        if idx < ip_to_first_op.len() {
            *target = ip_to_first_op[idx];
        }
    }
}

pub(crate) fn try_predecode_i64_function(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    entry_ip: usize,
    end_ip: usize,
    param_slots: &[usize],
) -> Option<I64FunctionBlock> {
    try_predecode_i64_function_inner(
        code,
        functions,
        base_function_count,
        entry_ip,
        end_ip,
        param_slots,
        0,
        &[],
    )
}

fn try_predecode_i64_function_inner(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    entry_ip: usize,
    end_ip: usize,
    param_slots: &[usize],
    depth: usize,
    visiting_entries: &[usize],
) -> Option<I64FunctionBlock> {
    if end_ip <= entry_ip || end_ip > code.len() || end_ip - entry_ip > MAX_I64_FUNCTION_OPS {
        return None;
    }
    if depth > MAX_I64_FUNCTION_CALL_DEPTH || visiting_entries.contains(&entry_ip) {
        return None;
    }
    let mut nested_visiting_entries = visiting_entries.to_vec();
    nested_visiting_entries.push(entry_ip);

    let mut builder = ScalarFunctionBuilder::<i64>::new(param_slots);
    let mut has_return = false;
    let mut ip_to_first_op = Vec::with_capacity(end_ip - entry_ip);
    for ip in entry_ip..end_ip {
        ip_to_first_op.push(builder.ops.len());
        let instr = code.get(ip)?;
        match instr {
            Instr::PushI64(value) => {
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Push(*value));
            }
            Instr::LoadSlot(slot) | Instr::LoadSlotI64(slot) => {
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadSlot(local));
            }
            Instr::StoreSlot(slot) | Instr::StoreSlotI64(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.ops.push(ScalarFunctionOp::StoreSlot(local));
            }
            Instr::AddI64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Add);
            }
            Instr::SubI64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Sub);
            }
            Instr::MulI64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Mul);
            }
            Instr::ModI64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Rem);
            }
            Instr::LoadAddI64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadAddSlot(local));
            }
            Instr::LoadSubI64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadSubSlot(local));
            }
            Instr::LoadMulI64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadMulSlot(local));
            }
            Instr::LoadModI64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadRemSlot(local));
            }
            Instr::IncVarI64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.ops.push(ScalarFunctionOp::IncSlot(local));
            }
            Instr::DecVarI64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.ops.push(ScalarFunctionOp::DecSlot(local));
            }
            Instr::AddConstI64Slot(slot, delta) => {
                let local = builder.slot(*slot);
                builder
                    .ops
                    .push(ScalarFunctionOp::AddConstSlot(local, *delta));
            }
            Instr::EqI64
            | Instr::NeI64
            | Instr::LtI64
            | Instr::GtI64
            | Instr::LeI64
            | Instr::GeI64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_bool()?;
                builder
                    .ops
                    .push(ScalarFunctionOp::Cmp(i64_relation(instr)?));
            }
            Instr::CallDynamicBinaryBoth(intrinsic, _) => {
                if let Some(op) = i64_function_arithmetic_intrinsic(intrinsic) {
                    builder.pop_value()?;
                    builder.pop_value()?;
                    builder.push_value()?;
                    builder.ops.push(op);
                } else {
                    let relation = i64_function_relation_intrinsic(intrinsic)?;
                    builder.pop_value()?;
                    builder.pop_value()?;
                    builder.push_bool()?;
                    builder.ops.push(ScalarFunctionOp::Cmp(relation));
                }
            }
            Instr::Call(target_index, arg_count)
            | Instr::CallInbounds(target_index, arg_count)
            | Instr::CallResolved(target_index, arg_count) => {
                let op = i64_function_call_op(
                    code,
                    functions,
                    base_function_count,
                    *target_index,
                    *arg_count,
                    depth,
                    &nested_visiting_entries,
                    &mut builder,
                )?;
                for _ in 0..*arg_count {
                    builder.pop_value()?;
                }
                builder.push_value()?;
                builder.ops.push(op);
            }
            Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => {
                let arg_count = operands.slots.len();
                let op = i64_function_call_op(
                    code,
                    functions,
                    base_function_count,
                    operands.func_index,
                    arg_count,
                    depth,
                    &nested_visiting_entries,
                    &mut builder,
                )?;
                for slot in &operands.slots {
                    let local = builder.slot(*slot);
                    builder.push_value()?;
                    builder.ops.push(ScalarFunctionOp::LoadSlot(local));
                }
                for _ in 0..arg_count {
                    builder.pop_value()?;
                }
                builder.push_value()?;
                builder.ops.push(op);
            }
            Instr::JumpIfZero(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.pop_bool()?;
                builder.ops.push(ScalarFunctionOp::JumpIfZero(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfEqI64(target)
            | Instr::JumpIfNeI64(target)
            | Instr::JumpIfLtI64(target)
            | Instr::JumpIfGtI64(target)
            | Instr::JumpIfLeI64(target)
            | Instr::JumpIfGeI64(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.pop_value()?;
                builder.pop_value()?;
                builder
                    .ops
                    .push(ScalarFunctionOp::JumpIf(i64_relation(instr)?, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                let lhs_local = builder.slot(*lhs_slot);
                let rhs_local = builder.slot(*rhs_slot);
                builder.ops.push(ScalarFunctionOp::JumpIfSlots(
                    lhs_local,
                    rhs_local,
                    ScalarRelation::Gt,
                    target,
                ));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            // Fused slot-vs-constant compare-and-branch (Issue #10105).
            // Reconstruct the identical I64-function IR the recognizer would have
            // emitted for the un-fused `LoadSlotI64(slot); PushI64(konst);
            // <cmp>I64; JumpIfZero` guard (`LoadI64Slot` + `PushI64` +
            // `JumpIfI64`), so constant-bounded functions stay on the whole-
            // function I64 fast path. `cmp` is already the branch predicate.
            Instr::JumpIfCmpI64SlotConst(slot, konst, cmp, target) => {
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadSlot(local));
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Push(*konst));
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                let relation = match cmp {
                    subset_julia_vm_bytecode::I64Cmp::Eq => ScalarRelation::Eq,
                    subset_julia_vm_bytecode::I64Cmp::Ne => ScalarRelation::Ne,
                    subset_julia_vm_bytecode::I64Cmp::Lt => ScalarRelation::Lt,
                    subset_julia_vm_bytecode::I64Cmp::Gt => ScalarRelation::Gt,
                    subset_julia_vm_bytecode::I64Cmp::Le => ScalarRelation::Le,
                    subset_julia_vm_bytecode::I64Cmp::Ge => ScalarRelation::Ge,
                };
                builder.pop_value()?;
                builder.pop_value()?;
                builder.ops.push(ScalarFunctionOp::JumpIf(relation, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                let local = builder.slot(*slot);
                let stop_local = builder.slot(*stop_slot);
                builder.ops.push(ScalarFunctionOp::AddConstSlotAndJumpIfLe(
                    local, *delta, stop_local, target,
                ));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::Jump(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.ops.push(ScalarFunctionOp::Jump(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::ReturnI64 => {
                builder.pop_value()?;
                if !builder.stack_is_empty() {
                    return None;
                }
                builder.ops.push(ScalarFunctionOp::Return);
                has_return = true;
            }
            _ => return None,
        }
    }

    if !has_return || builder.slots.len() > I64_FUNCTION_SLOT_CAP {
        return None;
    }
    remap_scalar_function_op_targets(&mut builder.ops, entry_ip, &ip_to_first_op);
    Some(I64FunctionBlock {
        slots: builder.slots,
        ops: builder.ops,
        callees: builder.callees,
    })
}

pub(crate) fn try_predecode_f64_function(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    entry_ip: usize,
    end_ip: usize,
    param_slots: &[usize],
) -> Option<F64FunctionBlock> {
    try_predecode_f64_function_inner(
        code,
        functions,
        base_function_count,
        entry_ip,
        end_ip,
        param_slots,
        0,
        &[],
    )
}

fn try_predecode_f64_function_inner(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    entry_ip: usize,
    end_ip: usize,
    param_slots: &[usize],
    depth: usize,
    visiting_entries: &[usize],
) -> Option<F64FunctionBlock> {
    if end_ip <= entry_ip || end_ip > code.len() || end_ip - entry_ip > MAX_F64_FUNCTION_OPS {
        return None;
    }
    if depth > MAX_F64_FUNCTION_CALL_DEPTH || visiting_entries.contains(&entry_ip) {
        return None;
    }
    let mut nested_visiting_entries = visiting_entries.to_vec();
    nested_visiting_entries.push(entry_ip);

    let mut builder = ScalarFunctionBuilder::<f64>::new(param_slots);
    let mut has_return = false;
    let mut ip_to_first_op = Vec::with_capacity(end_ip - entry_ip);
    for ip in entry_ip..end_ip {
        ip_to_first_op.push(builder.ops.len());
        let instr = code.get(ip)?;
        match instr {
            Instr::PushF64(value) => {
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Push(*value));
            }
            Instr::PushI64(value) => {
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Push(*value as f64));
            }
            Instr::LoadSlot(slot) | Instr::LoadSlotF64(slot) => {
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadSlot(local));
            }
            Instr::StoreSlot(slot) | Instr::StoreSlotF64(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.ops.push(ScalarFunctionOp::StoreSlot(local));
            }
            Instr::AddF64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Add);
            }
            Instr::SubF64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Sub);
            }
            Instr::MulF64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Mul);
            }
            Instr::DivF64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Div);
            }
            Instr::LoadAddF64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadAddSlot(local));
            }
            Instr::LoadSubF64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadSubSlot(local));
            }
            Instr::LoadMulF64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadMulSlot(local));
            }
            Instr::LoadDivF64Slot(slot) => {
                builder.pop_value()?;
                let local = builder.slot(*slot);
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::LoadDivSlot(local));
            }
            Instr::NegF64 | Instr::CallIntrinsic(Intrinsic::NegFloat) => {
                builder.pop_value()?;
                builder.push_value()?;
                builder.ops.push(ScalarFunctionOp::Neg);
            }
            Instr::EqF64
            | Instr::NeF64
            | Instr::LtF64
            | Instr::GtF64
            | Instr::LeF64
            | Instr::GeF64 => {
                builder.pop_value()?;
                builder.pop_value()?;
                builder.push_bool()?;
                builder
                    .ops
                    .push(ScalarFunctionOp::Cmp(f64_relation(instr)?));
            }
            Instr::CallDynamicBinaryBoth(intrinsic, _) => {
                if let Some(op) = f64_function_arithmetic_intrinsic(intrinsic) {
                    builder.pop_value()?;
                    builder.pop_value()?;
                    builder.push_value()?;
                    builder.ops.push(op);
                } else {
                    let relation = f64_function_relation_intrinsic(intrinsic)?;
                    builder.pop_value()?;
                    builder.pop_value()?;
                    builder.push_bool()?;
                    builder.ops.push(ScalarFunctionOp::Cmp(relation));
                }
            }
            Instr::Call(target_index, arg_count)
            | Instr::CallInbounds(target_index, arg_count)
            | Instr::CallResolved(target_index, arg_count) => {
                let op = f64_function_call_op(
                    code,
                    functions,
                    base_function_count,
                    *target_index,
                    *arg_count,
                    depth,
                    &nested_visiting_entries,
                    &mut builder,
                )?;
                for _ in 0..*arg_count {
                    builder.pop_value()?;
                }
                builder.push_value()?;
                builder.ops.push(op);
            }
            Instr::JumpIfZero(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.pop_bool()?;
                builder.ops.push(ScalarFunctionOp::JumpIfZero(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfEqF64(target) | Instr::JumpIfNeF64(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.pop_value()?;
                builder.pop_value()?;
                builder
                    .ops
                    .push(ScalarFunctionOp::JumpIf(f64_relation(instr)?, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfNotLtF64(target)
            | Instr::JumpIfNotGtF64(target)
            | Instr::JumpIfNotLeF64(target)
            | Instr::JumpIfNotGeF64(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.pop_value()?;
                builder.pop_value()?;
                let relation = match instr {
                    Instr::JumpIfNotLtF64(_) => ScalarRelation::Ge,
                    Instr::JumpIfNotGtF64(_) => ScalarRelation::Le,
                    Instr::JumpIfNotLeF64(_) => ScalarRelation::Gt,
                    Instr::JumpIfNotGeF64(_) => ScalarRelation::Lt,
                    _ => unreachable!(),
                };
                builder.ops.push(ScalarFunctionOp::JumpIf(relation, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::Jump(target) => {
                let target = scalar_function_target(entry_ip, end_ip, *target)?;
                builder.ops.push(ScalarFunctionOp::Jump(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::ReturnF64 => {
                builder.pop_value()?;
                if !builder.stack_is_empty() {
                    return None;
                }
                builder.ops.push(ScalarFunctionOp::Return);
                has_return = true;
            }
            _ => return None,
        }
    }

    if !has_return || builder.slots.len() > F64_FUNCTION_SLOT_CAP {
        return None;
    }
    remap_scalar_function_op_targets(&mut builder.ops, entry_ip, &ip_to_first_op);
    Some(F64FunctionBlock {
        slots: builder.slots,
        ops: builder.ops,
        callees: builder.callees,
    })
}

/// Convert a bytecode-relative jump target into a scalar-function-block-relative
/// index, or reject it (Issue #10427; shared by the i64 and f64 predecoders).
fn scalar_function_target(entry_ip: usize, end_ip: usize, target_ip: usize) -> Option<usize> {
    if target_ip >= entry_ip && target_ip < end_ip {
        return Some(target_ip - entry_ip);
    }
    None
}

fn i64_function_arithmetic_intrinsic(intrinsic: &Intrinsic) -> Option<ScalarFunctionOp<i64>> {
    match intrinsic {
        Intrinsic::DynamicAdd | Intrinsic::AddInt => Some(ScalarFunctionOp::Add),
        Intrinsic::DynamicSub | Intrinsic::SubInt => Some(ScalarFunctionOp::Sub),
        Intrinsic::DynamicMul | Intrinsic::MulInt => Some(ScalarFunctionOp::Mul),
        Intrinsic::SremInt => Some(ScalarFunctionOp::Rem),
        _ => None,
    }
}

fn f64_function_arithmetic_intrinsic(intrinsic: &Intrinsic) -> Option<ScalarFunctionOp<f64>> {
    match intrinsic {
        Intrinsic::DynamicAdd => Some(ScalarFunctionOp::Add),
        Intrinsic::DynamicSub => Some(ScalarFunctionOp::Sub),
        Intrinsic::DynamicMul => Some(ScalarFunctionOp::Mul),
        Intrinsic::DynamicDiv => Some(ScalarFunctionOp::Div),
        _ => None,
    }
}

fn i64_function_call_op(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
    depth: usize,
    visiting_entries: &[usize],
    builder: &mut ScalarFunctionBuilder<'_, i64>,
) -> Option<ScalarFunctionOp<i64>> {
    if let Some(op) =
        i64_function_base_unary_call(functions, base_function_count, target_index, arg_count)
    {
        return Some(op);
    }

    if depth >= MAX_I64_FUNCTION_CALL_DEPTH {
        return None;
    }
    let target = functions.get(target_index)?;
    if !i64_function_target_shape(target, arg_count) {
        return None;
    }
    let callee = try_predecode_i64_function_inner(
        code,
        functions,
        base_function_count,
        target.entry,
        target.code_end,
        &target.param_slots,
        depth + 1,
        visiting_entries,
    )?;
    let callee_index = builder.add_callee(callee)?;
    Some(ScalarFunctionOp::Call(callee_index, arg_count))
}

fn f64_function_call_op(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
    depth: usize,
    visiting_entries: &[usize],
    builder: &mut ScalarFunctionBuilder<'_, f64>,
) -> Option<ScalarFunctionOp<f64>> {
    if let Some(op) =
        f64_function_base_unary_call(functions, base_function_count, target_index, arg_count)
    {
        return Some(op);
    }

    if depth >= MAX_F64_FUNCTION_CALL_DEPTH {
        return None;
    }
    let target = functions.get(target_index)?;
    if !f64_function_target_shape(target, arg_count) {
        return None;
    }
    let callee = try_predecode_f64_function_inner(
        code,
        functions,
        base_function_count,
        target.entry,
        target.code_end,
        &target.param_slots,
        depth + 1,
        visiting_entries,
    )?;
    let callee_index = builder.add_callee(callee)?;
    Some(ScalarFunctionOp::Call(callee_index, arg_count))
}

fn i64_function_base_unary_call(
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
) -> Option<ScalarFunctionOp<i64>> {
    if arg_count != 1 {
        return None;
    }
    if target_index >= base_function_count {
        return None;
    }
    let target = functions.get(target_index)?;
    let short_name = target
        .name
        .rsplit_once('.')
        .map_or(target.name.as_str(), |(_, short)| short);
    if short_name == "abs"
        && target.params.len() == 1
        && target.param_slots.len() == 1
        && target.vararg_param_index.is_none()
        && target.kwparams.is_empty()
        && target.type_params.is_empty()
        && matches!(
            target.params.first().map(|(_, ty)| ty),
            Some(ValueType::I64)
        )
        && matches!(target.return_type, ValueType::I64)
    {
        return Some(ScalarFunctionOp::Abs);
    }
    None
}

fn f64_function_base_unary_call(
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
) -> Option<ScalarFunctionOp<f64>> {
    if arg_count != 1 {
        return None;
    }
    if target_index >= base_function_count {
        return None;
    }
    let target = functions.get(target_index)?;
    if target.name.as_str() != "abs" {
        return None;
    }
    Some(ScalarFunctionOp::Abs)
}

#[allow(clippy::too_many_arguments)]
fn typed_loop_i64_call_op(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
    callees: &mut Vec<I64FunctionBlock>,
    typed_callees: &mut Vec<TypedScalarFunctionBlock>,
    allow_typed_fallback: bool,
) -> Option<TypedLoopOp> {
    // Base unary intrinsics such as `abs` are not expressed as typed-loop ops;
    // rejecting the loop lets the normal frame path handle them.
    if target_index < base_function_count {
        return None;
    }
    let target = functions.get(target_index)?;
    if !i64_function_target_shape(target, arg_count) {
        return None;
    }
    if let Some(callee) = try_predecode_i64_function(
        code,
        functions,
        base_function_count,
        target.entry,
        target.code_end,
        &target.param_slots,
    ) {
        if callees.len() >= I64_FUNCTION_CALLEE_CAP {
            return None;
        }
        let callee_index = callees.len();
        callees.push(callee);
        return Some(TypedLoopOp::CallI64Function(callee_index, arg_count));
    }
    // Issue #10542: the pure-I64 predecoder rejects bodies that mix in an
    // F64 local (a "loop counter is I64, math is F64" helper shape is rare
    // for an I64-return function but structurally possible), which used to
    // reject the WHOLE caller loop. Fall back to the mixed-type
    // `TypedScalarFunctionBlock` decoder (Issue #9693) — the shape gate above
    // already guarantees pure-I64 params/return, so argument binding stays
    // unambiguous (every arg comes off the i64 stack and binds to an I64
    // local; see `run_typed_scalar_block_with_i64_args`). `allow_typed_fallback`
    // is `false` while already decoding a fallback callee's own body, so this
    // recursion is bounded to one extra level — a further nested mixed-type
    // call inside the fallback callee is rejected here (falls to the frame
    // path for that inner call) rather than recursing without a depth guard.
    if !allow_typed_fallback {
        return None;
    }
    let typed = try_predecode_typed_scalar_function_inner(
        code,
        functions,
        target.entry,
        target.code_end,
        base_function_count,
        &target.param_slots,
        false,
    )?;
    if typed_callees.len() >= I64_FUNCTION_CALLEE_CAP {
        return None;
    }
    let callee_index = typed_callees.len();
    typed_callees.push(typed);
    Some(TypedLoopOp::CallTypedI64Function(callee_index, arg_count))
}

#[allow(clippy::too_many_arguments)]
fn typed_loop_f64_call_op(
    code: &[Instr],
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
    f64_callees: &mut Vec<F64FunctionBlock>,
    typed_callees: &mut Vec<TypedScalarFunctionBlock>,
    allow_typed_fallback: bool,
) -> Option<TypedLoopOp> {
    if target_index < base_function_count {
        return None;
    }
    let target = functions.get(target_index)?;
    if !f64_function_target_shape(target, arg_count) {
        return None;
    }
    if let Some(callee) = try_predecode_f64_function(
        code,
        functions,
        base_function_count,
        target.entry,
        target.code_end,
        &target.param_slots,
    ) {
        if f64_callees.len() >= F64_FUNCTION_CALLEE_CAP {
            return None;
        }
        let callee_index = f64_callees.len();
        f64_callees.push(callee);
        return Some(TypedLoopOp::CallF64Function(callee_index, arg_count));
    }
    // Issue #10542: fall back to the mixed-type `TypedScalarFunctionBlock`
    // decoder when the pure-F64 predecoder rejects the body (the common
    // "F64 math + I64 loop counter" helper shape). The shape gate above
    // already guarantees pure-F64 params/return, so every argument comes off
    // the f64 stack and binds to an F64 local (see
    // `run_typed_scalar_block_with_f64_args`) — no arg-type ambiguity. See
    // the I64 mirror above for the `allow_typed_fallback` recursion-depth
    // rationale.
    if !allow_typed_fallback {
        return None;
    }
    let typed = try_predecode_typed_scalar_function_inner(
        code,
        functions,
        target.entry,
        target.code_end,
        base_function_count,
        &target.param_slots,
        false,
    )?;
    if typed_callees.len() >= F64_FUNCTION_CALLEE_CAP {
        return None;
    }
    let callee_index = typed_callees.len();
    typed_callees.push(typed);
    Some(TypedLoopOp::CallTypedF64Function(callee_index, arg_count))
}

fn i64_function_target_shape(target: &FunctionInfo, arg_count: usize) -> bool {
    !target.is_generated
        && target.vararg_param_index.is_none()
        && target.kwparams.is_empty()
        && target.type_params.is_empty()
        && target.params.len() == arg_count
        && target.param_slots.len() == arg_count
        && matches!(target.return_type, ValueType::I64)
        && target
            .params
            .iter()
            .all(|(_, ty)| matches!(ty, ValueType::I64))
}

fn f64_function_target_shape(target: &FunctionInfo, arg_count: usize) -> bool {
    !target.is_generated
        && target.vararg_param_index.is_none()
        && target.kwparams.is_empty()
        && target.type_params.is_empty()
        && target.params.len() == arg_count
        && target.param_slots.len() == arg_count
        && matches!(target.return_type, ValueType::F64)
        && target
            .params
            .iter()
            .all(|(_, ty)| matches!(ty, ValueType::F64))
}

fn i64_function_relation_intrinsic(intrinsic: &Intrinsic) -> Option<I64Relation> {
    match intrinsic {
        Intrinsic::EqFloat | Intrinsic::EqInt => Some(I64Relation::Eq),
        Intrinsic::NeFloat | Intrinsic::NeInt => Some(I64Relation::Ne),
        Intrinsic::LtFloat | Intrinsic::SltInt => Some(I64Relation::Lt),
        Intrinsic::GtFloat | Intrinsic::SgtInt => Some(I64Relation::Gt),
        Intrinsic::LeFloat | Intrinsic::SleInt => Some(I64Relation::Le),
        Intrinsic::GeFloat | Intrinsic::SgeInt => Some(I64Relation::Ge),
        _ => None,
    }
}

fn f64_function_relation_intrinsic(intrinsic: &Intrinsic) -> Option<F64Relation> {
    match intrinsic {
        Intrinsic::EqFloat => Some(F64Relation::Eq),
        Intrinsic::NeFloat => Some(F64Relation::Ne),
        Intrinsic::LtFloat => Some(F64Relation::Lt),
        Intrinsic::GtFloat => Some(F64Relation::Gt),
        Intrinsic::LeFloat => Some(F64Relation::Le),
        Intrinsic::GeFloat => Some(F64Relation::Ge),
        _ => None,
    }
}

fn i64_relation(instr: &Instr) -> Option<I64Relation> {
    match instr {
        Instr::EqI64 | Instr::JumpIfEqI64(_) => Some(I64Relation::Eq),
        Instr::NeI64 | Instr::JumpIfNeI64(_) => Some(I64Relation::Ne),
        Instr::LtI64 | Instr::JumpIfLtI64(_) => Some(I64Relation::Lt),
        Instr::GtI64 | Instr::JumpIfGtI64(_) => Some(I64Relation::Gt),
        Instr::LeI64 | Instr::JumpIfLeI64(_) => Some(I64Relation::Le),
        Instr::GeI64 | Instr::JumpIfGeI64(_) => Some(I64Relation::Ge),
        _ => None,
    }
}

fn f64_relation(instr: &Instr) -> Option<F64Relation> {
    match instr {
        Instr::EqF64 | Instr::JumpIfEqF64(_) => Some(F64Relation::Eq),
        Instr::NeF64 | Instr::JumpIfNeF64(_) => Some(F64Relation::Ne),
        Instr::LtF64 => Some(F64Relation::Lt),
        Instr::GtF64 => Some(F64Relation::Gt),
        Instr::LeF64 => Some(F64Relation::Le),
        Instr::GeF64 => Some(F64Relation::Ge),
        _ => None,
    }
}

fn not_f64_relation(instr: &Instr) -> Option<F64Relation> {
    match instr {
        Instr::JumpIfNotLtF64(_) => Some(F64Relation::Lt),
        Instr::JumpIfNotGtF64(_) => Some(F64Relation::Gt),
        Instr::JumpIfNotLeF64(_) => Some(F64Relation::Le),
        Instr::JumpIfNotGeF64(_) => Some(F64Relation::Ge),
        _ => None,
    }
}

#[derive(Default)]
struct TypedLoopBuilder {
    array_slots: Vec<TypedLoopSlot>,
    f64_slots: Vec<TypedLoopSlot>,
    i64_slots: Vec<TypedLoopSlot>,
    // Issue #10559: see `TypedLoopBlock::str_slots` / `str_consts`.
    str_slots: Vec<TypedLoopSlot>,
    str_consts: Vec<StrRef>,
    ops: Vec<TypedLoopOp>,
    /// Frame-less predecoded I64 callees referenced by `TypedLoopOp::CallI64Function`
    /// (Issue #10309).
    i64_callees: Vec<I64FunctionBlock>,
    /// Frame-less predecoded F64 callees referenced by `TypedLoopOp::CallF64Function`.
    f64_callees: Vec<F64FunctionBlock>,
    /// `(spec_func_index, arg_count)` for each `CallSpecializeI64Slots` site
    /// inlined by `TypedLoopOp::CallSpecializeI64Function` (Issue #10439).
    specialize_callees: Vec<(usize, usize)>,
    /// F64 mirror (Issue #10491): `CallSpecializeF64Slots` sites inlined by
    /// `TypedLoopOp::CallSpecializeF64Function`.
    specialize_f64_callees: Vec<(usize, usize)>,
    /// Issue #10567 (round 2): see `TypedLoopBlock::specialize_complex_i64_callees`.
    specialize_complex_i64_callees: Vec<usize>,
    /// Issue #10542: frame-less predecoded mixed-type I64-shaped callees
    /// referenced by `TypedLoopOp::CallTypedI64Function`.
    typed_i64_callees: Vec<TypedScalarFunctionBlock>,
    /// Issue #10542: frame-less predecoded mixed-type F64-shaped callees
    /// referenced by `TypedLoopOp::CallTypedF64Function`.
    typed_f64_callees: Vec<TypedScalarFunctionBlock>,
    array_depth: usize,
    // Issue #10104: bytecode slot provenance of each array currently on the
    // simulated array stack, kept aligned with `array_depth`. A typed
    // `IndexLoad*` needs the indexed array's declared element type, which is
    // resolved from the owning function's param `Vector{T}` type via this slot.
    array_slot_stack: Vec<usize>,
    f64_depth: usize,
    i64_depth: usize,
    bool_depth: usize,
    // Issue #10559: simulated string mini-stack depth.
    str_depth: usize,
    // Issue #9693 (function mode): simulated complex mini-stack depth and the
    // ComplexF64 param slots encountered, in first-use order (the op operand
    // is an index into this list).
    complex_depth: usize,
    complex_slots: Vec<usize>,
}

impl TypedLoopBuilder {
    fn read_array_slot(&mut self, slot: usize) -> usize {
        read_typed_slot(&mut self.array_slots, slot)
    }

    /// Issue #10566(c): mark `local` (an ARRAY slot) as the target of an
    /// `IndexStoreTyped`, so block entry resolves it through a private
    /// transactional buffer instead of the frame's shared array.
    fn mark_array_slot_stored(&mut self, local: usize) {
        if let Some(slot) = self.array_slots.get_mut(local) {
            slot.stored = true;
        }
    }

    fn read_f64_slot(&mut self, slot: usize) -> usize {
        read_typed_slot(&mut self.f64_slots, slot)
    }

    fn write_f64_slot(&mut self, slot: usize) -> usize {
        write_typed_slot(&mut self.f64_slots, slot)
    }

    fn read_i64_slot(&mut self, slot: usize) -> usize {
        read_typed_slot(&mut self.i64_slots, slot)
    }

    fn write_i64_slot(&mut self, slot: usize) -> usize {
        write_typed_slot(&mut self.i64_slots, slot)
    }

    fn mark_i64_slot_written(&mut self, local: usize) {
        if let Some(slot) = self.i64_slots.get_mut(local) {
            slot.written_in_loop = true;
        }
    }

    fn read_str_slot(&mut self, slot: usize) -> usize {
        read_typed_slot(&mut self.str_slots, slot)
    }

    fn write_str_slot(&mut self, slot: usize) -> usize {
        write_typed_slot(&mut self.str_slots, slot)
    }

    fn push_str(&mut self) -> Option<()> {
        push_depth(&mut self.str_depth)
    }

    fn pop_str(&mut self) -> Option<()> {
        pop_depth(&mut self.str_depth)
    }

    /// Intern a compile-time string literal, returning its `str_consts` index
    /// (Issue #10559). Not deduplicated — predecode-time cost only, and typed
    /// loop bodies rarely repeat the same literal more than a couple of times.
    fn str_const(&mut self, value: StrRef) -> Option<usize> {
        if self.str_consts.len() >= TYPED_LOOP_SLOT_CAP {
            return None;
        }
        let index = self.str_consts.len();
        self.str_consts.push(value);
        Some(index)
    }

    fn push_array(&mut self) -> Option<()> {
        push_depth(&mut self.array_depth)
    }

    fn pop_array(&mut self) -> Option<()> {
        pop_depth(&mut self.array_depth)
    }

    fn push_f64(&mut self) -> Option<()> {
        push_depth(&mut self.f64_depth)
    }

    fn pop_f64(&mut self) -> Option<()> {
        pop_depth(&mut self.f64_depth)
    }

    fn push_i64(&mut self) -> Option<()> {
        push_depth(&mut self.i64_depth)
    }

    fn pop_i64(&mut self) -> Option<()> {
        pop_depth(&mut self.i64_depth)
    }

    fn push_bool(&mut self) -> Option<()> {
        push_depth(&mut self.bool_depth)
    }

    fn pop_bool(&mut self) -> Option<()> {
        pop_depth(&mut self.bool_depth)
    }

    fn push_complex(&mut self) -> Option<()> {
        if self.complex_depth >= COMPLEX_MINI_STACK_CAP {
            return None;
        }
        self.complex_depth += 1;
        Some(())
    }

    fn pop_complex(&mut self) -> Option<()> {
        if self.complex_depth == 0 {
            return None;
        }
        self.complex_depth -= 1;
        Some(())
    }

    /// Index of `slot` in the complex-param list (first-use order).
    fn complex_slot(&mut self, slot: usize) -> Option<usize> {
        if let Some(idx) = self.complex_slots.iter().position(|s| *s == slot) {
            return Some(idx);
        }
        if self.complex_slots.len() >= TYPED_FUNCTION_COMPLEX_PARAM_CAP {
            return None;
        }
        self.complex_slots.push(slot);
        Some(self.complex_slots.len() - 1)
    }

    fn stack_is_empty(&self) -> bool {
        self.array_depth == 0
            && self.f64_depth == 0
            && self.i64_depth == 0
            && self.bool_depth == 0
            && self.complex_depth == 0
            && self.str_depth == 0
    }
}

fn read_typed_slot(slots: &mut Vec<TypedLoopSlot>, slot: usize) -> usize {
    let local = typed_slot_index(slots, slot);
    if !slots[local].written_in_loop {
        slots[local].live_in = true;
    }
    local
}

fn write_typed_slot(slots: &mut Vec<TypedLoopSlot>, slot: usize) -> usize {
    let local = typed_slot_index(slots, slot);
    slots[local].written_in_loop = true;
    local
}

fn typed_slot_index(slots: &mut Vec<TypedLoopSlot>, slot: usize) -> usize {
    if let Some(index) = slots.iter().position(|entry| entry.slot == slot) {
        return index;
    }
    let index = slots.len();
    slots.push(TypedLoopSlot {
        slot,
        live_in: false,
        written_in_loop: false,
        stored: false,
    });
    index
}

fn push_depth(depth: &mut usize) -> Option<()> {
    *depth += 1;
    if *depth > TYPED_LOOP_STACK_CAP {
        return None;
    }
    Some(())
}

fn pop_depth(depth: &mut usize) -> Option<()> {
    if *depth == 0 {
        return None;
    }
    *depth -= 1;
    Some(())
}

/// Builder for a frame-less scalar function block (Issue #10427), generic over
/// the operand element type `S`. Tracks slot deduplication + param binding and a
/// simulated operand/bool stack depth (guarding stack discipline at predecode
/// time). `F64FunctionBuilder` / `I64FunctionBuilder` alias this; `slots` /
/// `new` / `slot` stay `pub` for the `#[doc(hidden)]` F64 test API.
#[derive(Debug, Clone)]
pub struct ScalarFunctionBuilder<'a, S> {
    param_slots: &'a [usize],
    pub slots: Vec<ScalarFunctionSlot>,
    ops: Vec<ScalarFunctionOp<S>>,
    callees: Vec<ScalarFunctionBlock<S>>,
    value_depth: usize,
    bool_depth: usize,
}

impl<'a, S> ScalarFunctionBuilder<'a, S> {
    pub fn new(param_slots: &'a [usize]) -> Self {
        Self {
            param_slots,
            slots: Vec::new(),
            ops: Vec::new(),
            callees: Vec::new(),
            value_depth: 0,
            bool_depth: 0,
        }
    }

    /// Return the dense local index for bytecode `slot`, allocating (and
    /// recording its param binding) on first use.
    pub fn slot(&mut self, slot: usize) -> usize {
        if let Some(index) = self.slots.iter().position(|entry| entry.slot == slot) {
            return index;
        }
        let index = self.slots.len();
        self.slots.push(ScalarFunctionSlot {
            slot,
            param_index: self
                .param_slots
                .iter()
                .position(|param_slot| *param_slot == slot),
        });
        index
    }

    fn push_value(&mut self) -> Option<()> {
        push_depth(&mut self.value_depth)
    }

    fn pop_value(&mut self) -> Option<()> {
        pop_depth(&mut self.value_depth)
    }

    fn push_bool(&mut self) -> Option<()> {
        push_depth(&mut self.bool_depth)
    }

    fn pop_bool(&mut self) -> Option<()> {
        pop_depth(&mut self.bool_depth)
    }

    fn stack_is_empty(&self) -> bool {
        self.value_depth == 0 && self.bool_depth == 0
    }

    fn add_callee(&mut self, block: ScalarFunctionBlock<S>) -> Option<usize> {
        if self.callees.len() >= SCALAR_FUNCTION_CALLEE_CAP {
            return None;
        }
        let index = self.callees.len();
        self.callees.push(block);
        Some(index)
    }
}

pub type F64FunctionBuilder<'a> = ScalarFunctionBuilder<'a, f64>;

impl<R: RngLike> Vm<R> {
    #[inline]
    pub(crate) fn refresh_next_executable_ip_from(&mut self, ip: usize) {
        self.next_executable_ip = self.executable.next_ip_from(ip);
    }

    #[inline]
    pub(crate) fn try_execute_executable_block(
        &mut self,
        ip: usize,
    ) -> Result<ExecutableBlockResult, super::VmError> {
        let Some(block) = self.executable.block_at(ip).cloned() else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        match block {
            ExecutableBlock::Typed(block) => self.execute_typed_loop_block(&block),
        }
    }

    #[inline]
    pub(crate) fn try_execute_i64_function_call(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[Value],
    ) -> Option<i64> {
        let mut i64_args = Vec::with_capacity(args.len());
        for value in args {
            let Value::I64(value) = value else {
                return None;
            };
            i64_args.push(*value);
        }
        self.try_execute_i64_function_call_i64_args(entry_ip, end_ip, param_slots, &i64_args)
    }

    pub(crate) fn try_execute_i64_function_call_i64_args(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[i64],
    ) -> Option<i64> {
        if !self.i64_function_cache.contains_key(&entry_ip) {
            let decoded = try_predecode_i64_function(
                self.code.as_ref(),
                &self.functions,
                self.base_function_count,
                entry_ip,
                end_ip,
                param_slots,
            );
            self.i64_function_cache.insert(entry_ip, decoded);
            self.enforce_i64_function_cache_limit();
        }
        let block = self.i64_function_cache.get(&entry_ip)?.as_ref()?;
        Self::execute_i64_function_block(block, args)
    }

    /// Resolve the runtime-specialized I64 body of an untyped callee reached
    /// through a `CallSpecializeI64Slots` site inside a typed loop (Issue #10439).
    ///
    /// Returns a cloned [`I64FunctionBlock`] iff the callee already has an all-`I64`
    /// specialization recorded in `specialization_i64_cache` (populated by the
    /// generic dispatch path's `record_i64_spec_dispatch`) AND that specialized
    /// body predecodes to a pure I64 function. Returns `None` otherwise — the
    /// callee has not been specialized yet (e.g. the loop's very first entry,
    /// before the callee's first call) or its body is not I64-decodable — so the
    /// typed-loop executor bails to the generic interpreter, which runs the site
    /// correctly and populates the cache for the next entry. The block is the
    /// *same* body the generic `CallSpecializeI64Slots` hit path executes, so the
    /// result is identical; returning an owned clone lets the caller lend it into
    /// the static typed-op core alongside `self.rng` (disjoint borrows).
    fn resolve_specialize_i64_callee(
        &mut self,
        spec_func_index: usize,
        arg_count: usize,
    ) -> Option<I64FunctionBlock> {
        let dispatch = self
            .specialization_i64_cache
            .get(&(spec_func_index, arg_count))
            .cloned()?;
        // Defensive: the cache key already pins the arity, but the frame-less
        // call reads exactly `param_slots.len()` args, so require agreement.
        if dispatch.param_slots.len() != arg_count {
            return None;
        }
        if !self.i64_function_cache.contains_key(&dispatch.entry) {
            let decoded = try_predecode_i64_function(
                self.code.as_ref(),
                &self.functions,
                self.base_function_count,
                dispatch.entry,
                dispatch.code_end,
                &dispatch.param_slots,
            );
            self.i64_function_cache.insert(dispatch.entry, decoded);
            self.enforce_i64_function_cache_limit();
        }
        self.i64_function_cache
            .get(&dispatch.entry)
            .and_then(|decoded| decoded.clone())
    }

    /// Float64 mirror of [`Self::resolve_specialize_i64_callee`] (Issue
    /// #10491): resolve a `CallSpecializeF64Slots` site's frame-less body from
    /// the live `specialization_f64_cache`. A pure-F64 body resolves through
    /// `f64_function_cache`; a mixed-type body (e.g. an I64 loop counter)
    /// through `typed_function_cache` as a [`TypedScalarFunctionBlock`].
    fn resolve_specialize_f64_callee(
        &mut self,
        spec_func_index: usize,
        arg_count: usize,
    ) -> Option<ResolvedSpecF64Callee> {
        let dispatch = self
            .specialization_f64_cache
            .get(&(spec_func_index, arg_count))
            .cloned()?;
        if dispatch.param_slots.len() != arg_count {
            return None;
        }
        if !self.f64_function_cache.contains_key(&dispatch.entry) {
            let decoded = try_predecode_f64_function(
                self.code.as_ref(),
                &self.functions,
                self.base_function_count,
                dispatch.entry,
                dispatch.code_end,
                &dispatch.param_slots,
            );
            self.f64_function_cache.insert(dispatch.entry, decoded);
            self.enforce_f64_function_cache_limit();
        }
        if let Some(Some(block)) = self.f64_function_cache.get(&dispatch.entry) {
            return Some(ResolvedSpecF64Callee::F64(block.clone()));
        }
        if !self.typed_function_cache.contains_key(&dispatch.entry) {
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                dispatch.entry,
                dispatch.code_end,
                self.base_function_count,
                &dispatch.param_slots,
            );
            self.typed_function_cache.insert(dispatch.entry, decoded);
            self.enforce_typed_function_cache_limit();
        }
        self.typed_function_cache
            .get(&dispatch.entry)
            .and_then(|decoded| decoded.clone())
            .map(ResolvedSpecF64Callee::Typed)
    }

    /// Narrow mixed-arg mirror of [`Self::resolve_specialize_f64_callee`]
    /// (Issue #10567 round 2): resolve a `CallSpecialize`/`CallSpecializeInbounds`
    /// site's frame-less body for the `f(complex_arg, i64_arg)` shape from the
    /// live `specialization_mixed_cache` (populated by
    /// `Vm::record_mixed_spec_dispatch` alongside the I64/F64 dispatch
    /// recorders whenever the callee's argument types are not uniformly I64
    /// or F64). The recognizer only ever emits `CallSpecializeComplexI64Function`
    /// for a call site whose own two arguments are exactly
    /// `(Complex{Float64}, Int64)` positionally, so the lookup key is fixed
    /// to that exact type pair — NOT just `(spec_func_index, arity)`, which
    /// would let a same-arity call to the same method with different
    /// concrete argument types (e.g. `Complex{Int64}` instead of
    /// `Complex{Float64}`) silently resolve to the WRONG specialized body
    /// (see the field doc on `Vm::specialization_mixed_cache`). Returns
    /// `None` — the caller bails to the generic interpreter, which is always
    /// correct and populates the cache — whenever the callee has not been
    /// specialized for this exact `(spec_func_index, [ComplexF64, I64])` key
    /// yet, its body does not predecode as a [`TypedScalarFunctionBlock`], or
    /// its parameter shape is not exactly `(ComplexF64, I64)` in that order:
    /// binding is only sound when the callee's declared params agree.
    fn resolve_specialize_complex_i64_callee(
        &mut self,
        spec_func_index: usize,
    ) -> Option<TypedScalarFunctionBlock> {
        let dispatch = self
            .specialization_mixed_cache
            .get(&(spec_func_index, vec![ValueType::ComplexF64, ValueType::I64]))
            .cloned()?;
        if dispatch.param_slots.len() != 2 {
            return None;
        }
        if !self.typed_function_cache.contains_key(&dispatch.entry) {
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                dispatch.entry,
                dispatch.code_end,
                self.base_function_count,
                &dispatch.param_slots,
            );
            self.typed_function_cache.insert(dispatch.entry, decoded);
            self.enforce_typed_function_cache_limit();
        }
        let block = self
            .typed_function_cache
            .get(&dispatch.entry)
            .and_then(|decoded| decoded.clone())?;
        if block.params.len() != 2
            || !matches!(block.params[0], TypedFunctionParamBinding::ComplexF64(_))
            || !matches!(block.params[1], TypedFunctionParamBinding::I64(_))
        {
            return None;
        }
        Some(block)
    }

    /// Execute a resolved narrow mixed-arg `(Complex, I64)` specialize callee
    /// frame-lessly (Issue #10567 round 2), binding the arguments directly
    /// into `TypedOpsState` — no boxed `Complex{Float64}` struct is ever
    /// allocated. Returns the callee's returned `Value`, or `None` on any
    /// bail; the callee body is effect-free by construction (same guarantee
    /// as the F64/I64 mirrors above).
    pub(in crate::vm) fn run_typed_scalar_block_with_complex_i64_args(
        block: &TypedScalarFunctionBlock,
        complex_arg: (f64, f64),
        i64_arg: i64,
        rng: &mut R,
    ) -> Option<Value> {
        if block.params.len() != 2 {
            return None;
        }
        let mut st = TypedOpsState::new(0, 0);
        match block.params[0] {
            TypedFunctionParamBinding::ComplexF64(idx) => {
                if idx >= st.complex_params.len() {
                    return None;
                }
                st.complex_params[idx] = complex_arg;
            }
            TypedFunctionParamBinding::Unused => {}
            _ => return None,
        }
        match block.params[1] {
            TypedFunctionParamBinding::I64(local) => {
                st.i64_locals[local] = i64_arg;
                st.i64_init[local] = true;
            }
            TypedFunctionParamBinding::Unused => {}
            _ => return None,
        }
        let outcome = Self::run_typed_ops_dispatch(
            block.ops_trusted,
            &block.ops,
            &block.i64_callees,
            &block.f64_callees,
            &[],
            &[],
            &[],
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &[],
            None,
            &mut st,
            rng,
        )
        .ok()?;
        let TypedOpsOutcome::EarlyReturn(value) = outcome else {
            return None;
        };
        Some(value)
    }

    /// Execute a resolved mixed-type specialize callee frame-lessly with all-F64
    /// arguments (Issue #10491). A free function (no `&self`) so the typed-loop
    /// core can call it while borrowing the resolved-callee slice. Returns the
    /// callee's returned `Value`, or `None` on any bail — the callee body is
    /// effect-free by construction (`try_predecode_typed_scalar_function`
    /// rejects `RandF64` and array ops), so a bail is always safe to re-run.
    pub(in crate::vm) fn run_typed_scalar_block_with_f64_args(
        block: &TypedScalarFunctionBlock,
        args: &[f64],
        rng: &mut R,
    ) -> Option<Value> {
        if block.params.len() != args.len() {
            return None;
        }
        let mut st = TypedOpsState::new(0, 0);
        for (binding, value) in block.params.iter().zip(args.iter()) {
            match binding {
                TypedFunctionParamBinding::F64(local) => {
                    st.f64_locals[*local] = *value;
                    st.f64_init[*local] = true;
                }
                TypedFunctionParamBinding::Unused => {}
                _ => return None,
            }
        }
        let outcome = Self::run_typed_ops_dispatch(
            block.ops_trusted,
            &block.ops,
            &block.i64_callees,
            &block.f64_callees,
            &[],
            &[],
            &[],
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &[],
            None,
            &mut st,
            rng,
        )
        .ok()?;
        let TypedOpsOutcome::EarlyReturn(value) = outcome else {
            return None;
        };
        Some(value)
    }

    /// Issue #10542: I64 mirror of [`Self::run_typed_scalar_block_with_f64_args`]
    /// — execute a resolved mixed-type callee frame-lessly with all-I64
    /// arguments (the `CallTypedI64Function` typed-loop op's callee: pure-I64
    /// declared params/return, mixed-type body). Returns the callee's returned
    /// `Value`, or `None` on any bail; the callee body is effect-free by
    /// construction (same guarantee as the F64 mirror).
    pub(in crate::vm) fn run_typed_scalar_block_with_i64_args(
        block: &TypedScalarFunctionBlock,
        args: &[i64],
        rng: &mut R,
    ) -> Option<Value> {
        if block.params.len() != args.len() {
            return None;
        }
        let mut st = TypedOpsState::new(0, 0);
        for (binding, value) in block.params.iter().zip(args.iter()) {
            match binding {
                TypedFunctionParamBinding::I64(local) => {
                    st.i64_locals[*local] = *value;
                    st.i64_init[*local] = true;
                }
                TypedFunctionParamBinding::Unused => {}
                _ => return None,
            }
        }
        let outcome = Self::run_typed_ops_dispatch(
            block.ops_trusted,
            &block.ops,
            &block.i64_callees,
            &block.f64_callees,
            &[],
            &[],
            &[],
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &[],
            None,
            &mut st,
            rng,
        )
        .ok()?;
        let TypedOpsOutcome::EarlyReturn(value) = outcome else {
            return None;
        };
        Some(value)
    }

    #[inline]
    pub(crate) fn try_execute_f64_function_call(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[Value],
    ) -> Option<f64> {
        let mut f64_args = Vec::with_capacity(args.len());
        for value in args {
            let Value::F64(value) = value else {
                return None;
            };
            f64_args.push(*value);
        }
        self.try_execute_f64_function_call_f64_args(entry_ip, end_ip, param_slots, &f64_args)
    }

    pub(crate) fn try_execute_f64_function_call_f64_args(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[f64],
    ) -> Option<f64> {
        if !self.f64_function_cache.contains_key(&entry_ip) {
            let decoded = try_predecode_f64_function(
                self.code.as_ref(),
                &self.functions,
                self.base_function_count,
                entry_ip,
                end_ip,
                param_slots,
            );
            self.f64_function_cache.insert(entry_ip, decoded);
            self.enforce_f64_function_cache_limit();
        }
        let block = self.f64_function_cache.get(&entry_ip)?.as_ref()?;
        Self::execute_f64_function_block(block, args)
    }

    /// Mixed-type mirror of [`Self::try_execute_f64_function_call_f64_args`]
    /// (Issue #10491): run a specialized body that predecodes only as a
    /// [`TypedScalarFunctionBlock`] (e.g. all-F64 params with an I64 loop
    /// counter) frame-lessly from all-F64 argument values.
    pub(in crate::vm) fn try_execute_typed_scalar_function_call_f64_args(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[f64],
    ) -> Option<Value> {
        if !self.typed_function_cache.contains_key(&entry_ip) {
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                entry_ip,
                end_ip,
                self.base_function_count,
                param_slots,
            );
            self.typed_function_cache.insert(entry_ip, decoded);
            self.enforce_typed_function_cache_limit();
        }
        let Some(Some(block)) = self.typed_function_cache.get(&entry_ip) else {
            return None;
        };
        Self::run_typed_scalar_block_with_f64_args(block, args, &mut self.rng)
    }

    /// Frame-less typed scalar function call (Issue #9693): when the callee's
    /// whole body predecodes to a [`TypedScalarFunctionBlock`], bind the call
    /// arguments directly into typed locals and run the ops — no frame, no
    /// slot binding, no per-instruction dispatch, no return routing. Returns
    /// `None` (with the argument stack untouched) whenever anything does not
    /// fit; the caller falls back to the normal frame path.
    pub(in crate::vm) fn try_execute_typed_scalar_function_call(
        &mut self,
        func_index: usize,
        entry_ip: usize,
        end_ip: usize,
        arg_count: usize,
    ) -> Option<crate::vm::exec::DispatchAction> {
        if !self.typed_function_cache.contains_key(&entry_ip) {
            let param_slots = self.functions.get(func_index)?.param_slots.clone();
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                entry_ip,
                end_ip,
                self.base_function_count,
                &param_slots,
            );
            self.typed_function_cache.insert(entry_ip, decoded);
            self.enforce_typed_function_cache_limit();
        }
        let Some(Some(block)) = self.typed_function_cache.get(&entry_ip) else {
            return None;
        };
        if block.params.len() != arg_count {
            return None;
        }

        let start = self.stack.len().checked_sub(arg_count)?;
        let mut st = TypedOpsState::new(0, 0);
        for (binding, value) in block.params.iter().zip(self.stack[start..].iter()) {
            bind_typed_function_param(binding, value, &self.struct_heap, &mut st)?;
        }

        // Array ops are rejected at predecode, so the core cannot error here;
        // treat a (theoretically unreachable) error as a bail.
        let outcome = Self::run_typed_ops_dispatch(
            block.ops_trusted,
            &block.ops,
            &block.i64_callees,
            &block.f64_callees,
            // Typed scalar *function* / broadcast blocks never inline a
            // `CallSpecializeI64Slots` / `CallSpecializeF64Slots` site
            // (#10439/#10491 gate emission to loop mode), so there is nothing
            // to resolve; a stray op would bail. Same for the string const
            // pool (Issue #10559): function/broadcast blocks carry no String
            // params in this MVP.
            &[],
            &[],
            &[],
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &[],
            None,
            &mut st,
            &mut self.rng,
        )
        .ok()?;
        let TypedOpsOutcome::EarlyReturn(value) = outcome else {
            // `Completed` (fell past the ops) or a guard bail: the argument
            // stack is untouched — re-run through the frame path.
            return None;
        };

        profiler::record_event("CallDirectFastTypedFunctionHit");
        let new_len = self.stack.len() - arg_count;
        self.stack.truncate(new_len);
        self.stack.push(value);
        Some(crate::vm::exec::DispatchAction::Continue)
    }

    /// Frame-less typed scalar function call from an explicit, already-owned
    /// `args: &[Value]` slice reached through the *generic* `CallSpecialize` /
    /// `CallSpecializeInbounds` dispatch path (Issues #10567 / #10704).
    ///
    /// `execute_call_specialize_with_args` already resolves a runtime
    /// specialization for genuinely mixed-type call-site arguments (e.g. a
    /// boxed `ComplexF64` struct plus an `Int64` counter, as at
    /// `mandel_point(c, maxiter)`'s call site) — but its two existing
    /// frame-less fast paths, [`Self::try_execute_i64_function_call`] and
    /// [`Self::try_execute_f64_function_call`], both require *every* argument
    /// to be the same scalar `Value` variant, so a mixed-type arg list falls
    /// through to the generic frame-allocating path on every call even though
    /// the callee's specialized body may already be pure scalar/ComplexF64
    /// ops (SROA'd by `subset_julia_vm_vm/src/vm/specialize`). This helper
    /// generalizes those two paths using the same
    /// [`TypedScalarFunctionBlock`] / [`bind_typed_function_param`] machinery
    /// `try_broadcast_typed_kernel` / `try_run_typed_scalar_function_with_args`
    /// already use for typed callees, so a `ComplexF64` argument binds via
    /// [`TypedFunctionParamBinding::ComplexF64`] into `(re, im)` locals
    /// instead of round-tripping through a boxed struct read inside the body.
    ///
    /// Returns `None` (no side effects — the argument slice and any state are
    /// left untouched) whenever anything does not fit: wrong arity, a
    /// non-decodable body, an argument that does not bind (e.g. a `Struct`
    /// that is not a genuine two-`F64`-field `Complex{Float64}`), or the body
    /// falls past its ops instead of hitting a `Return*`. The caller then runs
    /// the normal frame path unchanged, so this is a pure, additive
    /// optimization: identical observable behavior on every input, just a
    /// faster path for the common case.
    pub(crate) fn try_execute_typed_scalar_function_call_from_values(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[Value],
    ) -> Option<Value> {
        if param_slots.len() != args.len() {
            return None;
        }
        if !self.typed_function_cache.contains_key(&entry_ip) {
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                entry_ip,
                end_ip,
                self.base_function_count,
                param_slots,
            );
            self.typed_function_cache.insert(entry_ip, decoded);
            self.enforce_typed_function_cache_limit();
        }
        let Some(Some(block)) = self.typed_function_cache.get(&entry_ip) else {
            return None;
        };
        if block.params.len() != args.len() {
            return None;
        }
        let mut st = TypedOpsState::new(0, 0);
        for (binding, value) in block.params.iter().zip(args.iter()) {
            bind_typed_function_param(binding, value, &self.struct_heap, &mut st)?;
        }
        // Array ops are rejected at predecode, so the core cannot error here;
        // treat a (theoretically unreachable) error as a bail.
        // Issue #10565: `ops_trusted` was certified once at predecode for this
        // exact op list, so this goes through the same trusted/checked dispatch
        // as every other site.
        let outcome = Self::run_typed_ops_dispatch(
            block.ops_trusted,
            &block.ops,
            &block.i64_callees,
            &block.f64_callees,
            // Typed scalar *function* / broadcast blocks never inline a
            // `CallSpecializeI64Slots` / `CallSpecializeF64Slots` site
            // (#10439/#10491 gate emission to loop mode), so there is nothing
            // to resolve; a stray op would bail. Same for the string const
            // pool (Issue #10559): function/broadcast blocks carry no String
            // params in this MVP.
            &[],
            &[],
            &[],
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &[],
            None,
            &mut st,
            &mut self.rng,
        )
        .ok()?;
        let TypedOpsOutcome::EarlyReturn(value) = outcome else {
            // `Completed` (fell past the ops) or a guard bail: nothing was
            // mutated (the block is side-effect-free by predecode
            // construction — see `try_predecode_typed_scalar_function`'s
            // `RandF64` rejection) — re-run through the frame path.
            return None;
        };
        profiler::record_event("CallDirectFastTypedFunctionFromValuesHit");
        Some(value)
    }

    /// Frame-less typed scalar function call from explicit argument values
    /// (Issue #9693): the HOF/broadcast element driver resolves the callee
    /// once per broadcast, so per-element applications execute directly —
    /// no frame, no per-element specialization probe, no argument vec.
    pub(crate) fn try_run_typed_scalar_function_with_args(
        &mut self,
        func_index: usize,
        entry_ip: usize,
        end_ip: usize,
        first: &Value,
        extras: &[Value],
    ) -> Option<Value> {
        if !self.typed_function_cache.contains_key(&entry_ip) {
            let param_slots = self.functions.get(func_index)?.param_slots.clone();
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                entry_ip,
                end_ip,
                self.base_function_count,
                &param_slots,
            );
            self.typed_function_cache.insert(entry_ip, decoded);
            self.enforce_typed_function_cache_limit();
        }
        let Some(Some(block)) = self.typed_function_cache.get(&entry_ip) else {
            return None;
        };
        if block.params.len() != 1 + extras.len() {
            return None;
        }
        let mut st = TypedOpsState::new(0, 0);
        bind_typed_function_param(&block.params[0], first, &self.struct_heap, &mut st)?;
        for (binding, value) in block.params[1..].iter().zip(extras.iter()) {
            bind_typed_function_param(binding, value, &self.struct_heap, &mut st)?;
        }
        let outcome = Self::run_typed_ops_dispatch(
            block.ops_trusted,
            &block.ops,
            &block.i64_callees,
            &block.f64_callees,
            // Typed scalar *function* / broadcast blocks never inline a
            // `CallSpecializeI64Slots` / `CallSpecializeF64Slots` site
            // (#10439/#10491 gate emission to loop mode), so there is nothing
            // to resolve; a stray op would bail. Same for the string const
            // pool (Issue #10559): function/broadcast blocks carry no String
            // params in this MVP.
            &[],
            &[],
            &[],
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &[],
            None,
            &mut st,
            &mut self.rng,
        )
        .ok()?;
        let TypedOpsOutcome::EarlyReturn(value) = outcome else {
            return None;
        };
        Some(value)
    }

    /// Bulk typed-kernel broadcast (Issues #9693/#8797): `f.(A, scalars...)`
    /// where `f`'s dispatched method predecodes to a frame-less typed scalar
    /// function block runs as one Rust loop over the array's raw storage — the
    /// method is dispatched once per broadcast, elements are read directly
    /// from `ArrayData` (no per-element boxing), and each application is a
    /// `run_typed_ops_core` call (no frame, no per-element dispatch). Returns
    /// `None` whenever anything does not fit; the caller keeps the generic
    /// per-element broadcast path. Sound because the array's element type is
    /// concrete and homogeneous, so per-element dispatch cannot differ from
    /// the first element's.
    pub(crate) fn try_broadcast_typed_kernel(
        &mut self,
        f: &Value,
        bargs: &[Value],
    ) -> Option<ArrayValue> {
        if bargs.is_empty() || bargs.len() > TYPED_FUNCTION_COMPLEX_PARAM_CAP {
            return None;
        }
        // Exactly one array argument (the element source); the rest must be
        // plain scalars a typed block can bind.
        let mut array_arg: Option<(usize, ArrayValue)> = None;
        for (i, v) in bargs.iter().enumerate() {
            let wrapped =
                super::value::array_wrapper_value_to_array_value(v, &self.struct_heap).ok()?;
            let as_array = wrapped
                .or_else(|| super::value::native_array_value_ref(v).map(|r| r.borrow().clone()));
            if let Some(a) = as_array {
                if array_arg.is_some() {
                    return None;
                }
                array_arg = Some((i, a));
                continue;
            }
            match v {
                Value::I64(_) | Value::F64(_) => {}
                Value::Struct(si) if si.complex_f64_parts().is_some() => {}
                _ => return None,
            }
        }
        let (apos, arr) = array_arg?;

        enum Elems<'a> {
            C64(&'a [f64]),
            F64(&'a [f64]),
            I64(&'a [i64]),
        }
        let count: usize = arr.shape.iter().product();
        if count == 0 {
            // Empty broadcasts keep the generic path (result eltype inference).
            return None;
        }
        let elems = match (&arr.data, arr.element_type()) {
            (ArrayData::StructF64(v), ArrayElementType::ComplexF64) if v.len() == count * 2 => {
                Elems::C64(v)
            }
            (ArrayData::F64(v), ArrayElementType::F64) if v.len() == count => Elems::F64(v),
            (ArrayData::I64(v), ArrayElementType::I64) if v.len() == count => Elems::I64(v),
            _ => return None,
        };

        // Dispatch once, with a properly-tagged boxed first element.
        let elem0 = arr.get_linear(0).ok()?;
        let mut dispatch_args: Vec<Value> = Vec::with_capacity(bargs.len());
        for (i, v) in bargs.iter().enumerate() {
            dispatch_args.push(if i == apos { elem0.clone() } else { v.clone() });
        }
        let func_index = self.resolve_runtime_callable_function_index(f, &dispatch_args)?;
        let param_slots = self.functions.get(func_index)?.param_slots.clone();
        let (entry, end) = if let Some((entry, end, _local_slot_count)) =
            self.try_specialized_body_for_runtime_call(func_index, &dispatch_args)
        {
            (entry, end)
        } else {
            let info = self.functions.get(func_index)?;
            (info.entry, info.code_end)
        };
        if !self.typed_function_cache.contains_key(&entry) {
            let decoded = try_predecode_typed_scalar_function(
                self.code.as_ref(),
                &self.functions,
                entry,
                end,
                self.base_function_count,
                &param_slots,
            );
            self.typed_function_cache.insert(entry, decoded);
            self.enforce_typed_function_cache_limit();
        }
        let Some(Some(block)) = self.typed_function_cache.get(&entry) else {
            return None;
        };
        if block.params.len() != bargs.len() {
            return None;
        }

        // Bind the fixed scalars once into a template; record how the element
        // parameter binds (its kind must match the array's element type).
        let mut template = TypedOpsState::new(0, 0);
        let mut elem_binding = None;
        for (i, (binding, value)) in block.params.iter().zip(dispatch_args.iter()).enumerate() {
            if i == apos {
                let compatible = matches!(
                    (binding, &elems),
                    (TypedFunctionParamBinding::ComplexF64(_), Elems::C64(_))
                        | (TypedFunctionParamBinding::F64(_), Elems::F64(_))
                        | (TypedFunctionParamBinding::I64(_), Elems::I64(_))
                        | (TypedFunctionParamBinding::Unused, _)
                );
                if !compatible {
                    return None;
                }
                elem_binding = Some(*binding);
            } else {
                bind_typed_function_param(binding, value, &self.struct_heap, &mut template)?;
            }
        }
        let elem_binding = elem_binding?;

        enum Out {
            I(Vec<i64>),
            F(Vec<f64>),
        }
        let mut out: Option<Out> = None;
        let mut st = TypedOpsState::new(template.array_locals.len(), template.str_locals.len());
        st.reset_from(&template);
        for idx in 0..count {
            st.reset_from(&template);
            match (elem_binding, &elems) {
                (TypedFunctionParamBinding::ComplexF64(c), Elems::C64(v)) => {
                    st.complex_params[c] = (v[2 * idx], v[2 * idx + 1]);
                }
                (TypedFunctionParamBinding::F64(local), Elems::F64(v)) => {
                    st.f64_locals[local] = v[idx];
                    st.f64_init[local] = true;
                }
                (TypedFunctionParamBinding::I64(local), Elems::I64(v)) => {
                    st.i64_locals[local] = v[idx];
                    st.i64_init[local] = true;
                }
                (TypedFunctionParamBinding::Unused, _) => {}
                _ => return None,
            }
            let outcome = Self::run_typed_ops_dispatch(
                block.ops_trusted,
                &block.ops,
                &block.i64_callees,
                &block.f64_callees,
                // Broadcast blocks never inline a specialize site (#10439/#10491),
                // nor carry a String const pool (Issue #10559).
                &[],
                &[],
                &[],
                &block.typed_i64_callees,
                &block.typed_f64_callees,
                &[],
                None,
                &mut st,
                &mut self.rng,
            )
            .ok()?;
            let value = match outcome {
                TypedOpsOutcome::EarlyReturn(value) => value,
                _ => return None,
            };
            match (&mut out, value) {
                (None, Value::I64(v)) => {
                    let mut vec = Vec::with_capacity(count);
                    vec.push(v);
                    out = Some(Out::I(vec));
                }
                (None, Value::F64(v)) => {
                    let mut vec = Vec::with_capacity(count);
                    vec.push(v);
                    out = Some(Out::F(vec));
                }
                (Some(Out::I(vec)), Value::I64(v)) => vec.push(v),
                (Some(Out::F(vec)), Value::F64(v)) => vec.push(v),
                _ => return None,
            }
        }

        profiler::record_event("BroadcastTypedKernelHit");
        let shape = arr.shape.clone();
        Some(match out? {
            Out::I(vec) => ArrayValue::memory_first_from_array_data_with_element_type(
                ArrayData::I64(vec),
                shape,
                ArrayElementType::I64,
            ),
            Out::F(vec) => ArrayValue::memory_first_from_array_data_with_element_type(
                ArrayData::F64(vec),
                shape,
                ArrayElementType::F64,
            ),
        })
    }

    #[inline]
    pub(crate) fn execute_i64_function_block(
        block: &I64FunctionBlock,
        args: &[i64],
    ) -> Option<i64> {
        Self::execute_scalar_function_block::<I64Kind>(block, args)
    }

    /// The single frame-less scalar-function-block mini-interpreter (Issue
    /// #10427), generic over the scalar kind `K`. Monomorphizes to one i64 and
    /// one f64 interpreter; the arithmetic, comparison, and profiler-tag details
    /// come entirely from `K: ScalarKind`, so control flow, operand/bool stack
    /// discipline, slot binding, nested-call dispatch, and bail-to-frame guards
    /// are written exactly once. Returns `None` on any guard failure (the caller
    /// falls back to the normal frame path).
    fn execute_scalar_function_block<K: ScalarKind>(
        block: &ScalarFunctionBlock<K::Scalar>,
        args: &[K::Scalar],
    ) -> Option<K::Scalar> {
        if block.slots.len() > SCALAR_FUNCTION_SLOT_CAP || block.ops.len() > MAX_SCALAR_FUNCTION_OPS
        {
            return None;
        }

        let mut locals = [K::Scalar::default(); SCALAR_FUNCTION_SLOT_CAP];
        let mut local_init = [false; SCALAR_FUNCTION_SLOT_CAP];
        for (local, slot) in block.slots.iter().enumerate() {
            if let Some(param_index) = slot.param_index {
                locals[local] = *args.get(param_index)?;
                local_init[local] = true;
            }
        }

        profiler::record_event(K::BLOCK_EVENT);

        let mut value_stack = [K::Scalar::default(); TYPED_LOOP_STACK_CAP];
        let mut bool_stack = [false; TYPED_LOOP_STACK_CAP];
        let mut value_sp = 0usize;
        let mut bool_sp = 0usize;
        let mut op_pc = 0usize;

        while op_pc < block.ops.len() {
            match block.ops[op_pc] {
                ScalarFunctionOp::Push(value) => {
                    push_stack(&mut value_stack, &mut value_sp, value);
                    op_pc += 1;
                }
                ScalarFunctionOp::LoadSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    push_stack(&mut value_stack, &mut value_sp, locals[local]);
                    op_pc += 1;
                }
                ScalarFunctionOp::StoreSlot(local) => {
                    let value = pop_stack(&value_stack, &mut value_sp)?;
                    locals[local] = value;
                    local_init[local] = true;
                    op_pc += 1;
                }
                ScalarFunctionOp::Add => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::add(lhs, rhs));
                    op_pc += 1;
                }
                ScalarFunctionOp::Sub => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::sub(lhs, rhs));
                    op_pc += 1;
                }
                ScalarFunctionOp::Mul => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::mul(lhs, rhs));
                    op_pc += 1;
                }
                ScalarFunctionOp::Div => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::div(lhs, rhs));
                    op_pc += 1;
                }
                ScalarFunctionOp::Neg => {
                    let value = pop_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::neg(value));
                    op_pc += 1;
                }
                ScalarFunctionOp::Abs => {
                    let value = pop_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::abs(value));
                    op_pc += 1;
                }
                ScalarFunctionOp::Rem => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    let value = K::checked_rem(lhs, rhs)?;
                    push_stack(&mut value_stack, &mut value_sp, value);
                    op_pc += 1;
                }
                ScalarFunctionOp::LoadAddSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::add(lhs, locals[local]));
                    op_pc += 1;
                }
                ScalarFunctionOp::LoadSubSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::sub(lhs, locals[local]));
                    op_pc += 1;
                }
                ScalarFunctionOp::LoadMulSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::mul(lhs, locals[local]));
                    op_pc += 1;
                }
                ScalarFunctionOp::LoadDivSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_stack(&value_stack, &mut value_sp)?;
                    push_stack(&mut value_stack, &mut value_sp, K::div(lhs, locals[local]));
                    op_pc += 1;
                }
                ScalarFunctionOp::LoadRemSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_stack(&value_stack, &mut value_sp)?;
                    let value = K::checked_rem(lhs, locals[local])?;
                    push_stack(&mut value_stack, &mut value_sp, value);
                    op_pc += 1;
                }
                ScalarFunctionOp::IncSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let delta = pop_stack(&value_stack, &mut value_sp)?;
                    locals[local] = K::add(locals[local], delta);
                    op_pc += 1;
                }
                ScalarFunctionOp::DecSlot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let delta = pop_stack(&value_stack, &mut value_sp)?;
                    locals[local] = K::sub(locals[local], delta);
                    op_pc += 1;
                }
                ScalarFunctionOp::AddConstSlot(local, delta) => {
                    if !local_init[local] {
                        return None;
                    }
                    locals[local] = K::add(locals[local], delta);
                    op_pc += 1;
                }
                ScalarFunctionOp::AddConstSlotAndJumpIfLe(local, delta, stop_local, target) => {
                    if !local_init[local] || !local_init[stop_local] {
                        return None;
                    }
                    locals[local] = K::add(locals[local], delta);
                    op_pc = if K::eval_relation(
                        locals[local],
                        locals[stop_local],
                        ScalarRelation::Le,
                    ) {
                        target
                    } else {
                        op_pc + 1
                    };
                }
                ScalarFunctionOp::Call(callee_index, arg_count) => {
                    if arg_count > TYPED_LOOP_STACK_CAP {
                        return None;
                    }
                    let callee = block.callees.get(callee_index)?;
                    let mut call_args = [K::Scalar::default(); TYPED_LOOP_STACK_CAP];
                    for index in (0..arg_count).rev() {
                        call_args[index] = pop_stack(&value_stack, &mut value_sp)?;
                    }
                    profiler::record_event(K::NESTED_CALL_EVENT);
                    let value =
                        Self::execute_scalar_function_block::<K>(callee, &call_args[..arg_count])?;
                    push_stack(&mut value_stack, &mut value_sp, value);
                    op_pc += 1;
                }
                ScalarFunctionOp::Cmp(relation) => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    // Not the typed-loop core: #10565's TRUSTED mode applies
                    // only inside `run_typed_ops_core`, so always checked here.
                    push_bool_stack::<false>(
                        &mut bool_stack,
                        &mut bool_sp,
                        K::eval_relation(lhs, rhs, relation),
                    );
                    op_pc += 1;
                }
                ScalarFunctionOp::JumpIfZero(target) => {
                    let cond = pop_bool_stack::<false>(&bool_stack, &mut bool_sp)?;
                    op_pc = if cond { op_pc + 1 } else { target };
                }
                ScalarFunctionOp::JumpIf(relation, target) => {
                    let (lhs, rhs) = pop2_stack(&value_stack, &mut value_sp)?;
                    op_pc = if K::eval_relation(lhs, rhs, relation) {
                        target
                    } else {
                        op_pc + 1
                    };
                }
                ScalarFunctionOp::JumpIfSlots(lhs_local, rhs_local, relation, target) => {
                    if !local_init[lhs_local] || !local_init[rhs_local] {
                        return None;
                    }
                    op_pc = if K::eval_relation(locals[lhs_local], locals[rhs_local], relation) {
                        target
                    } else {
                        op_pc + 1
                    };
                }
                ScalarFunctionOp::Jump(target) => {
                    op_pc = target;
                }
                ScalarFunctionOp::Return => {
                    return pop_stack(&value_stack, &mut value_sp);
                }
            }
        }
        None
    }

    #[doc(hidden)]
    #[inline]
    pub fn execute_f64_function_block(block: &F64FunctionBlock, args: &[f64]) -> Option<f64> {
        Self::execute_scalar_function_block::<F64Kind>(block, args)
    }

    #[inline]
    pub(crate) fn try_consume_i64_eq_branch(&mut self, value: i64) -> bool {
        let ip = self.ip;
        let code = self.code.as_ref();
        let Some(Instr::PushI64(rhs)) = code.get(ip) else {
            return false;
        };
        let Some(compare) = code.get(ip + 1) else {
            return false;
        };

        let fused_target = match compare {
            Instr::JumpIfEqI64(target) => Some((*target, value == *rhs)),
            Instr::JumpIfNeI64(target) => Some((*target, value != *rhs)),
            _ => None,
        };
        if let Some((target, should_jump)) = fused_target {
            profiler::record_event("ExecutableBlock::I64FunctionCompareBranch");
            self.ip = if should_jump { target } else { ip + 2 };
            return true;
        }

        let Some(Instr::JumpIfZero(target)) = code.get(ip + 2) else {
            return false;
        };

        let cond = match compare {
            Instr::EqI64
            | Instr::CallDynamicBinaryBoth(Intrinsic::EqFloat | Intrinsic::EqInt, _) => {
                value == *rhs
            }
            Instr::NeI64
            | Instr::CallDynamicBinaryBoth(Intrinsic::NeFloat | Intrinsic::NeInt, _) => {
                value != *rhs
            }
            _ => return false,
        };

        profiler::record_event("ExecutableBlock::I64FunctionCompareBranch");
        self.ip = if cond { ip + 3 } else { *target };
        true
    }

    #[inline]
    pub(crate) fn try_consume_f64_eq_branch(&mut self, value: f64) -> bool {
        let ip = self.ip;
        let code = self.code.as_ref();
        let Some(Instr::PushF64(rhs)) = code.get(ip) else {
            return false;
        };
        let Some(compare) = code.get(ip + 1) else {
            return false;
        };

        let fused_target = match compare {
            Instr::JumpIfEqF64(target) => Some((*target, value == *rhs)),
            Instr::JumpIfNeF64(target) => Some((*target, value != *rhs)),
            _ => None,
        };
        if let Some((target, should_jump)) = fused_target {
            profiler::record_event("ExecutableBlock::F64FunctionCompareBranch");
            self.ip = if should_jump { target } else { ip + 2 };
            return true;
        }

        let Some(Instr::JumpIfZero(target)) = code.get(ip + 2) else {
            return false;
        };

        let cond = match compare {
            Instr::EqF64 | Instr::CallDynamicBinaryBoth(Intrinsic::EqFloat, _) => value == *rhs,
            Instr::NeF64 | Instr::CallDynamicBinaryBoth(Intrinsic::NeFloat, _) => value != *rhs,
            _ => return false,
        };

        profiler::record_event("ExecutableBlock::F64FunctionCompareBranch");
        self.ip = if cond { ip + 3 } else { *target };
        true
    }

    fn execute_typed_loop_block(
        &mut self,
        block: &TypedLoopBlock,
    ) -> Result<ExecutableBlockResult, super::VmError> {
        if block.array_slots.len() > TYPED_LOOP_SLOT_CAP
            || block.f64_slots.len() > TYPED_LOOP_SLOT_CAP
            || block.i64_slots.len() > TYPED_LOOP_SLOT_CAP
            || block.str_slots.len() > TYPED_LOOP_SLOT_CAP
        {
            return Ok(ExecutableBlockResult::NotExecuted);
        }
        if self.frames.last().is_none() {
            return Ok(ExecutableBlockResult::NotExecuted);
        }

        let mut st = TypedOpsState::new(block.array_slots.len(), block.str_slots.len());
        // Issue #10566(c): where each STORED array local's buffer commits back
        // to on a non-`Bail` outcome. `None` for a read-only local (unchanged
        // #10104 behavior: either the live ExprArgs `ArrayRef` used directly,
        // never written back because nothing ever mutates it, or a throwaway
        // snapshot that is never written back either).
        let mut array_origins: Vec<Option<ArrayWriteOrigin>> = vec![None; block.array_slots.len()];
        // Issue #10566(c): storage identity of every live-in array local
        // (regardless of stored-ness), for the aliasing check below. Two
        // locals that resolve to the SAME underlying storage — one of them
        // stored — must reject the whole block: a stored local's writes only
        // become visible through its own buffer's commit, so a second live-in
        // local that is really the same object would observe stale reads (or,
        // stored/stored, race two independent buffers against one origin).
        let mut array_identities: Vec<Option<ArrayIdentity>> = vec![None; block.array_slots.len()];

        // Issue #10104: resolve each live-in array. Either the ExprArgs native
        // carrier (`load_array_slot`) or, for a general MemoryRef-backed
        // `Vector{T}` struct, a read-only snapshot (`snapshot_read_only_vector`,
        // which reads `struct_heap`). Snapshot-sourced slots are recorded so they
        // are never written back — the recognizer guarantees the loop only reads
        // arrays, so the frame's original struct representation is preserved. The
        // inner scope releases the `frame` borrow before the `&self` snapshot call.
        for (local, slot) in block.array_slots.iter().enumerate() {
            if !slot.live_in {
                continue;
            }
            let resolved = {
                let Some(frame) = self.frames.last() else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                match load_array_slot(frame, slot.slot) {
                    Some(v) => Ok(v),
                    None => match frame.locals_slots.get(slot.slot).and_then(|o| o.clone()) {
                        Some(raw) => Err(raw),
                        None => return Ok(ExecutableBlockResult::NotExecuted),
                    },
                }
            };
            let value = match resolved {
                Ok(v) => {
                    // Issue #10566(c): a native `ArrayValue` created by
                    // `reshape` carries a `shared_parent` — and
                    // `ArrayValue::set` writes THROUGH it into the parent
                    // (`array_value/mutation.rs`). A `Clone` of such a value
                    // clones that parent `Rc` too, so the "private" buffer
                    // would NOT be private: its stores would hit the parent
                    // immediately and survive a `Bail`. Never buffer one; and
                    // since a read-only reshape may equally be the alias of a
                    // stored parent (its `Rc` is a different pointer, so the
                    // identity check below cannot see it), reject the whole
                    // block whenever a store is in play.
                    let shares_parent = v.borrow().shared_parent.is_some();
                    if shares_parent && block.array_slots.iter().any(|s| s.stored) {
                        return Ok(ExecutableBlockResult::NotExecuted);
                    }
                    let len: usize = v.borrow().shape.iter().product();
                    array_identities[local] = Some(ArrayIdentity {
                        ptr: Rc::as_ptr(&v) as *const u8 as usize,
                        native: true,
                        start: 1,
                        len,
                    });
                    if slot.stored {
                        // Issue #10566(c): a private buffer, decoupled from
                        // the frame's shared Rc — `IndexStoreTyped` mutates
                        // this buffer only, never `v` itself, so a `Bail`
                        // (buffer simply dropped) leaves `v` untouched for the
                        // generic re-run. Committed into `v` (the SAME Rc, so
                        // any other alias observes the write) on completion,
                        // early return, or a propagated `VmError`.
                        array_origins[local] = Some(ArrayWriteOrigin::Native(v.clone()));
                        new_array_ref(v.borrow().clone())
                    } else {
                        v
                    }
                }
                // Issue #10566(c): a stored local backed by a MemoryRef Vector
                // struct resolves through the same read-only-snapshot bridge,
                // but ALSO records the struct's BACKING-STORAGE identity so its
                // buffer can be committed back (elementwise, in place, no
                // reallocation) via `write_back_numeric_vector_buffer`, and so
                // the alias check below sees the storage it actually writes to.
                Err(raw) if slot.stored => {
                    let Some(storage) = self.numeric_vector_storage_id(&raw) else {
                        return Ok(ExecutableBlockResult::NotExecuted);
                    };
                    match self.snapshot_numeric_vector_for_store(&raw) {
                        Some((buffer, idx)) => {
                            array_identities[local] = Some(storage);
                            array_origins[local] = Some(ArrayWriteOrigin::StructVector(idx));
                            buffer
                        }
                        None => return Ok(ExecutableBlockResult::NotExecuted),
                    }
                }
                // The snapshot is a throwaway read-only copy that is never written
                // back, so it is only sound for a local that this block never
                // stores into (Issue #10104). Gated per-local (`slot.stored`,
                // already false here — the arm above consumes every
                // `slot.stored` case) rather than block-wide, so a block that
                // mixes a stored local with a read-only MemoryRef-backed
                // local (`y[i] = x[i] + 1`) still natively fast-paths `x`.
                Err(raw) => {
                    // The identity is recorded for the read-only local too:
                    // its snapshot is taken ONCE at entry, so if a stored
                    // local writes the same storage, this local's reads would
                    // go stale mid-block. A storage-id miss on a read-only
                    // local is only safe when NOTHING in this block stores.
                    let storage = self.numeric_vector_storage_id(&raw);
                    if storage.is_none() && block.array_slots.iter().any(|s| s.stored) {
                        return Ok(ExecutableBlockResult::NotExecuted);
                    }
                    match self.snapshot_read_only_numeric_vector(&raw) {
                        Some(snap) => {
                            st.array_snapshot_only[local] = true;
                            array_identities[local] = storage;
                            snap
                        }
                        None => return Ok(ExecutableBlockResult::NotExecuted),
                    }
                }
            };
            if !typed_loop_array_guard(&value) {
                return Ok(ExecutableBlockResult::NotExecuted);
            }
            st.array_locals[local] = Some(value);
            st.array_init[local] = true;
        }
        // Issue #10566(c): pairwise alias rejection over BACKING STORAGE (same
        // storage `Rc` + overlapping element window), not wrapper identity.
        // Only a pair involving at least one STORED local is dangerous — two
        // read-only locals aliasing each other never observe a write.
        for i in 0..array_identities.len() {
            let Some(a) = array_identities[i] else {
                continue;
            };
            for (j, identity) in array_identities.iter().enumerate().skip(i + 1) {
                if !(block.array_slots[i].stored || block.array_slots[j].stored) {
                    continue;
                }
                let Some(b) = identity else {
                    continue;
                };
                if a.overlaps(b) {
                    return Ok(ExecutableBlockResult::NotExecuted);
                }
            }
        }
        let Some(frame) = self.frames.last() else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        for (local, slot) in block.f64_slots.iter().enumerate() {
            if slot.live_in {
                let Some(value) = load_f64_slot(frame, slot.slot) else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                st.f64_locals[local] = value;
                st.f64_init[local] = true;
            }
        }
        for (local, slot) in block.i64_slots.iter().enumerate() {
            if slot.live_in {
                let Some(value) = load_i64_slot(frame, slot.slot) else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                st.i64_locals[local] = value;
                st.i64_init[local] = true;
            }
        }
        // Issue #10559: resolve each live-in String slot. `Value::Str` is
        // `Rc<str>` (Issue #8630), so this clone is a refcount bump — no
        // snapshot-vs-live distinction is needed the way arrays need one.
        for (local, slot) in block.str_slots.iter().enumerate() {
            if slot.live_in {
                let Some(value) = load_str_slot(frame, slot.slot) else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                st.str_locals[local] = Some(value);
                st.str_init[local] = true;
            }
        }

        // Issue #10439: resolve each `CallSpecializeI64Slots` site inlined into
        // this loop against the live specialization cache. `self.frames.last()`
        // borrow above is dead here (NLL), so `&mut self` is free. A miss means
        // the callee has not been specialized to an I64 body yet (typically the
        // very first entry, before its first call), or its specialized body is
        // not I64-decodable — in either case we defer to the generic
        // interpreter, which runs the site correctly and populates the cache for
        // the next entry. Never cache the resolved body in the block: re-reading
        // the live cache each entry inherits the generic path's invalidation.
        let resolved_specialize: Vec<I64FunctionBlock> = if block.specialize_callees.is_empty() {
            Vec::new()
        } else {
            let mut resolved = Vec::with_capacity(block.specialize_callees.len());
            for &(spec_func_index, arg_count) in &block.specialize_callees {
                let Some(callee) = self.resolve_specialize_i64_callee(spec_func_index, arg_count)
                else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                resolved.push(callee);
            }
            resolved
        };
        // Issue #10491: F64 mirror of the resolution above.
        let resolved_specialize_f64: Vec<ResolvedSpecF64Callee> =
            if block.specialize_f64_callees.is_empty() {
                Vec::new()
            } else {
                let mut resolved = Vec::with_capacity(block.specialize_f64_callees.len());
                for &(spec_func_index, arg_count) in &block.specialize_f64_callees {
                    let Some(callee) =
                        self.resolve_specialize_f64_callee(spec_func_index, arg_count)
                    else {
                        return Ok(ExecutableBlockResult::NotExecuted);
                    };
                    resolved.push(callee);
                }
                resolved
            };
        // Issue #10567 (round 2): narrow mixed-arg (Complex, I64) mirror of
        // the resolutions above, resolved against `specialization_mixed_cache`.
        let resolved_specialize_complex_i64: Vec<TypedScalarFunctionBlock> =
            if block.specialize_complex_i64_callees.is_empty() {
                Vec::new()
            } else {
                let mut resolved = Vec::with_capacity(block.specialize_complex_i64_callees.len());
                for &spec_func_index in &block.specialize_complex_i64_callees {
                    let Some(callee) = self.resolve_specialize_complex_i64_callee(spec_func_index)
                    else {
                        return Ok(ExecutableBlockResult::NotExecuted);
                    };
                    resolved.push(callee);
                }
                resolved
            };

        profiler::record_event("ExecutableBlock::TypedLoop");

        // Issue #10516: splice small resolved I64 callee bodies into the op
        // stream for this execution. Entry-time splicing re-reads the live
        // resolution each entry, so cache invalidation is inherited from the
        // specialize path unchanged.
        let inlined_ops = try_inline_i64_callees_into_typed_ops(
            &block.ops,
            &block.i64_callees,
            &resolved_specialize,
            block.i64_slots.len(),
        );
        if inlined_ops.is_some() {
            profiler::record_event("ExecutableBlock::TypedLoopInlineI64Callee");
        }
        let ops: &[TypedLoopOp] = inlined_ops.as_deref().unwrap_or(&block.ops);

        let outcome = match Self::run_typed_ops_dispatch(
            // Certify the EXACT slice about to run. When #10516 entry-time
            // inlining fired, `ops` is the rewritten `inlined_ops` — a
            // different op list, rebuilt on this entry — so it needs its own
            // certification. Otherwise `ops` IS `block.ops`, already certified
            // once at predecode (`ops_trusted`); re-scanning it on every entry
            // is what made Mandelbrot (one entry per pixel) regress.
            match inlined_ops.as_deref() {
                Some(inlined) => certify_typed_ops_trusted(inlined),
                None => block.ops_trusted,
            },
            ops,
            &block.i64_callees,
            &block.f64_callees,
            &resolved_specialize,
            &resolved_specialize_f64,
            &resolved_specialize_complex_i64,
            &block.typed_i64_callees,
            &block.typed_f64_callees,
            &block.str_consts,
            self.memory_budget_bytes,
            &mut st,
            &mut self.rng,
        ) {
            Ok(outcome) => outcome,
            // Issue #10566(c): a genuine `VmError` mid-block (currently only
            // `typed_loop_index_store`'s shape/bounds guard) is NOT a `Bail` —
            // it propagates like any other raised error, and upstream Julia
            // applies every store that precedes the point of the error
            // (observable under an enclosing `try`/`catch`; verified against
            // `julia` directly for the `a[i]=i; s+=a[i]` OOB shape). Commit
            // the transactional array buffers AND the buffered scalar/String
            // locals — exactly the same commit a clean completion performs —
            // before propagating, so a `catch` around this loop observes the
            // same partial state the generic interpreter would have left.
            Err(err) => {
                self.commit_typed_loop_array_buffers(&st, &array_origins);
                if let Some(frame) = self.frames.last_mut() {
                    write_back_typed_loop_scalar_locals(frame, block, &st);
                }
                return Err(err);
            }
        };
        if matches!(outcome, TypedOpsOutcome::Bail) {
            // The frame is untouched mid-block (array buffers are simply
            // dropped, unread — Issue #10566(c)); the interpreter re-runs
            // from the header.
            profiler::record_event("ExecutableBlock::TypedLoopBail");
            return Ok(ExecutableBlockResult::NotExecuted);
        }

        // Issue #10566(c): commit every STORED array local's buffer to its
        // origin (never on `Bail`, handled above).
        self.commit_typed_loop_array_buffers(&st, &array_origins);
        let Some(frame) = self.frames.last_mut() else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        for (local, slot) in block.array_slots.iter().enumerate() {
            // A STORED local already committed via `array_origins` above —
            // never rebind the frame slot for it (that would either break the
            // `Native` origin's Rc-identity guarantee or, for a
            // `StructVector` origin, overwrite the frame's `StructRef` with a
            // native `ArrayRef` it was never typed to hold).
            if array_origins[local].is_some() {
                continue;
            }
            // Issue #10104: never write back a snapshot-sourced array — the frame
            // still holds the original (unmodified) `Vector{T}` struct, and the
            // snapshot is a throwaway read-only copy in a different representation.
            if st.array_init[local] && !st.array_snapshot_only[local] {
                let Some(value) = st.array_locals[local].as_ref().cloned() else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                let _ = frame.set_slot_array(slot.slot, value);
            }
        }
        write_back_typed_loop_scalar_locals(frame, block, &st);

        match outcome {
            // Issue #9654: a Return op fired inside the loop. Route exactly
            // like the interpreter's `Return*` (HOF/broadcast, generator, and
            // caller-frame continuations included). The write-back above is
            // harmless — the frame is about to be popped.
            TypedOpsOutcome::EarlyReturn(value) => self.route_executable_value_return(value),
            TypedOpsOutcome::Completed => {
                self.ip = block.exit_ip;
                Ok(ExecutableBlockResult::Continue)
            }
            TypedOpsOutcome::Bail => Ok(ExecutableBlockResult::NotExecuted),
            // Issue #10536: an uninit-local-load bail fired after an
            // already-applied side effect (`RandF64` — as of Issue #10566(c),
            // `IndexStore*` no longer counts; its write is transactional and
            // buffered). Every local write committed before the bail is
            // already written back
            // above (matching what a partial generic-path iteration would
            // have committed); raise the matching `UndefVarError` directly
            // instead of discarding state and letting the caller re-run the
            // block from the header, which would re-apply the side effect.
            TypedOpsOutcome::UndefLocal { kind, local } => {
                profiler::record_event("ExecutableBlock::TypedLoopUndefLocalAfterSideEffect");
                let slot = match kind {
                    TypedLocalKind::F64 => block.f64_slots.get(local),
                    TypedLocalKind::I64 => block.i64_slots.get(local),
                    TypedLocalKind::Array => block.array_slots.get(local),
                    TypedLocalKind::Str => block.str_slots.get(local),
                }
                .map(|s| s.slot);
                let name = match (slot, self.frames.last()) {
                    (Some(slot), Some(frame)) => self.slot_name_for_frame(frame, slot),
                    _ => format!("slot {local}"),
                };
                let err = super::VmError::UndefVarError(name);
                // Issue #10406 pattern: `run_typed_ops_core` returns this
                // error via a bare `Ok(TypedOpsOutcome::UndefLocal)` /
                // subsequent `Err` conversion here instead of `self.raise`,
                // so an enclosing `try`/`catch` handler would otherwise
                // never be consulted. Route it through the same handler
                // machinery the instruction-level `raise` sites use;
                // `raise` (via `handle_error`) already respects the
                // `eval_dispatch_floor` ancestor-handler guard, so this is
                // safe whether we are driven by `run()` or a nested
                // `run_until_frame_return_inner`.
                if Self::is_catchable_vm_error(&err) {
                    self.raise(err)?;
                    Ok(ExecutableBlockResult::Continue)
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Issue #10566(c): commit every STORED array local's private
    /// transactional buffer back into its `ArrayWriteOrigin`. Called on every
    /// non-`Bail` outcome of `run_typed_ops_core` (`Completed`,
    /// `EarlyReturn`, `UndefLocal`, and a propagated `VmError` alike) —
    /// NEVER on `Bail`, where the buffer is simply dropped unread and the
    /// original array is left untouched for the generic interpreter's re-run.
    /// Best-effort: a resolution miss (`st.array_locals[local]` not actually
    /// initialized, or the struct heap no longer matching the shape the
    /// buffer was snapshotted from — neither reachable in practice, since
    /// nothing in a typed loop's op set can un-initialize a local it already
    /// initialized at block entry or mutate `struct_heap` itself) silently
    /// skips that one local rather than panicking or discarding `outcome`.
    fn commit_typed_loop_array_buffers(
        &mut self,
        st: &TypedOpsState,
        array_origins: &[Option<ArrayWriteOrigin>],
    ) {
        for (local, origin) in array_origins.iter().enumerate() {
            let Some(origin) = origin else { continue };
            if !st.array_init[local] {
                continue;
            }
            let Some(buffer) = st.array_locals[local].as_ref() else {
                continue;
            };
            match origin {
                ArrayWriteOrigin::Native(target) => {
                    // Overwrite the ORIGIN's contents in place — this is the
                    // SAME Rc the frame slot holds, so its identity (and thus
                    // every other live alias of it) is preserved; only the
                    // interior `ArrayValue` is replaced.
                    *target.borrow_mut() = buffer.borrow().clone();
                }
                ArrayWriteOrigin::StructVector(idx) => {
                    let _ = self.write_back_numeric_vector_buffer(*idx, buffer);
                }
            }
        }
    }

    /// Shared typed-op interpreter core (Issues #9654/#9693): executes a
    /// typed op list over borrowed local state. Used by the frame-backed
    /// typed-loop block (entry/write-back against the current frame) and the
    /// frame-less typed scalar function call (entry from call arguments, no
    /// frame at all). A free function taking `rng` explicitly so callers can
    /// borrow the op list out of one `Vm` field while lending `self.rng`
    /// (disjoint field borrows). `Bail` means the caller must discard the
    /// state and fall back (nothing observable was mutated); `EarlyReturn`
    /// carries a fired `Return*` op's value; `Completed` means execution fell
    /// past the ops or took an `Exit` target.
    #[allow(clippy::too_many_arguments)]
    ///
    /// Issue #10565: run `ops` on the trusted (unchecked) executor when
    /// `trusted` says `certify_typed_ops_trusted` accepted THIS EXACT slice,
    /// otherwise on the unchanged checked one. The single place the two
    /// monomorphizations are selected, so no call site can accidentally pass
    /// `TRUSTED = true` for a stream it did not certify.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn run_typed_ops_dispatch(
        trusted: bool,
        ops: &[TypedLoopOp],
        i64_callees: &[I64FunctionBlock],
        f64_callees: &[F64FunctionBlock],
        specialize_callees: &[I64FunctionBlock],
        specialize_f64_callees: &[ResolvedSpecF64Callee],
        // Issue #10567 (round 2): resolved-per-entry callees for
        // `TypedLoopOp::CallSpecializeComplexI64Function`, mirroring
        // `specialize_callees`/`specialize_f64_callees` above. Always empty
        // for the frame-less function/broadcast callers below — the narrow
        // mixed-arg specialize op is emitted in loop mode only.
        specialize_complex_i64_callees: &[TypedScalarFunctionBlock],
        typed_i64_callees: &[TypedScalarFunctionBlock],
        typed_f64_callees: &[TypedScalarFunctionBlock],
        // Issue #10559: compile-time string literal pool for `PushStrConst`.
        str_consts: &[StrRef],
        // Issue #10559: `Vm::memory_budget_bytes`, so `ConcatStr` enforces the
        // same allocation budget the interpreter's `StringConcat` arm does.
        memory_budget_bytes: Option<usize>,
        st: &mut TypedOpsState,
        rng: &mut R,
    ) -> Result<TypedOpsOutcome, super::VmError> {
        if trusted {
            profiler::record_event("ExecutableBlock::TypedLoopTrusted");
            Self::run_typed_ops_core::<true>(
                ops,
                i64_callees,
                f64_callees,
                specialize_callees,
                specialize_f64_callees,
                specialize_complex_i64_callees,
                typed_i64_callees,
                typed_f64_callees,
                str_consts,
                memory_budget_bytes,
                st,
                rng,
            )
        } else {
            profiler::record_event("ExecutableBlock::TypedLoopNotCertified");
            Self::run_typed_ops_core::<false>(
                ops,
                i64_callees,
                f64_callees,
                specialize_callees,
                specialize_f64_callees,
                specialize_complex_i64_callees,
                typed_i64_callees,
                typed_f64_callees,
                str_consts,
                memory_budget_bytes,
                st,
                rng,
            )
        }
    }

    /// `TRUSTED` (Issue #10565): when `true`, every push/pop against the
    /// fixed-capacity i64/f64/bool operand stacks uses the unchecked
    /// primitive (`push_stack_unchecked` / `pop_stack_unchecked`, via the
    /// `push_*_stack::<TRUSTED>` wrappers) instead of the checked one,
    /// skipping the per-op overflow/underflow test that dominates this
    /// function on the coprime-pi kernel (Issue #10515 / #10565). Callers
    /// MUST only pass `TRUSTED = true` after `certify_typed_ops_trusted(ops)`
    /// returned `true` for the EXACT `ops` slice passed here — a different
    /// slice (e.g. after the #10516 entry-time inliner rewrites the stream)
    /// needs its own certification.
    ///
    /// `TRUSTED` changes NOTHING else. Op semantics, the `*_init[local]`
    /// init-before-use checks (which are the bail / `UndefLocal` semantics of
    /// Issues #10504 / #10536, not defensive checks), local-slot indexing, and
    /// the array stack are identical in both modes — one source, monomorphized
    /// twice, so the checked and unchecked paths cannot drift. In
    /// `debug_assertions` builds the trusted instantiation re-derives the
    /// certification and asserts it still holds, so a mis-certification trips
    /// a test instead of reading out of bounds in release.
    fn run_typed_ops_core<const TRUSTED: bool>(
        ops: &[TypedLoopOp],
        i64_callees: &[I64FunctionBlock],
        f64_callees: &[F64FunctionBlock],
        specialize_callees: &[I64FunctionBlock],
        specialize_f64_callees: &[ResolvedSpecF64Callee],
        // Issue #10567 (round 2): resolved-per-entry callees for
        // `TypedLoopOp::CallSpecializeComplexI64Function`, mirroring
        // `specialize_callees`/`specialize_f64_callees` above. Always empty
        // for the frame-less function/broadcast callers below — the narrow
        // mixed-arg specialize op is emitted in loop mode only.
        specialize_complex_i64_callees: &[TypedScalarFunctionBlock],
        typed_i64_callees: &[TypedScalarFunctionBlock],
        typed_f64_callees: &[TypedScalarFunctionBlock],
        // Issue #10559: compile-time string literal pool for `PushStrConst`.
        str_consts: &[StrRef],
        // Issue #10559: `Vm::memory_budget_bytes`, so `ConcatStr` enforces the
        // same allocation budget the interpreter's `StringConcat` arm does.
        memory_budget_bytes: Option<usize>,
        st: &mut TypedOpsState,
        rng: &mut R,
    ) -> Result<TypedOpsOutcome, super::VmError> {
        debug_assert!(
            !TRUSTED || certify_typed_ops_trusted(ops),
            "run_typed_ops_core::<true> on an op stream certify_typed_ops_trusted \
             does not accept — mis-certification at the call site"
        );
        let TypedOpsState {
            array_locals,
            array_init,
            array_snapshot_only: _,
            f64_locals,
            i64_locals,
            f64_init,
            i64_init,
            complex_params,
            str_locals,
            str_init,
        } = st;
        let array_locals: &mut Vec<Option<ArrayRef>> = array_locals;
        let array_init: &mut Vec<bool> = array_init;
        let str_locals: &mut Vec<Option<StrRef>> = str_locals;
        let str_init: &mut Vec<bool> = str_init;
        let mut f64_stack = [0.0; TYPED_LOOP_STACK_CAP];
        let mut i64_stack = [0_i64; TYPED_LOOP_STACK_CAP];
        let mut bool_stack = [false; TYPED_LOOP_STACK_CAP];
        // Issue #10536: true once an in-place side effect (`RandF64`/
        // `IndexStore*`) has been applied this entry. Persists across
        // `'loop_body` iterations (back-edges never reset it) so a later
        // uninit-local-load bail — reachable only on a control-flow path
        // that skips every store to the slot — knows the generic
        // interpreter must NOT re-run the whole block from the header: that
        // would re-apply the side effect (e.g. re-draw `rand()`), shifting
        // the observable RNG stream past a `try`/`catch` around the loop.
        let mut side_effect_applied = false;

        'loop_body: loop {
            let mut array_stack: Vec<ArrayRef> = Vec::with_capacity(TYPED_LOOP_STACK_CAP);
            // Issue #10559: string mini-stack, reset every loop-back iteration
            // like `array_stack` — a bytecode jump target is only ever reached
            // with an empty operand stack (the predecoder's linear per-type
            // depth simulation enforces `stack_is_empty()` at every jump).
            let mut str_stack: Vec<StrRef> = Vec::with_capacity(TYPED_LOOP_STACK_CAP);
            let mut f64_sp = 0usize;
            let mut i64_sp = 0usize;
            let mut bool_sp = 0usize;
            let mut complex_stack = [(0.0_f64, 0.0_f64); COMPLEX_MINI_STACK_CAP];
            let mut complex_sp = 0usize;
            let mut op_pc = 0usize;

            macro_rules! jump_to {
                ($target:expr) => {
                    match $target {
                        TypedLoopTarget::Exit => break 'loop_body,
                        TypedLoopTarget::LoopBack => continue 'loop_body,
                        TypedLoopTarget::Op(target) => {
                            if target >= ops.len() {
                                return Ok(TypedOpsOutcome::Bail);
                            }
                            op_pc = target;
                            continue;
                        }
                    }
                };
            }

            // Issue #10536: every uninit-loop-local-load guard below routes
            // through this macro instead of bailing unconditionally. Once a
            // side effect has been applied this entry, a bail here can no
            // longer be re-run generically from the header (see
            // `side_effect_applied` above), so the caller must raise the
            // matching `UndefVarError` for `$local` directly.
            macro_rules! bail_or_undef_local {
                ($cond:expr, $kind:expr, $local:expr) => {
                    if !$cond {
                        if side_effect_applied {
                            return Ok(TypedOpsOutcome::UndefLocal {
                                kind: $kind,
                                local: $local,
                            });
                        }
                        return Ok(TypedOpsOutcome::Bail);
                    }
                };
            }

            while op_pc < ops.len() {
                let op = &ops[op_pc];
                match *op {
                    TypedLoopOp::LoadArraySlot(local) => {
                        bail_or_undef_local!(array_init[local], TypedLocalKind::Array, local);
                        let Some(value) = array_locals[local].as_ref().cloned() else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_array_stack(&mut array_stack, value)?;
                    }
                    // Issue #10566(b): the runtime half of an elided
                    // identity-rebind `StoreSlotArray` — pop and discard.
                    TypedLoopOp::DropArray => {
                        let Some(_array) = pop_array_stack(&mut array_stack) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                    }
                    TypedLoopOp::IndexStoreI64 => {
                        let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(index) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(array) = pop_array_stack(&mut array_stack) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        // Issue #10566(c): a false return is a BAIL (shape
                        // guard, or an OUT-OF-BOUNDS index whose Julia-level
                        // `BoundsError` only the generic `setindex!` dispatch
                        // can raise) — never an error raised from here.
                        if !typed_loop_index_store(
                            &array,
                            index,
                            Value::I64(value),
                            ArrayElementType::I64,
                        ) {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        // Issue #10566(c): `array` here is the local's private
                        // transactional buffer (`ArrayWriteOrigin`), not the
                        // shared array heap — this write is fully discardable
                        // on a later `Bail`, so it does NOT set
                        // `side_effect_applied` (Issue #10536's guard is for
                        // truly irreversible in-place effects; `RandF64` is
                        // the only one left).
                        push_array_stack(&mut array_stack, array)?;
                    }
                    TypedLoopOp::IndexStoreF64 => {
                        let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(index) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(array) = pop_array_stack(&mut array_stack) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        // Issue #10566(c): see `IndexStoreI64` above — a false
                        // return BAILS (out-of-bounds included).
                        if !typed_loop_index_store(
                            &array,
                            index,
                            Value::F64(value),
                            ArrayElementType::F64,
                        ) {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        push_array_stack(&mut array_stack, array)?;
                    }
                    // Issue #10104: typed 1-D array element read. Pop index + array,
                    // read the element with the fully bounds-checked accessor, and
                    // push it on the matching typed stack. Any out-of-bounds access
                    // or runtime element-type mismatch bails to the interpreter,
                    // which reproduces the exact `BoundsError` / dispatch. The
                    // recognizer guarantees the loop has no other in-place side
                    // effect, so the interpreter's re-run from the header is safe.
                    TypedLoopOp::IndexLoadF64 => {
                        let Some(index) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(array) = pop_array_stack(&mut array_stack) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let loaded = array.borrow().get(&[index]);
                        match loaded {
                            Ok(Value::F64(v)) => {
                                push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, v);
                            }
                            _ => return Ok(TypedOpsOutcome::Bail),
                        }
                    }
                    TypedLoopOp::IndexLoadI64 => {
                        let Some(index) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(array) = pop_array_stack(&mut array_stack) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let loaded = array.borrow().get(&[index]);
                        match loaded {
                            Ok(Value::I64(v)) => {
                                push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, v);
                            }
                            _ => return Ok(TypedOpsOutcome::Bail),
                        }
                    }
                    TypedLoopOp::PushF64(value) => {
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value);
                    }
                    TypedLoopOp::RandF64 => {
                        // Issue #10536: the RNG stream advances here, in
                        // place — a later uninit-local-load bail can no
                        // longer be safely re-run generically from the
                        // header (that would re-draw and shift the stream).
                        side_effect_applied = true;
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, rng.next_f64());
                    }
                    TypedLoopOp::DupF64 => {
                        let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value);
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value);
                    }
                    TypedLoopOp::LoadF64Slot(local) => {
                        bail_or_undef_local!(f64_init[local], TypedLocalKind::F64, local);
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, f64_locals[local]);
                    }
                    TypedLoopOp::StoreF64Slot(local) => {
                        let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        f64_locals[local] = value;
                        f64_init[local] = true;
                    }
                    TypedLoopOp::LoadSquareF64Slot(local) => {
                        bail_or_undef_local!(f64_init[local], TypedLocalKind::F64, local);
                        let value = f64_locals[local];
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value * value);
                    }
                    TypedLoopOp::LoadAddF64Slot(local) => {
                        bail_or_undef_local!(f64_init[local], TypedLocalKind::F64, local);
                        let Some(lhs) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            lhs + f64_locals[local],
                        );
                    }
                    TypedLoopOp::LoadSubF64Slot(local) => {
                        bail_or_undef_local!(f64_init[local], TypedLocalKind::F64, local);
                        let Some(lhs) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            lhs - f64_locals[local],
                        );
                    }
                    TypedLoopOp::LoadMulF64Slot(local) => {
                        bail_or_undef_local!(f64_init[local], TypedLocalKind::F64, local);
                        let Some(lhs) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            lhs * f64_locals[local],
                        );
                    }
                    TypedLoopOp::AddF64 => {
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, lhs + rhs);
                    }
                    TypedLoopOp::SubF64 => {
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, lhs - rhs);
                    }
                    TypedLoopOp::MulF64 => {
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, lhs * rhs);
                    }
                    TypedLoopOp::DivF64 => {
                        // IEEE 754 division (matches the interpreter's `DivF64`):
                        // x/0.0 = ±Inf, 0.0/0.0 = NaN — no bail needed.
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, lhs / rhs);
                    }
                    TypedLoopOp::LoadDivF64Slot(local) => {
                        bail_or_undef_local!(f64_init[local], TypedLocalKind::F64, local);
                        let Some(lhs) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            lhs / f64_locals[local],
                        );
                    }
                    // Issue #9126: `f64[dst] = f64[lhs] + f64[rhs]`, no stack use.
                    TypedLoopOp::AddF64Slots(dst, lhs, rhs) => {
                        // Evaluation order matches `lhs + rhs`: check `lhs`
                        // before `rhs` (Issue #10536).
                        bail_or_undef_local!(f64_init[lhs], TypedLocalKind::F64, lhs);
                        bail_or_undef_local!(f64_init[rhs], TypedLocalKind::F64, rhs);
                        f64_locals[dst] = f64_locals[lhs] + f64_locals[rhs];
                        f64_init[dst] = true;
                    }
                    // Issue #9126: `f64[dst] = f64[lhs] + f64(i64[rhs])`.
                    TypedLoopOp::AddF64I64Slots(dst, lhs, rhs) => {
                        bail_or_undef_local!(f64_init[lhs], TypedLocalKind::F64, lhs);
                        bail_or_undef_local!(i64_init[rhs], TypedLocalKind::I64, rhs);
                        f64_locals[dst] = f64_locals[lhs] + i64_locals[rhs] as f64;
                        f64_init[dst] = true;
                    }
                    TypedLoopOp::NegF64 => {
                        let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, -value);
                    }
                    TypedLoopOp::PushI64(value) => {
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::DupI64 => {
                        let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::ToF64 => {
                        let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value as f64);
                    }
                    TypedLoopOp::LoadI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, i64_locals[local]);
                    }
                    TypedLoopOp::LoadI64SlotToF64(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            i64_locals[local] as f64,
                        );
                    }
                    TypedLoopOp::StoreI64Slot(local) => {
                        let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        i64_locals[local] = value;
                        i64_init[local] = true;
                    }
                    TypedLoopOp::AddI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_add(rhs),
                        );
                    }
                    TypedLoopOp::SubI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_sub(rhs),
                        );
                    }
                    TypedLoopOp::MulI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_mul(rhs),
                        );
                    }
                    TypedLoopOp::ModI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        // Bail to the interpreter on the cases it would raise on /
                        // wrap (`rhs == 0`, `i64::MIN % -1`); the frame is untouched
                        // mid-block, so re-running from the header is correct.
                        let Some(value) = checked_i64_rem(lhs, rhs) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::LoadAddI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        let Some(lhs) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_add(i64_locals[local]),
                        );
                    }
                    TypedLoopOp::LoadSubI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        let Some(lhs) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_sub(i64_locals[local]),
                        );
                    }
                    TypedLoopOp::LoadMulI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        let Some(lhs) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_mul(i64_locals[local]),
                        );
                    }
                    TypedLoopOp::LoadModI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        let Some(lhs) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(value) = checked_i64_rem(lhs, i64_locals[local]) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    // Issue #10516: reset an inlined callee's local to the
                    // uninitialized state at its call boundary.
                    TypedLoopOp::UninitI64Slot(local) => {
                        i64_init[local] = false;
                    }
                    // Issue #10309: frame-less call to a predecoded I64 callee.
                    TypedLoopOp::CallI64Function(callee_index, arg_count) => {
                        if arg_count > TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let Some(callee) = i64_callees.get(callee_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let mut call_args = [0_i64; TYPED_LOOP_STACK_CAP];
                        for index in (0..arg_count).rev() {
                            let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                            else {
                                return Ok(TypedOpsOutcome::Bail);
                            };
                            call_args[index] = value;
                        }
                        profiler::record_event("ExecutableBlock::TypedLoopCallI64Function");
                        let Some(value) =
                            Self::execute_i64_function_block(callee, &call_args[..arg_count])
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    // Issue #10439: frame-less call to a runtime-specialized
                    // untyped callee. `callee` is the *same* predecoded I64 body
                    // the generic `CallSpecializeI64Slots` hit path runs, so the
                    // result is bit-for-bit identical; the callee is pure (no
                    // frame, no I/O), so bailing before or after it is safe.
                    TypedLoopOp::CallSpecializeI64Function(scratch_index, arg_count) => {
                        if arg_count > TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let Some(callee) = specialize_callees.get(scratch_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let mut call_args = [0_i64; TYPED_LOOP_STACK_CAP];
                        for index in (0..arg_count).rev() {
                            let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                            else {
                                return Ok(TypedOpsOutcome::Bail);
                            };
                            call_args[index] = value;
                        }
                        profiler::record_event(
                            "ExecutableBlock::TypedLoopCallSpecializeI64Function",
                        );
                        let Some(value) =
                            Self::execute_i64_function_block(callee, &call_args[..arg_count])
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    // Issue #10567 (round 2): frame-less call to a runtime-
                    // specialized untyped callee reached through the narrow
                    // mixed-arg `f(complex_arg, i64_arg)` shape. `callee` is
                    // resolved per block entry (see
                    // `Vm::resolve_specialize_complex_i64_callee`) against
                    // the same `specialization_mixed_cache` the generic
                    // `CallSpecialize` hit path populates, so the result is
                    // bit-for-bit identical to the frame path; the callee
                    // body is effect-free by `try_predecode_typed_scalar_function`'s
                    // `RandF64` rejection, so bailing before or after it is
                    // safe.
                    TypedLoopOp::CallSpecializeComplexI64Function(scratch_index) => {
                        let Some(callee) = specialize_complex_i64_callees.get(scratch_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(i64_arg) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if complex_sp == 0 {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        complex_sp -= 1;
                        let complex_arg = complex_stack[complex_sp];
                        profiler::record_event(
                            "ExecutableBlock::TypedLoopCallSpecializeComplexI64Function",
                        );
                        let value = match Self::run_typed_scalar_block_with_complex_i64_args(
                            callee,
                            complex_arg,
                            i64_arg,
                            rng,
                        ) {
                            Some(Value::I64(v)) => Some(v),
                            // The call site is typed I64 (the fixed
                            // `mandel_point`-shaped return type); any other
                            // return re-runs generically.
                            _ => None,
                        };
                        let Some(value) = value else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    // Issue #10491: Float64 mirror — frame-less call to a
                    // runtime-specialized untyped callee whose specialized body
                    // is F64-decodable. Same resolve-or-bail contract as the
                    // I64 op; the callee is pure, so bailing before or after
                    // it is safe.
                    TypedLoopOp::CallSpecializeF64Function(scratch_index, arg_count) => {
                        if arg_count > TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let Some(callee) = specialize_f64_callees.get(scratch_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let mut call_args = [0.0_f64; TYPED_LOOP_STACK_CAP];
                        for index in (0..arg_count).rev() {
                            let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                            else {
                                return Ok(TypedOpsOutcome::Bail);
                            };
                            call_args[index] = value;
                        }
                        profiler::record_event(
                            "ExecutableBlock::TypedLoopCallSpecializeF64Function",
                        );
                        let value = match callee {
                            ResolvedSpecF64Callee::F64(block) => {
                                Self::execute_f64_function_block(block, &call_args[..arg_count])
                            }
                            ResolvedSpecF64Callee::Typed(block) => {
                                match Self::run_typed_scalar_block_with_f64_args(
                                    block,
                                    &call_args[..arg_count],
                                    rng,
                                ) {
                                    Some(Value::F64(v)) => Some(v),
                                    // The fused caller site is typed F64; a
                                    // non-F64 return re-runs generically.
                                    _ => None,
                                }
                            }
                        };
                        let Some(value) = value else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value);
                    }
                    // Issue #10309 follow-up: frame-less call to a predecoded F64 callee.
                    TypedLoopOp::CallF64Function(callee_index, arg_count) => {
                        if arg_count > TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let Some(callee) = f64_callees.get(callee_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let mut call_args = [0.0_f64; TYPED_LOOP_STACK_CAP];
                        for index in (0..arg_count).rev() {
                            let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                            else {
                                return Ok(TypedOpsOutcome::Bail);
                            };
                            call_args[index] = value;
                        }
                        profiler::record_event("ExecutableBlock::TypedLoopCallF64Function");
                        let Some(value) =
                            Self::execute_f64_function_block(callee, &call_args[..arg_count])
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value);
                    }
                    // Issue #10542: frame-less call to a predecoded mixed-type
                    // I64-shaped callee (pure-I64 params/return, mixed-type
                    // body) — the fallback `typed_loop_i64_call_op` emits when
                    // the pure-I64 predecoder rejects the body.
                    TypedLoopOp::CallTypedI64Function(callee_index, arg_count) => {
                        if arg_count > TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let Some(callee) = typed_i64_callees.get(callee_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let mut call_args = [0_i64; TYPED_LOOP_STACK_CAP];
                        for index in (0..arg_count).rev() {
                            let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                            else {
                                return Ok(TypedOpsOutcome::Bail);
                            };
                            call_args[index] = value;
                        }
                        profiler::record_event("ExecutableBlock::TypedLoopCallTypedI64Function");
                        let value = match Self::run_typed_scalar_block_with_i64_args(
                            callee,
                            &call_args[..arg_count],
                            rng,
                        ) {
                            Some(Value::I64(v)) => Some(v),
                            // The call site is typed I64; a non-I64 return
                            // re-runs generically.
                            _ => None,
                        };
                        let Some(value) = value else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(&mut i64_stack, &mut i64_sp, value);
                    }
                    // Issue #10542: F64 mirror of `CallTypedI64Function` — a
                    // pure-F64-shaped callee whose body mixes an I64 local
                    // (e.g. an F64 math helper with an I64 loop counter).
                    TypedLoopOp::CallTypedF64Function(callee_index, arg_count) => {
                        if arg_count > TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let Some(callee) = typed_f64_callees.get(callee_index) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let mut call_args = [0.0_f64; TYPED_LOOP_STACK_CAP];
                        for index in (0..arg_count).rev() {
                            let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                            else {
                                return Ok(TypedOpsOutcome::Bail);
                            };
                            call_args[index] = value;
                        }
                        profiler::record_event("ExecutableBlock::TypedLoopCallTypedF64Function");
                        let value = match Self::run_typed_scalar_block_with_f64_args(
                            callee,
                            &call_args[..arg_count],
                            rng,
                        ) {
                            Some(Value::F64(v)) => Some(v),
                            // The call site is typed F64; a non-F64 return
                            // re-runs generically.
                            _ => None,
                        };
                        let Some(value) = value else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, value);
                    }
                    TypedLoopOp::IncI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        let Some(delta) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        i64_locals[local] = i64_locals[local].wrapping_add(delta);
                    }
                    TypedLoopOp::DecI64Slot(local) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        let Some(delta) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        i64_locals[local] = i64_locals[local].wrapping_sub(delta);
                    }
                    TypedLoopOp::AddConstI64Slot(local, delta) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        i64_locals[local] = i64_locals[local].wrapping_add(delta);
                    }
                    TypedLoopOp::LoadAddConstI64Slot(local, delta) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            i64_locals[local].wrapping_add(delta),
                        );
                    }
                    TypedLoopOp::ReturnI64 => {
                        let Some(value) = pop_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        return Ok(TypedOpsOutcome::EarlyReturn(Value::I64(value)));
                    }
                    // Issue #9693: f64 早期 return (frame-less function blocks).
                    TypedLoopOp::ReturnF64 => {
                        let Some(value) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        return Ok(TypedOpsOutcome::EarlyReturn(Value::F64(value)));
                    }
                    // Issue #10645: fused `NewStruct(type_id, 2); ReturnStruct`
                    // — build a 2-`F64`-field struct (e.g. `Complex{Float64}`)
                    // from the top two f64 mini-stack values (`im` then `re`,
                    // LIFO — matching the push order `re` then `i` the source
                    // constructor call always evaluates in) and return it
                    // directly, without the general interpreter's
                    // `struct_defs` scan + `String` allocation.
                    TypedLoopOp::ReturnStructF64x2(type_id) => {
                        let Some(im) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(re) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let struct_name = crate::vm::value::struct_name_for_type_id(type_id)
                            .unwrap_or_else(|| Rc::from(""));
                        return Ok(TypedOpsOutcome::EarlyReturn(Value::Struct(
                            super::StructInstance {
                                type_id,
                                struct_name,
                                values: vec![Value::F64(re), Value::F64(im)],
                            },
                        )));
                    }
                    // Issue #10567 (round 2): materialize a `(re, im)` pair
                    // freshly computed by the loop body into the complex
                    // mini stack, without ever allocating a boxed
                    // `Complex{Float64}` struct — the only consumer this
                    // recognizer allows is `CallSpecializeComplexI64Function`
                    // immediately after, which reads the pair back out
                    // natively.
                    TypedLoopOp::MaterializeComplexF64 => {
                        let Some(im) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(re) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if complex_sp >= COMPLEX_MINI_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        complex_stack[complex_sp] = (re, im);
                        complex_sp += 1;
                    }
                    // Issue #9693: ComplexF64 param decompose ops (frame-less
                    // function blocks; loop-mode blocks never contain these).
                    TypedLoopOp::PushComplexParam(idx) => {
                        if idx >= complex_params.len() || complex_sp >= COMPLEX_MINI_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        complex_stack[complex_sp] = complex_params[idx];
                        complex_sp += 1;
                    }
                    TypedLoopOp::ComplexFieldF64(field) => {
                        if complex_sp == 0 {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        complex_sp -= 1;
                        let (re, im) = complex_stack[complex_sp];
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            if field == 0 { re } else { im },
                        );
                    }
                    TypedLoopOp::StoreComplexParamFieldF64(idx, field, dst) => {
                        if idx >= complex_params.len() {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let (re, im) = complex_params[idx];
                        f64_locals[dst] = if field == 0 { re } else { im };
                        f64_init[dst] = true;
                    }
                    // Issue #9654: predecode-fused 3-address superinstructions.
                    TypedLoopOp::PushMulF64Slots(a, b) => {
                        bail_or_undef_local!(f64_init[a], TypedLocalKind::F64, a);
                        bail_or_undef_local!(f64_init[b], TypedLocalKind::F64, b);
                        push_f64_stack::<TRUSTED>(
                            &mut f64_stack,
                            &mut f64_sp,
                            f64_locals[a] * f64_locals[b],
                        );
                    }
                    TypedLoopOp::PushSumSquaresF64Slots(a, b) => {
                        bail_or_undef_local!(f64_init[a], TypedLocalKind::F64, a);
                        bail_or_undef_local!(f64_init[b], TypedLocalKind::F64, b);
                        let (x, y) = (f64_locals[a], f64_locals[b]);
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, x * x + y * y);
                    }
                    TypedLoopOp::PushDiffSquaresF64Slots(a, b) => {
                        bail_or_undef_local!(f64_init[a], TypedLocalKind::F64, a);
                        bail_or_undef_local!(f64_init[b], TypedLocalKind::F64, b);
                        let (x, y) = (f64_locals[a], f64_locals[b]);
                        push_f64_stack::<TRUSTED>(&mut f64_stack, &mut f64_sp, x * x - y * y);
                    }
                    TypedLoopOp::CopyF64Slots(dst, src) => {
                        bail_or_undef_local!(f64_init[src], TypedLocalKind::F64, src);
                        f64_locals[dst] = f64_locals[src];
                        f64_init[dst] = true;
                    }
                    TypedLoopOp::CopyI64Slots(dst, src) => {
                        bail_or_undef_local!(i64_init[src], TypedLocalKind::I64, src);
                        i64_locals[dst] = i64_locals[src];
                        i64_init[dst] = true;
                    }
                    TypedLoopOp::AddF64SlotStore(src, dst) => {
                        bail_or_undef_local!(f64_init[src], TypedLocalKind::F64, src);
                        let Some(lhs) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        f64_locals[dst] = lhs + f64_locals[src];
                        f64_init[dst] = true;
                    }
                    // Issue #10532: execute the fused Complex{Float64} recurrence
                    // directly, avoiding the per-instruction dispatch of the
                    // SROA'd `z = z*z + c` expansion.
                    TypedLoopOp::ComplexMulAddAssign {
                        z_re,
                        z_im,
                        c_re,
                        c_im,
                    } => {
                        bail_or_undef_local!(f64_init[z_re], TypedLocalKind::F64, z_re);
                        bail_or_undef_local!(f64_init[z_im], TypedLocalKind::F64, z_im);
                        bail_or_undef_local!(f64_init[c_re], TypedLocalKind::F64, c_re);
                        bail_or_undef_local!(f64_init[c_im], TypedLocalKind::F64, c_im);
                        let zr = f64_locals[z_re];
                        let zi = f64_locals[z_im];
                        let cr = f64_locals[c_re];
                        let ci = f64_locals[c_im];
                        f64_locals[z_re] = zr * zr - zi * zi + cr;
                        f64_locals[z_im] = 2.0 * zr * zi + ci;
                    }
                    TypedLoopOp::JumpIfNotF64Const(relation, rhs, target) => {
                        let Some(lhs) = pop_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if !eval_ordered_f64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::CmpI64(relation) => {
                        let Some((lhs, rhs)) = pop2_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_bool_stack::<TRUSTED>(
                            &mut bool_stack,
                            &mut bool_sp,
                            eval_i64_relation(lhs, rhs, relation),
                        );
                    }
                    TypedLoopOp::CmpF64(relation) => {
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_bool_stack::<TRUSTED>(
                            &mut bool_stack,
                            &mut bool_sp,
                            eval_f64_relation(lhs, rhs, relation),
                        );
                    }
                    TypedLoopOp::JumpIfZero(target) => {
                        let Some(cond) = pop_bool_stack::<TRUSTED>(&bool_stack, &mut bool_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if !cond {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfI64(relation, target) => {
                        let Some((lhs, rhs)) = pop2_i64_stack::<TRUSTED>(&i64_stack, &mut i64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if eval_i64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfI64Slots(lhs_local, rhs_local, relation, target) => {
                        bail_or_undef_local!(i64_init[lhs_local], TypedLocalKind::I64, lhs_local);
                        bail_or_undef_local!(i64_init[rhs_local], TypedLocalKind::I64, rhs_local);
                        if eval_i64_relation(i64_locals[lhs_local], i64_locals[rhs_local], relation)
                        {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::AddConstI64SlotAndJumpIfLe(local, delta, stop_local, target) => {
                        bail_or_undef_local!(i64_init[local], TypedLocalKind::I64, local);
                        bail_or_undef_local!(i64_init[stop_local], TypedLocalKind::I64, stop_local);
                        i64_locals[local] = i64_locals[local].wrapping_add(delta);
                        if i64_locals[local] <= i64_locals[stop_local] {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfF64(relation, target) => {
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if eval_f64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfNotF64(relation, target) => {
                        let Some((lhs, rhs)) = pop2_f64_stack::<TRUSTED>(&f64_stack, &mut f64_sp)
                        else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if !eval_ordered_f64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::Jump(target) => jump_to!(target),
                    // Issue #10559: String slot reads/writes + accumulation.
                    TypedLoopOp::PushStrConst(idx) => {
                        let Some(s) = str_consts.get(idx) else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if str_stack.len() >= TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        str_stack.push(s.clone());
                    }
                    TypedLoopOp::LoadStrSlot(local) => {
                        bail_or_undef_local!(str_init[local], TypedLocalKind::Str, local);
                        let Some(value) = str_locals[local].clone() else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if str_stack.len() >= TYPED_LOOP_STACK_CAP {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        str_stack.push(value);
                    }
                    TypedLoopOp::StoreStrSlot(local) => {
                        let Some(value) = str_stack.pop() else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        str_locals[local] = Some(value);
                        str_init[local] = true;
                    }
                    // Mirrors `Instr::StringConcat`/`Instr::ConcatStrings` for
                    // the all-`String`-operand case the recognizer guarantees
                    // (Issue #10559): pop `n` strings (bottom-to-top program
                    // order), byte-concatenate, push the result. This is the
                    // one typed string op that allocates — matching exactly
                    // the allocation the interpreter's general path pays for
                    // the same `*`/interpolation expression, no more.
                    TypedLoopOp::ConcatStr(n) => {
                        if str_stack.len() < n {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let start = str_stack.len() - n;
                        let total_len = str_stack[start..]
                            .iter()
                            .try_fold(0usize, |acc, part| acc.checked_add(part.len()));
                        // Length overflow, or over the VM memory budget: bail so
                        // the generic interpreter re-runs the block and raises the
                        // exact `VmError::OutOfMemory` its `StringConcat` /
                        // `ConcatStrings` arm raises (Issue #10559). Never allocate
                        // past the budget on the fast path — that would let a typed
                        // loop succeed where the interpreter errors.
                        let Some(total_len) = total_len else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        if memory_budget_bytes.is_some_and(|limit| total_len > limit) {
                            return Ok(TypedOpsOutcome::Bail);
                        }
                        let mut joined = String::with_capacity(total_len);
                        for part in &str_stack[start..] {
                            joined.push_str(part);
                        }
                        str_stack.truncate(start);
                        str_stack.push(StrRef::from(joined.into_boxed_str()));
                    }
                    TypedLoopOp::EqStr => {
                        let Some(rhs) = str_stack.pop() else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        let Some(lhs) = str_stack.pop() else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_bool_stack::<TRUSTED>(&mut bool_stack, &mut bool_sp, lhs == rhs);
                    }
                    // Unicode codepoint count, NOT byte length — see the
                    // `TypedLoopOp::StrLen` doc comment.
                    TypedLoopOp::StrLen => {
                        let Some(s) = str_stack.pop() else {
                            return Ok(TypedOpsOutcome::Bail);
                        };
                        push_i64_stack::<TRUSTED>(
                            &mut i64_stack,
                            &mut i64_sp,
                            s.chars().count() as i64,
                        );
                    }
                }
                op_pc += 1;
            }
            break;
        }
        Ok(TypedOpsOutcome::Completed)
    }

    fn route_executable_value_return(
        &mut self,
        value: Value,
    ) -> Result<ExecutableBlockResult, super::VmError> {
        match self.route_value_return(value)? {
            super::exec::return_ops::ValueReturnRouting::Handled => {
                Ok(ExecutableBlockResult::Continue)
            }
            super::exec::return_ops::ValueReturnRouting::Exit(value) => {
                Ok(ExecutableBlockResult::Exit(value))
            }
        }
    }
}

/// Bind one call argument into `TypedOpsState` per its predecoded binding
/// (Issue #9693). `None` = type mismatch — the caller must fall back to the
/// frame path (e.g. a Complex{Int} argument, whose fields the frame path's
/// GetField preserves; strict F64 extraction per Issue #9167).
fn bind_typed_function_param(
    binding: &TypedFunctionParamBinding,
    value: &Value,
    struct_heap: &[super::StructInstance],
    st: &mut TypedOpsState,
) -> Option<()> {
    match binding {
        TypedFunctionParamBinding::I64(local) => match value {
            Value::I64(v) => {
                st.i64_locals[*local] = *v;
                st.i64_init[*local] = true;
            }
            _ => return None,
        },
        TypedFunctionParamBinding::F64(local) => match value {
            Value::F64(v) => {
                st.f64_locals[*local] = *v;
                st.f64_init[*local] = true;
            }
            _ => return None,
        },
        TypedFunctionParamBinding::ComplexF64(idx) => {
            let parts = match value {
                Value::Struct(instance) => instance.complex_f64_parts(),
                Value::StructRef(heap_idx) => struct_heap
                    .get(*heap_idx)
                    .and_then(|instance| instance.complex_f64_parts()),
                _ => None,
            }?;
            st.complex_params[*idx] = parts;
        }
        TypedFunctionParamBinding::Unused => {}
    }
    Some(())
}

fn load_i64_slot(frame: &super::frame::Frame, slot: usize) -> Option<i64> {
    frame.slot_i64(slot)
}

fn load_f64_slot(frame: &super::frame::Frame, slot: usize) -> Option<f64> {
    frame.slot_f64(slot)
}

fn load_array_slot(frame: &super::frame::Frame, slot: usize) -> Option<ArrayRef> {
    frame.slot_array(slot).cloned()
}

// Issue #10559: only the plain `Value::Str` (valid-UTF-8) variant qualifies —
// `Value::StrBytes` (Issue #8995, non-UTF-8 raw byte strings) is out of scope
// for the typed-loop String ops and falls back to the generic interpreter.
fn load_str_slot(frame: &super::frame::Frame, slot: usize) -> Option<StrRef> {
    frame.slot_str(slot).cloned()
}

/// Issue #10566(c): write every buffered f64/i64/String local back into the
/// frame. Factored out of `execute_typed_loop_block` so the SAME write-back
/// runs both on a clean/early-return completion AND on a propagated
/// `VmError` mid-block (upstream applies stores that precede an error,
/// observable under `try`/`catch` — array buffers get the analogous
/// treatment via `commit_typed_loop_array_buffers`, called separately since
/// it needs `&mut self` for the `StructVector` origin case). Best-effort: a
/// `str_init[local]` true with `str_locals[local]` still `None` is not
/// reachable in practice (`TypedOpsState` always sets both together) and is
/// silently skipped rather than discarding the whole write-back.
fn write_back_typed_loop_scalar_locals(
    frame: &mut super::frame::Frame,
    block: &TypedLoopBlock,
    st: &TypedOpsState,
) {
    for (local, slot) in block.f64_slots.iter().enumerate() {
        if st.f64_init[local] {
            let _ = frame.set_slot_f64(slot.slot, st.f64_locals[local]);
        }
    }
    for (local, slot) in block.i64_slots.iter().enumerate() {
        if st.i64_init[local] {
            let _ = frame.set_slot_i64(slot.slot, st.i64_locals[local]);
        }
    }
    for (local, slot) in block.str_slots.iter().enumerate() {
        if st.str_init[local] {
            if let Some(value) = st.str_locals[local].clone() {
                let _ = frame.set_slot_value(slot.slot, Value::Str(value));
            }
        }
    }
}

/// Issue #10104: the `FunctionInfo` whose code range contains `ip`. Used only at
/// predecode time (rarely — when a typed loop candidate contains an array read)
/// to recover an array param's static element type.
fn enclosing_function(functions: &[Rc<FunctionInfo>], ip: usize) -> Option<&Rc<FunctionInfo>> {
    functions
        .iter()
        .find(|f| f.code_start <= ip && ip < f.code_end)
}

/// Issue #10104: the concrete element type of an array-typed positional
/// parameter stored in `slot`, when it is a 1-D `Vector{Float64}` or
/// `Vector{Int64}`. Returns `None` for locals, non-array params, or any other
/// element type, so the typed `IndexLoad*` recognizer conservatively falls back.
fn param_array_element_type(func: &FunctionInfo, slot: usize) -> Option<ArrayElementType> {
    use subset_julia_vm_types::types::JuliaType;
    let param_index = func.param_slots.iter().position(|&s| s == slot)?;
    match func.param_julia_types.get(param_index)? {
        JuliaType::VectorOf(inner) => match **inner {
            JuliaType::Float64 => Some(ArrayElementType::F64),
            JuliaType::Int64 => Some(ArrayElementType::I64),
            _ => None,
        },
        _ => None,
    }
}

fn typed_loop_array_guard(array: &ArrayRef) -> bool {
    let borrow = array.borrow();
    borrow.shape.len() == 1
        && matches!(
            borrow.element_type(),
            ArrayElementType::I64 | ArrayElementType::F64
        )
}

/// Issue #10566(c): store one element into a typed-loop array buffer.
/// `false` means BAIL (never raise): the shape/element-type guard failed, or
/// the index is OUT OF BOUNDS.
///
/// The out-of-bounds case is the load-bearing one and mirrors the `IndexLoad*`
/// arms (Issue #10104) exactly. `ArrayValue::set` reports an out-of-range
/// index as `VmError::IndexOutOfBounds` ("Index [4] out of bounds for array
/// with shape [3]"), which is an INTERNAL shape error — NOT the Julia-level
/// `BoundsError` upstream raises and user code catches (the same trap PR
/// #10756 hit in blocker (a)'s delegation path, pinned by the fixture
/// `array_specialized_store_bounds_10566`). So the typed path must not
/// perform (or report) the store at all: bail, let the generic interpreter
/// re-run the whole block from the header, and let ITS `setindex!` dispatch
/// raise the upstream-compatible `BoundsError` after applying exactly the
/// in-bounds stores that precede it. This is only sound because the typed
/// path's stores are transactional — the discarded buffer means the generic
/// re-run starts from pristine arrays and cannot double-apply.
fn typed_loop_index_store(
    array: &ArrayRef,
    index: i64,
    value: Value,
    expected_element_type: ArrayElementType,
) -> bool {
    let mut borrow = array.borrow_mut();
    if borrow.shape.len() != 1 || borrow.element_type() != expected_element_type {
        return false;
    }
    borrow.set(&[index], value).is_ok()
}

fn push_array_stack(stack: &mut Vec<ArrayRef>, value: ArrayRef) -> Result<(), super::VmError> {
    if stack.len() >= TYPED_LOOP_STACK_CAP {
        return Err(super::VmError::InternalError(
            "typed loop array stack overflow".to_string(),
        ));
    }
    stack.push(value);
    Ok(())
}

fn pop_array_stack(stack: &mut Vec<ArrayRef>) -> Option<ArrayRef> {
    stack.pop()
}

/// Push onto a fixed-capacity operand stack (Issue #10427; generic over the
/// element type). The named `push_i64_stack` / `push_f64_stack` wrappers keep
/// the typed-loop and complex-escape call sites unchanged.
#[inline]
fn push_stack<T: Copy>(stack: &mut [T; TYPED_LOOP_STACK_CAP], sp: &mut usize, value: T) {
    debug_assert!(*sp < TYPED_LOOP_STACK_CAP);
    stack[*sp] = value;
    *sp += 1;
}

#[inline]
fn pop_stack<T: Copy>(stack: &[T; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<T> {
    if *sp == 0 {
        return None;
    }
    *sp -= 1;
    Some(stack[*sp])
}

#[inline]
fn pop2_stack<T: Copy>(stack: &[T; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<(T, T)> {
    let rhs = pop_stack(stack, sp)?;
    let lhs = pop_stack(stack, sp)?;
    Some((lhs, rhs))
}

/// Issue #10565: unchecked sibling of `push_stack`. Reachable ONLY through a
/// `push_*_stack::<true>` wrapper, i.e. only from `run_typed_ops_core::<true>`,
/// which its callers gate on `certify_typed_ops_trusted` accepting the exact
/// op stream being run.
#[inline(always)]
fn push_stack_unchecked<T: Copy>(stack: &mut [T; TYPED_LOOP_STACK_CAP], sp: &mut usize, value: T) {
    debug_assert!(
        *sp < TYPED_LOOP_STACK_CAP,
        "trusted typed-loop stack overflow: certify_typed_ops_trusted mis-certified this op stream"
    );
    // SAFETY: `certify_typed_ops_trusted` returned true for the op stream now
    // executing. Its linear depth walk proves every push in that stream happens
    // at a depth strictly below `TYPED_LOOP_STACK_CAP` (condition (2)), and its
    // jump-agreement rule (condition (3)) makes that walk a sound model of every
    // reachable state — so `*sp < TYPED_LOOP_STACK_CAP` holds here.
    unsafe {
        *stack.get_unchecked_mut(*sp) = value;
    }
    *sp += 1;
}

/// Issue #10565: unchecked sibling of `pop_stack`. Same certification contract
/// as `push_stack_unchecked`.
#[inline(always)]
fn pop_stack_unchecked<T: Copy>(stack: &[T; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> T {
    debug_assert!(
        *sp > 0,
        "trusted typed-loop stack underflow: certify_typed_ops_trusted mis-certified this op stream"
    );
    *sp -= 1;
    // SAFETY: as above — the certification proves this pop never runs at depth
    // 0 (condition (1)), so the decremented `*sp` indexes a slot written by an
    // earlier, still-unpopped push: in bounds and initialized.
    unsafe { *stack.get_unchecked(*sp) }
}

#[inline(always)]
fn pop2_stack_unchecked<T: Copy>(stack: &[T; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> (T, T) {
    let rhs = pop_stack_unchecked(stack, sp);
    let lhs = pop_stack_unchecked(stack, sp);
    (lhs, rhs)
}

/// Issue #10565: `TRUSTED` picks the unchecked primitive over the checked one.
/// Both modes go through this single body, so the checked and trusted paths
/// cannot drift — only the inner primitive differs. `TRUSTED = false` is what
/// every uncertified block (and `execute_scalar_function_block`, via the bare
/// `push_stack`/`pop_stack`) keeps using.
#[inline(always)]
fn push_f64_stack<const TRUSTED: bool>(
    stack: &mut [f64; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
    value: f64,
) {
    if TRUSTED {
        push_stack_unchecked(stack, sp, value)
    } else {
        push_stack(stack, sp, value)
    }
}

#[inline(always)]
fn pop_f64_stack<const TRUSTED: bool>(
    stack: &[f64; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
) -> Option<f64> {
    if TRUSTED {
        Some(pop_stack_unchecked(stack, sp))
    } else {
        pop_stack(stack, sp)
    }
}

#[inline(always)]
fn pop2_f64_stack<const TRUSTED: bool>(
    stack: &[f64; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
) -> Option<(f64, f64)> {
    if TRUSTED {
        Some(pop2_stack_unchecked(stack, sp))
    } else {
        pop2_stack(stack, sp)
    }
}

#[inline(always)]
fn push_i64_stack<const TRUSTED: bool>(
    stack: &mut [i64; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
    value: i64,
) {
    if TRUSTED {
        push_stack_unchecked(stack, sp, value)
    } else {
        push_stack(stack, sp, value)
    }
}

#[inline(always)]
fn pop_i64_stack<const TRUSTED: bool>(
    stack: &[i64; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
) -> Option<i64> {
    if TRUSTED {
        Some(pop_stack_unchecked(stack, sp))
    } else {
        pop_stack(stack, sp)
    }
}

#[inline(always)]
fn pop2_i64_stack<const TRUSTED: bool>(
    stack: &[i64; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
) -> Option<(i64, i64)> {
    if TRUSTED {
        Some(pop2_stack_unchecked(stack, sp))
    } else {
        pop2_stack(stack, sp)
    }
}

#[inline(always)]
fn push_bool_stack<const TRUSTED: bool>(
    stack: &mut [bool; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
    value: bool,
) {
    if TRUSTED {
        push_stack_unchecked(stack, sp, value)
    } else {
        push_stack(stack, sp, value)
    }
}

#[inline(always)]
fn pop_bool_stack<const TRUSTED: bool>(
    stack: &[bool; TYPED_LOOP_STACK_CAP],
    sp: &mut usize,
) -> Option<bool> {
    if TRUSTED {
        Some(pop_stack_unchecked(stack, sp))
    } else {
        pop_stack(stack, sp)
    }
}

fn checked_i64_rem(lhs: i64, rhs: i64) -> Option<i64> {
    if rhs == 0 || (lhs == i64::MIN && rhs == -1) {
        return None;
    }
    Some(lhs % rhs)
}

fn eval_i64_relation(lhs: i64, rhs: i64, relation: I64Relation) -> bool {
    match relation {
        I64Relation::Eq => lhs == rhs,
        I64Relation::Ne => lhs != rhs,
        I64Relation::Lt => lhs < rhs,
        I64Relation::Gt => lhs > rhs,
        I64Relation::Le => lhs <= rhs,
        I64Relation::Ge => lhs >= rhs,
    }
}

fn eval_f64_relation(lhs: f64, rhs: f64, relation: F64Relation) -> bool {
    match relation {
        F64Relation::Eq => lhs == rhs,
        F64Relation::Ne => lhs != rhs,
        F64Relation::Lt => lhs < rhs,
        F64Relation::Gt => lhs > rhs,
        F64Relation::Le => lhs <= rhs,
        F64Relation::Ge => lhs >= rhs,
    }
}

fn eval_ordered_f64_relation(lhs: f64, rhs: f64, relation: F64Relation) -> bool {
    match relation {
        F64Relation::Eq => lhs == rhs,
        F64Relation::Ne => lhs != rhs,
        F64Relation::Lt => matches!(lhs.partial_cmp(&rhs), Some(Ordering::Less)),
        F64Relation::Gt => matches!(lhs.partial_cmp(&rhs), Some(Ordering::Greater)),
        F64Relation::Le => matches!(
            lhs.partial_cmp(&rhs),
            Some(Ordering::Less | Ordering::Equal)
        ),
        F64Relation::Ge => {
            matches!(
                lhs.partial_cmp(&rhs),
                Some(Ordering::Greater | Ordering::Equal)
            )
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use crate::rng::StableRng;
    use crate::test_runtime::compile_source_with_cache;
    use crate::vm::value::Value;

    use super::*;

    fn compile_source(src: &str) -> super::super::CompiledProgram {
        compile_source_with_cache(src)
    }

    #[test]
    fn loop_recognizer_registry_is_the_predecode_pipeline() {
        // Issue #6829: predecode is driven by the ordered `LOOP_RECOGNIZERS`
        // registry, so adding a new optimized shape is a registry append, not a
        // predecode-control-flow edit. Every registered recognizer follows the
        // same `(code, ip, end) -> Option<ExecutableBlock>` match/validate/build
        // contract. Here we confirm the registry exists and that a known shape
        // is recognized *through* it. The Euclidean-modulo special case was
        // retired (Issue #10310 / #10532): the gcd and ComplexF64 Mandelbrot
        // special-case recognizers are gone; the general typed-loop recognizer
        // (`TypedLoopBlock`) is the only remaining entry.
        assert_eq!(LOOP_RECOGNIZERS.len(), 1);
        let gcd = compile_source(
            "function g(a, b)\n    while b != 0\n        t = b\n        b = a % b\n        a = t\n    end\n    a\nend\n\ng(48, 18)\n",
        );
        let matched = (0..gcd.code.len()).any(|ip| {
            LOOP_RECOGNIZERS.iter().any(|recognize| {
                recognize(
                    &gcd.code,
                    &gcd.functions,
                    ip,
                    gcd.code.len(),
                    gcd.base_function_count,
                )
                .is_some()
            })
        });
        assert!(matched, "gcd loop should be recognized via the registry");
    }

    // Issue #10310: the Euclidean-modulo special case (`EuclideanModuloI64Function`
    // / `EuclideanModuloI64LoopBlock`) was retired. `TypedLoopOp` already covers
    // `ModI64`/`LoadModI64Slot`/slot loads-stores, so the coprime-gcd loop below
    // is recognized and executed by the general typed-loop path. These tests
    // (previously euclidean-specific) now assert that general-path coverage.
    #[test]
    fn predecodes_gcd_loop_via_general_typed_loop_path_10310() {
        let compiled = compile_source(
            r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

mygcd(48, 18)
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(executable.len() >= 1);
        assert!(
            executable.has_typed_loop(),
            "gcd loop should be predecoded via the general TypedLoopBlock path"
        );
    }

    #[test]
    fn general_typed_loop_gcd_executes_and_returns_expected_value_10310() {
        let compiled = compile_source(
            r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

mygcd(48, 18)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        assert!(vm.executable.len() >= 1);
        let result = vm.run().expect("run");
        assert!(matches!(result, Value::I64(6)));
    }

    #[test]
    fn general_typed_loop_gcd_handles_zero_second_operand_10310() {
        let compiled = compile_source(
            r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

mygcd(7, 0)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        assert!(vm.executable.len() >= 1);
        let result = vm.run().expect("run");
        assert!(matches!(result, Value::I64(7)));
    }

    /// Issue #10104: a read-only `Vector{Float64}` reduction loop
    /// (`for i in 1:length(x); s += x[i]; end`) is recognized as a typed loop
    /// AND executes on the native block path (`executable_block_runs > 0`),
    /// producing the same result as the interpreter. The array param is a
    /// general MemoryRef-backed struct, resolved via the read-only snapshot
    /// bridge rather than the ExprArgs carrier.
    #[test]
    fn typed_array_read_reduction_executes_natively_issue_10104() {
        let compiled = compile_source(
            r#"
function asum(x::Vector{Float64})
    s = 0.0
    for i in 1:length(x)
        s += x[i]
    end
    s
end
asum(collect(1.0:1000.0))
"#,
        );
        super::super::stack_metrics::set_stack_vm_metrics_forced(true);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        let metrics = vm.stack_vm_metrics();
        super::super::stack_metrics::set_stack_vm_metrics_forced(false);
        assert!(
            matches!(result, Value::F64(v) if (v - 500500.0).abs() < 1e-6),
            "unexpected result {result:?}"
        );
        assert!(
            metrics.map(|m| m.executable_block_runs).unwrap_or(0) > 0,
            "the typed array-read loop must execute on the native block path"
        );
    }

    /// Issue #10104: an `Int64` array reduction takes the same native path and
    /// preserves integer semantics.
    #[test]
    fn typed_array_read_i64_reduction_executes_natively_issue_10104() {
        let compiled = compile_source(
            r#"
function isum(x::Vector{Int64})
    s = 0
    for i in 1:length(x)
        s += x[i]
    end
    s
end
isum(collect(1:1000))
"#,
        );
        super::super::stack_metrics::set_stack_vm_metrics_forced(true);
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        let metrics = vm.stack_vm_metrics();
        super::super::stack_metrics::set_stack_vm_metrics_forced(false);
        assert!(
            matches!(result, Value::I64(500500)),
            "unexpected result {result:?}"
        );
        assert!(
            metrics.map(|m| m.executable_block_runs).unwrap_or(0) > 0,
            "the typed array-read loop must execute on the native block path"
        );
    }

    #[test]
    fn predecodes_typed_float_loop_pattern() {
        let compiled = compile_source(
            r#"
function mandel_point(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    iter = 0
    while zr * zr + zi * zi <= 4.0 && iter < maxiter
        zr2 = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = zr2
        iter = iter + 1
    end
    iter
end

mandel_point(0.0, 0.0, 10)
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(executable.has_typed_loop());
    }

    #[test]
    fn predecodes_typed_float_loop_with_division_issue_8183() {
        // Issue #8183: a Float64 scalar loop containing `/` (DivF64) must be
        // recognized as a native typed loop. `DivF64` was previously absent from
        // the typed-loop IR, so the recognizer bailed and the loop fell back to
        // per-instruction interpretation (≈100x slower than native).
        let compiled = compile_source(
            r#"
function f(n::Int64)::Float64
    x = 1.0
    s = 0.0
    i = 0
    while i < n
        x = x + 1.0
        s = s + x / 3.0
        i = i + 1
    end
    s
end

f(10)
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(
            executable.has_typed_loop(),
            "float loop with `/` should be recognized as a typed loop"
        );
    }

    #[test]
    fn typed_float_loop_with_division_executes_issue_8183() {
        // Same loop, executed: x runs 2.0..=11.0, s = Σ x/3.0 = 65/3.
        let compiled = compile_source(
            r#"
function f(n::Int64)::Float64
    x = 1.0
    s = 0.0
    i = 0
    while i < n
        x = x + 1.0
        s = s + x / 3.0
        i = i + 1
    end
    s
end

f(10)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        assert!(vm.executable.has_typed_loop());
        let result = vm.run().expect("run");
        match result {
            Value::F64(v) => assert!((v - 65.0 / 3.0).abs() < 1e-9, "expected 65/3, got {v}"),
            other => panic!("expected F64, got {other:?}"),
        }
    }

    #[test]
    fn typed_loop_reject_reason_unsupported_instr_issue_8193() {
        // Issue #8193: the recognizer records *why* it declined a loop-header
        // candidate (env-surfaced via SJULIA_TYPED_LOOP_DEBUG) so native-fast-path
        // coverage can be measured. An instruction with no typed-loop op (here
        // `PushNothing`) is reported as `UnsupportedInstr` at the offending ip.
        let code = vec![Instr::PushNothing];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(block.is_none());
        assert!(matches!(reject, Some(TypedLoopReject::UnsupportedInstr(0))));
    }

    #[test]
    fn typed_loop_reject_reason_op_count_over_cap_issue_8193() {
        // A loop body longer than `MAX_TYPED_LOOP_OPS` is reported as
        // `OpCountOverCap` (checked before the per-instruction walk, so the op
        // payload is irrelevant here).
        let code = vec![Instr::AddF64; MAX_TYPED_LOOP_OPS + 2];
        let mut reject = None;
        let block =
            try_predecode_typed_loop_range(&code, &[], 0, MAX_TYPED_LOOP_OPS + 2, 0, &mut reject);
        assert!(block.is_none());
        assert!(matches!(reject, Some(TypedLoopReject::OpCountOverCap)));
    }

    #[test]
    fn typed_loop_reject_reason_no_exit_issue_8193() {
        // A balanced body whose only branch loops back to the header (no branch
        // leaves the loop) is reported as `NoExit`.
        let code = vec![Instr::Jump(0)];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(block.is_none());
        assert!(matches!(reject, Some(TypedLoopReject::NoExit)));
    }

    // Issue #10439: a `CallSpecializeI64Slots` site inside a typed loop is
    // inlined as `TypedLoopOp::CallSpecializeI64Function`, recorded in
    // `specialize_callees`.
    #[test]
    fn typed_loop_inlines_call_specialize_i64_slots_10439() {
        use subset_julia_vm_bytecode::CallSpecializeSlots;
        // header: call untyped helper on slot 0, compare == 1, branch out, loop back.
        let code = vec![
            Instr::CallSpecializeI64Slots(Box::new(CallSpecializeSlots {
                spec_func_index: 0,
                slots: vec![0],
            })),
            Instr::PushI64(1),
            Instr::JumpIfNeI64(4), // target == end_ip -> loop exit
            Instr::Jump(0),        // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject)
            .expect("loop with a CallSpecializeI64Slots site should be recognized");
        assert_eq!(block.specialize_callees, vec![(0usize, 1usize)]);
        assert!(block
            .ops
            .iter()
            .any(|op| matches!(op, TypedLoopOp::CallSpecializeI64Function(0, 1))));
    }

    // Issue #10439 (transactionality guard): a loop that inlines a bail-capable
    // untyped specialize call must NOT be a typed loop if it also performs the
    // out-of-buffer `RandF64` effect — a runtime bail re-runs the block
    // generically from the header and would double-apply the RNG advance.
    #[test]
    fn typed_loop_rejects_call_specialize_i64_with_rand_side_effect_10439() {
        use subset_julia_vm_bytecode::CallSpecializeSlots;
        let code = vec![
            Instr::RandF64,         // side effect: advances the RNG
            Instr::StoreSlotF64(1), // consume the rand result into a slot
            Instr::CallSpecializeI64Slots(Box::new(CallSpecializeSlots {
                spec_func_index: 0,
                slots: vec![0],
            })),
            Instr::PushI64(1),
            Instr::JumpIfNeI64(6), // target == end_ip -> loop exit
            Instr::Jump(0),        // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(
            block.is_none(),
            "a specialize-call loop that also advances the RNG must not become a typed loop"
        );
    }

    // Issue #10491: a `CallSpecializeF64Slots` site inside a typed loop is
    // inlined as `TypedLoopOp::CallSpecializeF64Function`, recorded in
    // `specialize_f64_callees` (the F64 mirror of the #10439 test above).
    #[test]
    fn typed_loop_inlines_call_specialize_f64_slots_10491() {
        use subset_julia_vm_bytecode::CallSpecializeSlots;
        let code = vec![
            Instr::CallSpecializeF64Slots(Box::new(CallSpecializeSlots {
                spec_func_index: 0,
                slots: vec![0],
            })),
            Instr::PushF64(1.5),
            Instr::JumpIfNotGtF64(4), // target == end_ip -> loop exit
            Instr::Jump(0),           // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject)
            .expect("loop with a CallSpecializeF64Slots site should be recognized");
        assert_eq!(block.specialize_f64_callees, vec![(0usize, 1usize)]);
        assert!(block
            .ops
            .iter()
            .any(|op| matches!(op, TypedLoopOp::CallSpecializeF64Function(0, 1))));
    }

    // Issue #10491: the F64 specialize site is loop-mode only — typed scalar
    // *function* blocks execute frame-lessly without `&mut self`, so they
    // cannot resolve a runtime specialization (same gate as the I64 arm).
    #[test]
    fn typed_function_mode_rejects_call_specialize_f64_slots_10491() {
        use subset_julia_vm_bytecode::CallSpecializeSlots;
        let code = vec![
            Instr::CallSpecializeF64Slots(Box::new(CallSpecializeSlots {
                spec_func_index: 0,
                slots: vec![0],
            })),
            Instr::ReturnF64,
        ];
        let block = try_predecode_typed_scalar_function(&code, &[], 0, code.len(), 0, &[0]);
        assert!(
            block.is_none(),
            "function-mode predecode must not inline a specialize site"
        );
    }

    // Issue #10491 (guard interaction): the generalized #10504 transactionality
    // guard covers the F64 specialize op — a loop that also draws RNG values
    // stays fully generic.
    #[test]
    fn typed_loop_rejects_call_specialize_f64_with_rand_side_effect_10491() {
        use subset_julia_vm_bytecode::CallSpecializeSlots;
        let code = vec![
            Instr::RandF64,
            Instr::StoreSlotF64(1),
            Instr::CallSpecializeF64Slots(Box::new(CallSpecializeSlots {
                spec_func_index: 0,
                slots: vec![0],
            })),
            Instr::PushF64(1.5),
            Instr::JumpIfNotGtF64(6), // target == end_ip -> loop exit
            Instr::Jump(0),           // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(
            block.is_none(),
            "an F64 specialize-call loop that also advances the RNG must not become a typed loop"
        );
    }

    // Issue #10477 (scope item 4 audit): the general compound `while c1 && c2`
    // guard — an I64 counter condition AND an F64 threshold condition — is
    // recognized by the general typed-loop recognizer and executes natively
    // with the exact upstream result. Pinned so recognizer changes cannot
    // silently drop the shape.
    #[test]
    fn typed_loop_recognizes_compound_while_condition_10477() {
        let compiled = compile_source(
            r#"
function walk(n::Int64)::Float64
    x = 0.0
    i = 0
    while i < n && x < 1.0e6
        x = x + 1.5
        i = i + 1
    end
    x
end

walk(500000)
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(
            executable.has_typed_loop(),
            "compound `while c1 && c2` loop must be recognized as a typed loop"
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        match result {
            Value::F64(v) => assert_eq!(v, 750000.0),
            other => panic!("expected F64, got {other:?}"),
        }
    }

    // Issue #10516: the block-entry inliner splices a small I64 callee body
    // into the caller's op stream — Uninit ops for non-param locals, reverse
    // param stores, translated body with Return -> Jump(end), and caller jump
    // targets remapped across the splice.
    #[test]
    fn typed_loop_inline_splices_small_i64_callee_10516() {
        // Callee: gcd-step-like body over params a(slot0)/b(slot1), local t(slot2):
        //   return (a % b) + t-shaped shape is unnecessary; use: t = a % b; return t
        let callee = I64FunctionBlock {
            slots: vec![
                ScalarFunctionSlot {
                    slot: 0,
                    param_index: Some(0),
                },
                ScalarFunctionSlot {
                    slot: 1,
                    param_index: Some(1),
                },
                ScalarFunctionSlot {
                    slot: 2,
                    param_index: None,
                },
            ],
            ops: vec![
                ScalarFunctionOp::LoadSlot(0),
                ScalarFunctionOp::LoadRemSlot(1),
                ScalarFunctionOp::StoreSlot(2),
                ScalarFunctionOp::LoadSlot(2),
                ScalarFunctionOp::Return,
            ],
            callees: vec![],
        };
        // Caller loop: load 2 args, call, compare-and-exit, back-edge.
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::LoadI64Slot(1),
            TypedLoopOp::CallSpecializeI64Function(0, 2),
            TypedLoopOp::PushI64(1),
            TypedLoopOp::JumpIfI64(I64Relation::Ne, TypedLoopTarget::Exit),
            TypedLoopOp::Jump(TypedLoopTarget::LoopBack),
        ];
        let inlined = try_inline_i64_callees_into_typed_ops(&ops, &[], &[callee], 2)
            .expect("small callee must inline");
        // Splice: Uninit(t=4) + StoreSlot(b=3), StoreSlot(a=2) + 5 body ops
        // (Return -> Jump). Total = 6 - 1 + 1 + 2 + 5 = 13.
        assert_eq!(inlined.len(), 13);
        assert!(matches!(inlined[2], TypedLoopOp::UninitI64Slot(4)));
        assert!(matches!(inlined[3], TypedLoopOp::StoreI64Slot(3))); // arg b
        assert!(matches!(inlined[4], TypedLoopOp::StoreI64Slot(2))); // arg a
        assert!(matches!(inlined[5], TypedLoopOp::LoadI64Slot(2)));
        assert!(matches!(inlined[6], TypedLoopOp::LoadModI64Slot(3)));
        assert!(matches!(inlined[7], TypedLoopOp::StoreI64Slot(4)));
        assert!(matches!(inlined[8], TypedLoopOp::LoadI64Slot(4)));
        // Return -> jump to the op after the splice (index 10 = PushI64(1)).
        assert!(
            matches!(inlined[9], TypedLoopOp::Jump(TypedLoopTarget::Op(10))),
            "return must jump past the splice, got {:?}",
            inlined[9]
        );
        assert!(matches!(inlined[10], TypedLoopOp::PushI64(1)));
        assert!(matches!(
            inlined[11],
            TypedLoopOp::JumpIfI64(I64Relation::Ne, TypedLoopTarget::Exit)
        ));
        assert!(matches!(
            inlined[12],
            TypedLoopOp::Jump(TypedLoopTarget::LoopBack)
        ));
    }

    // Issue #10516: ineligible callees keep the call op / return None.
    #[test]
    fn typed_loop_inline_skips_ineligible_callees_10516() {
        let big_callee = I64FunctionBlock {
            slots: vec![ScalarFunctionSlot {
                slot: 0,
                param_index: Some(0),
            }],
            ops: vec![ScalarFunctionOp::Push(1); INLINE_MAX_CALLEE_OPS + 1],
            callees: vec![],
        };
        let nested_callee = I64FunctionBlock {
            slots: vec![ScalarFunctionSlot {
                slot: 0,
                param_index: Some(0),
            }],
            ops: vec![ScalarFunctionOp::Call(0, 1), ScalarFunctionOp::Return],
            callees: vec![I64FunctionBlock {
                slots: vec![],
                ops: vec![],
                callees: vec![],
            }],
        };
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::CallSpecializeI64Function(0, 1),
            TypedLoopOp::PushI64(1),
            TypedLoopOp::JumpIfI64(I64Relation::Ne, TypedLoopTarget::Exit),
            TypedLoopOp::Jump(TypedLoopTarget::LoopBack),
        ];
        assert!(
            try_inline_i64_callees_into_typed_ops(&ops, &[], &[big_callee], 1).is_none(),
            "an over-cap callee must not inline"
        );
        assert!(
            try_inline_i64_callees_into_typed_ops(&ops, &[], &[nested_callee], 1).is_none(),
            "a callee with nested calls must not inline"
        );
        // No call sites at all -> None without allocation.
        let no_sites = vec![TypedLoopOp::Jump(TypedLoopTarget::Exit)];
        assert!(try_inline_i64_callees_into_typed_ops(&no_sites, &[], &[], 0).is_none());
    }

    // Issue #10516: a site whose simulated i64 depth exceeds the argument
    // count (a pending operand below the args) is left un-inlined.
    #[test]
    fn typed_loop_inline_requires_exact_arg_depth_10516() {
        let callee = I64FunctionBlock {
            slots: vec![ScalarFunctionSlot {
                slot: 0,
                param_index: Some(0),
            }],
            ops: vec![ScalarFunctionOp::LoadSlot(0), ScalarFunctionOp::Return],
            callees: vec![],
        };
        // PushI64(7) leaves a pending operand beneath the argument.
        let ops = vec![
            TypedLoopOp::PushI64(7),
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::CallSpecializeI64Function(0, 1),
            TypedLoopOp::AddI64,
            TypedLoopOp::StoreI64Slot(1),
            TypedLoopOp::Jump(TypedLoopTarget::Exit),
        ];
        assert!(
            try_inline_i64_callees_into_typed_ops(&ops, &[], &[callee], 2).is_none(),
            "depth-mismatched sites must not inline"
        );
    }

    // Issue #10516 end-to-end: the coprime-pi kernel shape (untyped mygcd
    // reached through a fused specialize site inside a typed loop) produces
    // the exact upstream result with the inliner engaged.
    #[test]
    fn typed_loop_inline_i64_callee_end_to_end_10516() {
        let compiled = compile_source(
            r#"
function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function count_coprime(N::Int64)::Int64
    cnt = 0
    a = 1
    while a <= N
        b = 1
        while b <= N
            if mygcd(a, b) == 1
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

count_coprime(100)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        match result {
            // Upstream julia: sum(gcd(a,b)==1 for a in 1:100, b in 1:100) == 6087
            Value::I64(v) => assert_eq!(v, 6087, "count_coprime(100) must match upstream"),
            other => panic!("expected I64, got {other:?}"),
        }
    }

    // Issue #10491 end-to-end: an untyped loop-bodied F64 helper reached from a
    // typed nested loop through the fused `CallSpecializeF64Slots` site runs
    // natively and matches upstream Julia (expected value verified against
    // julia 1.12: scan(50) == 2491). The helper's specialized body carries an
    // I64 loop counter, so it resolves through the mixed-type
    // `TypedScalarFunctionBlock` path.
    #[test]
    fn typed_loop_call_specialize_f64_executes_natively_10491() {
        let compiled = compile_source(
            r#"
function fstep(x, y)
    r = x
    k = 0
    while k < 4
        r = r + y
        r = r * 0.5
        k = k + 1
    end
    r
end

function scan(N::Int64)::Int64
    cnt = 0
    x = 0.0
    a = 1
    while a <= N
        x = x + 1.0
        y = 0.0
        b = 1
        while b <= N
            y = y + 1.0
            if fstep(x, y) > 1.5
                cnt = cnt + 1
            end
            b = b + 1
        end
        a = a + 1
    end
    cnt
end

scan(50)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        match result {
            Value::I64(v) => assert_eq!(v, 2491, "scan(50) must match upstream Julia"),
            other => panic!("expected I64, got {other:?}"),
        }
        assert!(
            !vm.specialization_f64_cache.is_empty(),
            "the all-F64 specialize dispatch cache must be populated"
        );
    }

    // Issue #10504: the transactionality guard is general, not specialize-only.
    // ANY op that can bail for data-dependent reasons (`Mod*` on `x % 0` /
    // `typemin % -1`, `IndexLoad*`, the frame-less calls) combined with ANY
    // out-of-buffer side-effecting op (`RandF64` currently) must reject the
    // block: a bail re-runs the whole loop generically from the header and
    // would double-apply the side effect.
    #[test]
    fn typed_loop_rejects_mod_i64_with_rand_side_effect_10504() {
        let code = vec![
            Instr::RandF64,         // side effect: advances the RNG
            Instr::StoreSlotF64(1), // consume the rand result into a slot
            Instr::LoadSlotI64(2),
            Instr::PushI64(3),
            Instr::ModI64, // bail-capable: `x % 0` / `typemin % -1`
            Instr::StoreSlotI64(4),
            Instr::LoadSlotI64(2),
            Instr::PushI64(0),
            Instr::JumpIfNeI64(10), // target == end_ip -> loop exit
            Instr::Jump(0),         // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(
            block.is_none(),
            "a loop mixing ModI64 (bail-capable) with RandF64 (side effect) must not become a typed loop"
        );
    }

    // Issue #10504 (no over-rejection): the same shapes WITHOUT the conflicting
    // op stay recognized — `ModI64` alone (gcd/LCG loops) and `RandF64` alone
    // (Monte-Carlo loops) both keep the native path.
    #[test]
    fn typed_loop_accepts_mod_i64_without_side_effect_10504() {
        let code = vec![
            Instr::LoadSlotI64(2),
            Instr::PushI64(3),
            Instr::ModI64,
            Instr::StoreSlotI64(4),
            Instr::LoadSlotI64(2),
            Instr::PushI64(0),
            Instr::JumpIfNeI64(8), // target == end_ip -> loop exit
            Instr::Jump(0),        // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(
            block.is_some(),
            "a ModI64 loop with no in-place side effect must stay a typed loop"
        );
    }

    #[test]
    fn typed_loop_accepts_rand_without_bail_capable_op_10504() {
        let code = vec![
            Instr::RandF64,
            Instr::StoreSlotF64(1),
            Instr::LoadSlotI64(2),
            Instr::PushI64(0),
            Instr::JumpIfNeI64(6), // target == end_ip -> loop exit
            Instr::Jump(0),        // back-edge
        ];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, &[], 0, code.len(), 0, &mut reject);
        assert!(
            block.is_some(),
            "a RandF64 loop with no bail-capable op must stay a typed loop"
        );
    }

    // Issue #10814: `effects()` is one wildcard-free match over `TypedLoopOp`.
    // A new variant therefore fails to compile until both facts are decided;
    // this test pins every non-default classification and representative total
    // ops so edits to existing arms cannot silently weaken the guard.
    #[test]
    fn typed_loop_effect_classification_is_exhaustive_10814() {
        let assert_effects = |op: TypedLoopOp, bail_capable: bool, out_of_buffer_effect: bool| {
            assert_eq!(
                op.effects(),
                TypedLoopOpEffects {
                    bail_capable,
                    out_of_buffer_effect,
                },
                "unexpected effect classification for {op:?}"
            );
        };

        assert_effects(TypedLoopOp::RandF64, false, true);
        for op in [
            TypedLoopOp::ModI64,
            TypedLoopOp::LoadModI64Slot(0),
            TypedLoopOp::IndexLoadF64,
            TypedLoopOp::IndexLoadI64,
            TypedLoopOp::IndexStoreF64,
            TypedLoopOp::IndexStoreI64,
            TypedLoopOp::CallI64Function(0, 1),
            TypedLoopOp::CallF64Function(0, 1),
            TypedLoopOp::CallSpecializeI64Function(0, 1),
            TypedLoopOp::CallSpecializeF64Function(0, 1),
            TypedLoopOp::CallTypedI64Function(0, 1),
            TypedLoopOp::CallTypedF64Function(0, 1),
            TypedLoopOp::CallSpecializeComplexI64Function(0),
            TypedLoopOp::ConcatStr(2),
        ] {
            assert_effects(op, true, false);
        }
        for op in [
            TypedLoopOp::AddI64,
            TypedLoopOp::AddF64,
            TypedLoopOp::StoreI64Slot(0),
            TypedLoopOp::StoreF64Slot(0),
        ] {
            assert_effects(op, false, false);
        }
    }

    // Issue #10504 end-to-end: `typemin % -1` makes `ModI64` bail mid-iteration
    // (upstream Julia yields 0 there, so the generic re-run completes with NO
    // error — the divergence is silent). Before the generalized guard this loop
    // was recognized as a typed loop, so the bail re-ran the WHOLE loop
    // generically from the header AFTER the typed run had already advanced the
    // RNG: the result was built from the 6th..10th draws instead of the
    // 1st..5th. The guard keeps rand-plus-bail-capable loops fully generic.
    #[test]
    fn typed_loop_mod_bail_after_rand_draw_stays_generic_10504() {
        let compiled = compile_source(
            r#"
function f(n::Int64)::Float64
    s = 0.0
    m = -9223372036854775807 - 1
    i = 1
    while i <= n
        x = rand()
        r = m % (i - 6)
        s = s + x + r
        i = i + 1
    end
    s
end

f(5)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        // Ground truth: exactly one draw per iteration, `typemin % -1 == 0`
        // (Julia semantics), accumulated in the bytecode's rounding order
        // `(s + x) + Float64(r)`.
        let mut rng = StableRng::new(0);
        let m = i64::MIN;
        let mut s = 0.0;
        for i in 1..=5_i64 {
            let x = rng.next_f64();
            let d = i - 6;
            let r = if d == -1 { 0 } else { m % d };
            s = (s + x) + r as f64;
        }
        match result {
            Value::F64(v) => {
                assert_eq!(v, s, "loop must consume exactly one rand() per iteration")
            }
            other => panic!("expected F64, got {other:?}"),
        }
    }

    #[test]
    fn typed_loop_accept_leaves_reject_reason_unset_issue_8193() {
        // A *recognized* loop must not populate a reject reason. Scan only the
        // user function `f`'s code range (not all of Base) for its back-edge and
        // confirm the successful predecode leaves `reject` untouched.
        let compiled = compile_source(
            r#"
function f(n::Int64)::Float64
    x = 1.0
    s = 0.0
    i = 0
    while i < n
        x = x + 1.0
        s = s + x / 3.0
        i = i + 1
    end
    s
end

f(10)
"#,
        );
        let f = compiled
            .functions
            .iter()
            .find(|info| info.name == "f")
            .expect("function f");
        let code = &compiled.code;
        let mut accepted = false;
        for header in f.code_start..f.code_end {
            for jump_ip in header + 1..f.code_end {
                if matches!(code.get(jump_ip), Some(Instr::Jump(t)) if *t == header) {
                    let mut reject = None;
                    if try_predecode_typed_loop_range(
                        code,
                        &compiled.functions,
                        header,
                        jump_ip + 1,
                        compiled.base_function_count,
                        &mut reject,
                    )
                    .is_some()
                    {
                        assert!(
                            reject.is_none(),
                            "an accepted typed loop must not set a reject reason"
                        );
                        accepted = true;
                    }
                }
            }
        }
        assert!(accepted, "the float `/` loop in `f` should be accepted");
    }

    #[test]
    fn predecodes_typed_loop_with_integer_modulo_issue_8183() {
        // Issue #8183: an LCG-style loop with integer `%` (ModI64), a fused
        // `LoadMulI64Slot`, and a mixed `Int64 / Float64` division must be
        // recognized as a native typed loop. These ops were all missing from the
        // typed-loop IR (and the body exceeds the old 64-op scan window).
        let compiled = compile_source(
            r#"
function f(n::Int64)::Float64
    seed = 1
    s = 0.0
    i = 0
    while i < n
        seed = (1103515245 * seed + 12345) % 2147483648
        s = s + seed / 2147483648.0
        i = i + 1
    end
    s
end

f(10)
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(
            executable.has_typed_loop(),
            "loop with `%`, fused I64 load, and mixed `/` should be a typed loop"
        );
    }

    #[test]
    fn predecodes_large_typed_float_body_over_64_ops_issue_8183() {
        // Issue #8183: a dense Float64 ODE step (Aizawa attractor) compiles to a
        // ~68-op loop body that exceeded the old `MAX_TYPED_LOOP_OPS` (64) scan
        // window and used `DivF64`. It must now be recognized as a typed loop.
        let compiled = compile_source(
            r#"
function aizawa(n::Int64)::Float64
    a = 0.95; b = 0.7; c = 0.6; d = 3.5; e = 0.25; g = 0.1
    dt = 0.01
    x = 0.1; y = 0.0; z = 0.0
    s = 0.0
    i = 0
    while i < n
        dx = (z - b) * x - d * y
        dy = d * x + (z - b) * y
        dz = c + a * z - z * z * z / 3.0 - (x * x + y * y) * (1.0 + e * z) + g * z * x * x * x
        x = x + dx * dt
        y = y + dy * dt
        z = z + dz * dt
        s = s + x
        i = i + 1
    end
    s
end

aizawa(10)
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(
            executable.has_typed_loop(),
            "dense >64-op Float64 ODE body should be recognized as a typed loop"
        );
    }

    #[test]
    fn typed_float_loop_block_executes_mandel_inside_point() {
        let compiled = compile_source(
            r#"
function mandel_point(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    iter = 0
    while zr * zr + zi * zi <= 4.0 && iter < maxiter
        zr2 = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = zr2
        iter = iter + 1
    end
    iter
end

mandel_point(0.0, 0.0, 10)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        assert!(vm.executable.has_typed_loop());
        let result = vm.run().expect("run");
        assert!(matches!(result, Value::I64(10)));
    }

    #[test]
    fn typed_float_loop_block_executes_mandel_escaping_point() {
        let compiled = compile_source(
            r#"
function mandel_point(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    iter = 0
    while zr * zr + zi * zi <= 4.0 && iter < maxiter
        zr2 = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = zr2
        iter = iter + 1
    end
    iter
end

mandel_point(2.0, 2.0, 10)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        assert!(vm.executable.has_typed_loop());
        let result = vm.run().expect("run");
        assert!(matches!(result, Value::I64(1)));
    }

    #[test]
    fn typed_loop_block_executes_counted_for_loop_with_rand() {
        let compiled = compile_source(
            r#"
function random_count(n::Int64)::Int64
    inside = 0
    for _ in 1:n
        x = rand()
        y = rand()
        if x * x + y * y <= 1.0
            inside += 1
        end
    end
    inside
end

random_count(10)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(42));
        assert!(vm.executable.has_typed_loop());
        let result = vm.run().expect("run");
        assert!(matches!(result, Value::I64(8)));
    }

    #[test]
    fn typed_loop_block_executes_runtime_specialized_estimate_pi_shape() {
        let compiled = compile_source(
            r#"
function estimate_pi(n)
    inside = 0
    for _ in 1:n
        x, y = rand(), rand()
        if x^2 + y^2 <= 1
            inside += 1
        end
    end
    return 4 * inside / n
end

estimate_pi(10)
"#,
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(42));
        let result = vm.run().expect("run");
        assert!(vm.executable.has_typed_loop());
        assert!(matches!(result, Value::F64(value) if (value - 3.2).abs() < 1.0e-12));
    }

    #[test]
    fn complex_mandelbrot_escape_runtime_specialization_uses_typed_loop_6253() {
        let compiled = compile_source(
            r#"
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

f = mandelbrot_escape
f(0.0 + 0.0im, 10) + f(1.0 + 1.0im, 10) * 100
"#,
        );
        let initial_code_len = compiled.code.len();
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        assert!(
            vm.executable.has_typed_loop(),
            "appended bytecode: {:?}",
            vm.code
                .iter()
                .enumerate()
                .skip(initial_code_len)
                .collect::<Vec<_>>()
        );
        assert!(matches!(result, Value::I64(310)));
    }

    #[test]
    fn complex_mandelbrot_escape_k_minus_one_uses_typed_loop_8796() {
        let compiled = compile_source(
            r#"
function mandelbrot_escape(c::Complex, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

f = mandelbrot_escape
f(0.0 + 0.0im, 10) + f(1.0 + 1.0im, 10) * 100
"#,
        );
        let initial_code_len = compiled.code.len();
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        assert!(
            vm.executable.has_typed_loop(),
            "appended bytecode: {:?}",
            vm.code
                .iter()
                .enumerate()
                .skip(initial_code_len)
                .collect::<Vec<_>>()
        );
        assert!(matches!(result, Value::I64(210)));
    }

    #[test]
    fn fuse_typed_loop_ops_fuses_windows_and_remaps_targets_issue_9654() {
        use TypedLoopOp as Op;
        let ops = vec![
            Op::LoadF64Slot(0),                                        // 0 ┐ fuse
            Op::LoadMulF64Slot(1),                                     // 1 ┘
            Op::PushF64(4.0),                                          // 2 ┐ fuse
            Op::JumpIfNotF64(F64Relation::Gt, TypedLoopTarget::Op(5)), // 3 ┘
            Op::Jump(TypedLoopTarget::LoopBack),                       // 4
            Op::LoadF64Slot(2),                                        // 5 ┐ fuse (target
            Op::StoreF64Slot(3),                                       // 6 ┘  lands on 5)
        ];
        let fused = fuse_typed_loop_ops(ops);
        assert_eq!(fused.len(), 4, "expected 3 fusions: {fused:?}");
        assert!(matches!(fused[0], Op::PushMulF64Slots(0, 1)));
        assert!(matches!(
            fused[1],
            Op::JumpIfNotF64Const(F64Relation::Gt, c, TypedLoopTarget::Op(3)) if c == 4.0
        ));
        assert!(matches!(fused[2], Op::Jump(TypedLoopTarget::LoopBack)));
        assert!(matches!(fused[3], Op::CopyF64Slots(3, 2)));
    }

    #[test]
    fn fuse_typed_loop_ops_keeps_windows_with_interior_jump_targets_issue_9654() {
        use TypedLoopOp as Op;
        // A jump lands on the Store — the Load/Store pair must NOT fuse (the
        // jump needs to observe the intermediate stack state).
        let ops = vec![
            Op::LoadF64Slot(0),               // 0
            Op::StoreF64Slot(1),              // 1 <- jump target
            Op::Jump(TypedLoopTarget::Op(1)), // 2
        ];
        let fused = fuse_typed_loop_ops(ops);
        assert_eq!(
            fused.len(),
            3,
            "window with interior target fused: {fused:?}"
        );
        assert!(matches!(fused[0], Op::LoadF64Slot(0)));
        assert!(matches!(fused[1], Op::StoreF64Slot(1)));
        assert!(matches!(fused[2], Op::Jump(TypedLoopTarget::Op(1))));
    }

    #[test]
    fn fuse_complex_mul_add_assign_fuses_mandelbrot_update_issue_10532() {
        use TypedLoopOp as Op;
        let ops = vec![
            Op::PushDiffSquaresF64Slots(2, 3), // 0
            Op::AddF64SlotStore(0, 4),         // 1
            Op::PushMulF64Slots(2, 3),         // 2
            Op::PushMulF64Slots(3, 2),         // 3
            Op::AddF64,                        // 4
            Op::AddF64SlotStore(1, 5),         // 5
            Op::CopyF64Slots(2, 4),            // 6
            Op::CopyF64Slots(3, 5),            // 7
        ];
        let fused = fuse_complex_mul_add_assign(ops);
        assert_eq!(fused.len(), 1, "expected full update fusion: {fused:?}");
        assert!(
            matches!(
                fused[0],
                Op::ComplexMulAddAssign {
                    z_re: 2,
                    z_im: 3,
                    c_re: 0,
                    c_im: 1,
                }
            ),
            "unexpected fused op: {:?}",
            fused[0]
        );
    }

    #[test]
    fn fuse_complex_mul_add_assign_preserves_interior_jump_targets_issue_10532() {
        use TypedLoopOp as Op;
        // A jump lands inside the update window — fusion must NOT swallow it.
        let ops = vec![
            Op::PushDiffSquaresF64Slots(2, 3), // 0
            Op::AddF64SlotStore(0, 4),         // 1
            Op::PushMulF64Slots(2, 3),         // 2 <- target
            Op::PushMulF64Slots(3, 2),         // 3
            Op::AddF64,                        // 4
            Op::AddF64SlotStore(1, 5),         // 5
            Op::CopyF64Slots(2, 4),            // 6
            Op::CopyF64Slots(3, 5),            // 7
            Op::Jump(TypedLoopTarget::Op(2)),  // 8
        ];
        let fused = fuse_complex_mul_add_assign(ops);
        assert_eq!(
            fused.len(),
            9,
            "window with interior target fused: {fused:?}"
        );
    }

    #[test]
    fn typed_scalar_function_block_predecodes_mandel_point_issue_9693() {
        // Issue #9693: the whole SROA'd mandel_point body — ComplexF64 param
        // decompose preamble, escape loop with early return, loop-exhausted
        // tail return — predecodes to a frame-less typed scalar function
        // block, with the params bound as (ComplexF64, I64).
        let compiled = compile_source(
            r#"
function mandel_point(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

mandel_point(0.3 + 0.5im, 10)
"#,
        );
        let f = compiled
            .functions
            .iter()
            .find(|info| info.name == "mandel_point")
            .expect("function mandel_point");
        let block = try_predecode_typed_scalar_function(
            &compiled.code,
            &compiled.functions,
            f.entry,
            f.code_end,
            compiled.base_function_count,
            &f.param_slots,
        )
        .expect("mandel_point should predecode to a typed scalar function block");
        assert_eq!(block.params.len(), 2, "params: {:?}", block.params);
        assert!(
            matches!(block.params[0], TypedFunctionParamBinding::ComplexF64(0)),
            "params: {:?}",
            block.params
        );
        assert!(
            matches!(block.params[1], TypedFunctionParamBinding::I64(_)),
            "params: {:?}",
            block.params
        );
        // The decompose preamble fused to direct param-field stores.
        assert!(
            block
                .ops
                .iter()
                .any(|op| matches!(op, TypedLoopOp::StoreComplexParamFieldF64(0, _, _))),
            "ops: {:?}",
            block.ops
        );
        // Executes end-to-end through the frame-less call path with the same
        // escape counts as the frame path.
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        assert!(matches!(result, Value::I64(10)), "got {result:?}");
    }

    #[test]
    fn typed_scalar_function_block_rejects_rand_bodies_pr9733_review() {
        // A body containing `rand()` must NOT become a function block: RandF64
        // advances the RNG, and a bail after it would let the frame-path
        // fallback observe a shifted random sequence (PR #9733 review).
        let code = vec![
            Instr::RandF64,
            Instr::StoreSlotF64(2),
            Instr::LoadSlotI64(0),
            Instr::ReturnI64,
        ];
        assert!(
            try_predecode_typed_scalar_function(&code, &[], 0, 4, 0, &[0, 1]).is_none(),
            "rand-containing body must not predecode to a function block"
        );
    }

    #[test]
    fn typed_scalar_function_block_rejects_untyped_live_in_local_issue_9693() {
        // A function reading a non-param local before writing it (via a
        // conditional first write) must NOT become a function block — the
        // frame path raises UndefVarError there.
        let code = vec![
            Instr::LoadSlotF64(3), // live-in non-param slot
            Instr::ReturnF64,
        ];
        assert!(
            try_predecode_typed_scalar_function(&code, &[], 0, 2, 0, &[0, 1]).is_none(),
            "live-in non-param local must bail"
        );
    }

    #[test]
    fn typed_loop_early_return_real_decomposed_escape_issue_9654() {
        // Issue #9654: a counted loop whose escape path is an early `return`
        // (`LoadAddConstI64Slot(k, -1); ReturnI64`) must be recognized as a
        // native typed loop. Before, `ReturnI64` had no typed-loop op, so every
        // escape-style kernel (Mandelbrot form) fell back to per-instruction
        // interpretation (~5x slower).
        let compiled = compile_source(
            r#"
function esc_count(cr::Float64, ci::Float64, maxiter::Int64)::Int64
    zr = 0.0
    zi = 0.0
    for k in 1:maxiter
        if zr * zr + zi * zi > 4.0
            return k - 1
        end
        t = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = t
    end
    return maxiter
end

esc_count(2.0, 2.0, 10) + esc_count(0.0, 0.0, 10) * 100
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(
            executable.has_typed_loop(),
            "escape loop with early return should be recognized as a typed loop"
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        // esc_count(2,2,10): escapes at k=2 -> returns 1; esc_count(0,0,10) -> 10.
        assert!(matches!(result, Value::I64(1001)), "got {result:?}");
    }

    #[test]
    fn typed_loop_early_return_sroa_complex_mandelbrot_issue_9654() {
        // Issue #9654: the slot-pair SROA form (#9198 S2) of the ComplexF64
        // Mandelbrot escape loop — pure F64 slot ops + early return — must stay
        // on the native typed-loop path (the boxed-shape mandelbrot recognizer
        // no longer matches SROA'd bytecode, by design).
        let compiled = compile_source(
            r#"
function mandel_point(c::ComplexF64, maxiter::Int64)::Int64
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0
            return k - 1
        end
        z = z * z + c
    end
    return maxiter
end

mandel_point(0.0 + 0.0im, 10) + mandel_point(2.0 + 2.0im, 10) * 100
"#,
        );
        let executable = ExecutableProgram::from_bytecode(
            &compiled.code,
            &compiled.functions,
            compiled.base_function_count,
        );
        assert!(
            executable.has_typed_loop(),
            "SROA'd ComplexF64 escape loop should be recognized as a typed loop"
        );
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        // interior point -> 10; (2+2i) escapes at k=2 -> 1.
        assert!(matches!(result, Value::I64(110)), "got {result:?}");
    }

    #[test]
    fn index_assign_runtime_specialization_adds_typed_loop_6346() {
        let compiled = compile_source(
            r#"
function fill_index_assign_6346!(a, n)
    for i in 1:n
        a[i] = i * 3
    end
    return a[n]
end

arr = Vector{Int64}(undef, 5)
fill_index_assign_6346!(arr, 5)
"#,
        );
        let initial_code_len = compiled.code.len();
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        assert!(
            vm.executable.has_typed_loop(),
            "appended bytecode: {:?}",
            vm.code
                .iter()
                .enumerate()
                .skip(initial_code_len)
                .collect::<Vec<_>>()
        );
        assert!(matches!(result, Value::I64(15)));
    }

    #[test]
    fn untyped_mixed_division_loop_runtime_specializes_to_typed_loop_issue_8183() {
        // Issue #8183: an *untyped* LCG loop with a mixed `Int64 / Float64`
        // division must, after runtime specialization on `n::Int64`, be
        // recognized as a native typed loop. The specializer promoted the I64
        // operand of the mixed division with `Swap; ToF64; Swap`, and the stray
        // `Swap` aborted typed-loop recognition (Aizawa's all-Float64 body, with
        // no mixed op, specialized fine — only mixed-int/float code regressed).
        let compiled = compile_source(
            r#"
function ifs_like(n)
    seed = 1
    s = 0.0
    i = 0
    while i < n
        seed = (1103515245 * seed + 12345) % 2147483648
        r = seed / 2147483648.0
        s = s + r
        i = i + 1
    end
    s
end

ifs_like(100)
"#,
        );
        let initial_code_len = compiled.code.len();
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        let result = vm.run().expect("run");
        assert!(
            vm.executable.has_typed_loop(),
            "untyped mixed-division loop should runtime-specialize to a typed loop; \
             appended bytecode: {:?}",
            vm.code
                .iter()
                .enumerate()
                .skip(initial_code_len)
                .collect::<Vec<_>>()
        );
        // Independent reference: same LCG + mixed division in Rust.
        let mut seed: i64 = 1;
        let mut s = 0.0_f64;
        for _ in 0..100 {
            seed = (1103515245_i64.wrapping_mul(seed) + 12345) % 2147483648;
            s += seed as f64 / 2147483648.0;
        }
        match result {
            Value::F64(v) => assert!((v - s).abs() < 1e-9, "expected {s}, got {v}"),
            other => panic!("expected F64, got {other:?}"),
        }
    }

    /// Run `src`, let the VM runtime-specialize, and report `(recognized as a
    /// native typed loop, the runtime-specialized bytecode contains `Swap`)`.
    /// The specializer appends each recompiled body past the original program's
    /// `code` length, so everything from there on is specializer output.
    fn run_and_inspect_specialized(src: &str) -> (bool, bool) {
        let compiled = compile_source(src);
        let initial_code_len = compiled.code.len();
        let mut vm = Vm::new_program(compiled, StableRng::new(0));
        vm.run().expect("run");
        let recognized = vm.executable.has_typed_loop();
        let has_swap = vm
            .code
            .iter()
            .skip(initial_code_len)
            .any(|instr| matches!(instr, Instr::Swap));
        (recognized, has_swap)
    }

    #[test]
    fn untyped_scalar_hot_loops_specialize_to_swap_free_typed_loops_issue_8192() {
        // Issue #8192: prevention guard for the #8183 footgun across the binary
        // ops. Binary-op bytecode is generated by two independent paths — the
        // main compiler and the runtime arg-type specializer — and the
        // specializer must keep untyped Int64/Float64 scalar hot loops on the
        // native typed-loop fast path: recognized AND free of the on-stack `Swap`
        // that aborts recognition. Each case routes a different op through the
        // specializer (untyped params force runtime specialization). A regression
        // in the specializer's typed-instruction / promotion selection that
        // reintroduces a `Swap` (or any unrecognized instruction) into the hot
        // body fails here even though the result stays numerically correct.
        let cases: [(&str, &str); 6] = [
            (
                "mixed Float64 + Int64",
                r#"
function f(n)
    s = 0.0
    i = 0
    while i < n
        s = s + i
        i = i + 1
    end
    s
end
f(64)
"#,
            ),
            (
                "mixed Float64 - Int64",
                r#"
function f(n)
    s = 0.0
    i = 0
    while i < n
        s = s - i
        i = i + 1
    end
    s
end
f(64)
"#,
            ),
            (
                "mixed Float64 * Int64",
                r#"
function f(n)
    s = 0.0
    x = 1.5
    i = 0
    while i < n
        s = s + x * i
        i = i + 1
    end
    s
end
f(64)
"#,
            ),
            (
                "mixed Int64 / Float64",
                r#"
function f(n)
    s = 0.0
    i = 0
    while i < n
        s = s + i / 2.0
        i = i + 1
    end
    s
end
f(64)
"#,
            ),
            (
                "Int64 / Int64 (forces Float64)",
                r#"
function f(n)
    s = 0.0
    i = 0
    while i < n
        s = s + i / 3
        i = i + 1
    end
    s
end
f(64)
"#,
            ),
            (
                "pure Float64",
                r#"
function f(n)
    s = 0.0
    x = 1.0
    i = 0
    while i < n
        x = x + 1.0
        s = s + x
        i = i + 1
    end
    s
end
f(64)
"#,
            ),
        ];
        for (label, src) in cases {
            let (recognized, has_swap) = run_and_inspect_specialized(src);
            assert!(
                recognized,
                "{label}: untyped scalar hot loop should runtime-specialize to a native typed loop"
            );
            assert!(
                !has_swap,
                "{label}: runtime-specialized hot loop must be Swap-free (a stray Swap aborts \
                 typed-loop recognition — the #8183 footgun)"
            );
        }
    }

    #[test]
    fn shared_binary_table_only_emits_typed_loop_recognized_instrs_issue_8192() {
        // Issue #8192: the shared `typed_scalar_binary_instr` table is the single
        // source of truth feeding BOTH binary-op codegen paths. Every typed
        // instruction it can emit must be accepted by the typed-loop body
        // recognizer (`try_predecode_typed_loop_range` above); otherwise a
        // specialized hot loop emitting it silently drops off the native fast
        // path. `typed_loop_recognizes` mirrors that recognizer's scalar
        // binary / coercion arms: if you add a typed binary instruction to the
        // shared table, you MUST also teach the recognizer and extend this
        // oracle — this test (a unit-level tripwire) and the end-to-end
        // `untyped_scalar_hot_loops_…` guard above fail until you do.
        use crate::ir::core::BinaryOp;
        use subset_julia_vm_bytecode::typed_scalar_binary_instr;

        fn typed_loop_recognizes(instr: &Instr) -> bool {
            matches!(
                instr,
                // Coercion the specializer relies on for mixed Int/Float promotion.
                Instr::ToF64
                    // Integer arithmetic / modulo.
                    | Instr::AddI64 | Instr::SubI64 | Instr::MulI64 | Instr::ModI64
                    // Float arithmetic / division.
                    | Instr::AddF64 | Instr::SubF64 | Instr::MulF64 | Instr::DivF64
                    // Integer comparisons.
                    | Instr::EqI64 | Instr::NeI64 | Instr::LtI64
                    | Instr::GtI64 | Instr::LeI64 | Instr::GeI64
                    // Float comparisons.
                    | Instr::EqF64 | Instr::NeF64 | Instr::LtF64
                    | Instr::GtF64 | Instr::LeF64 | Instr::GeF64
            )
        }

        let ops = [
            BinaryOp::Add,
            BinaryOp::Sub,
            BinaryOp::Mul,
            BinaryOp::Div,
            BinaryOp::Mod,
            BinaryOp::Eq,
            BinaryOp::Ne,
            BinaryOp::Lt,
            BinaryOp::Le,
            BinaryOp::Gt,
            BinaryOp::Ge,
            BinaryOp::Pow,
            BinaryOp::IntDiv,
            BinaryOp::And,
            BinaryOp::Or,
        ];
        for op in ops {
            for is_float in [false, true] {
                if let Some(instr) = typed_scalar_binary_instr(op, is_float) {
                    assert!(
                        typed_loop_recognizes(&instr),
                        "typed_scalar_binary_instr({op:?}, {is_float}) = {instr:?} is not accepted \
                         by the typed-loop recognizer — teach vm::executable's recognizer (and this \
                         oracle), or specialized hot loops using it will silently de-optimize"
                    );
                }
            }
        }

        // Tripwires pinning the coupling that bit #8183: the recognizer accepts
        // the `ToF64` coercion the specializer emits for mixed Int/Float
        // promotion, but rejects the on-stack `Swap` — which is exactly why the
        // specializer must coerce operands as it compiles them rather than after
        // both are pushed.
        assert!(typed_loop_recognizes(&Instr::ToF64));
        assert!(!typed_loop_recognizes(&Instr::Swap));
    }

    // ---- Issue #10565: `certify_typed_ops_trusted` ----
    //
    // The trusted executor is memory-safe ONLY if this pass is sound, so the
    // negative cases matter more than the positive ones: each one is a stream
    // that, if certified, would let `run_typed_ops_core::<true>` index a
    // fixed-capacity stack out of bounds.

    #[test]
    fn certify_typed_ops_trusted_accepts_simple_i64_loop_body() {
        let ops = vec![
            TypedLoopOp::LoadAddConstI64Slot(0, 1),
            TypedLoopOp::StoreI64Slot(0),
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::LoadI64Slot(1),
            TypedLoopOp::JumpIfI64(I64Relation::Lt, TypedLoopTarget::LoopBack),
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_f64_and_bool_stack_ops() {
        // f64 push/pop2 plus a bool push (CmpF64) and bool pop (JumpIfZero).
        let ops = vec![
            TypedLoopOp::PushF64(1.0),
            TypedLoopOp::PushF64(2.0),
            TypedLoopOp::CmpF64(F64Relation::Lt),
            TypedLoopOp::JumpIfZero(TypedLoopTarget::Exit),
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_complex_mini_stack_ops() {
        let ops = vec![
            TypedLoopOp::PushComplexParam(0),
            TypedLoopOp::ComplexFieldF64(0),
            TypedLoopOp::StoreF64Slot(0),
            TypedLoopOp::PushComplexParam(0),
            TypedLoopOp::ComplexFieldF64(1),
            TypedLoopOp::ReturnF64,
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_typed_call_ops_10542() {
        // The #10542 ops must be modelled, not silently treated as no-effect:
        // each pops `argc` operands and pushes one result.
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::LoadI64Slot(1),
            TypedLoopOp::CallTypedI64Function(0, 2),
            TypedLoopOp::StoreI64Slot(2),
            TypedLoopOp::LoadF64Slot(0),
            TypedLoopOp::CallTypedF64Function(0, 1),
            TypedLoopOp::ReturnF64,
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_typed_call_with_too_few_operands_10542() {
        // `CallTypedI64Function(_, 2)` with only one operand pushed: the
        // executor would pop an empty i64 stack.
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::CallTypedI64Function(0, 2),
            TypedLoopOp::ReturnI64,
        ];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_models_string_ops_10559() {
        // The #10559 String ops run on a `Vec<StrRef>` that keeps its own
        // checks in both modes, so TRUSTED elides nothing there. But `EqStr`
        // pushes onto the bool stack and `StrLen` onto the i64 stack — both
        // UNCHECKED under TRUSTED — so those pushes must be modelled. Here the
        // pushes are consumed, so the stream certifies.
        let ops = vec![
            TypedLoopOp::LoadStrSlot(0),
            TypedLoopOp::PushStrConst(0),
            TypedLoopOp::EqStr,
            TypedLoopOp::JumpIfZero(TypedLoopTarget::Exit),
            TypedLoopOp::LoadStrSlot(0),
            TypedLoopOp::StrLen,
            TypedLoopOp::ReturnI64,
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_counts_streq_bool_push_10559() {
        // If `EqStr`'s bool push were modelled as "no effect", this stream
        // would certify while overflowing the (unchecked) bool stack at
        // runtime. Pushing more than `TYPED_LOOP_STACK_CAP` bool results
        // without consuming them must be REJECTED.
        let mut ops = Vec::new();
        for _ in 0..=TYPED_LOOP_STACK_CAP {
            ops.push(TypedLoopOp::LoadStrSlot(0));
            ops.push(TypedLoopOp::PushStrConst(0));
            ops.push(TypedLoopOp::EqStr);
        }
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_i64_stack_underflow() {
        let ops = vec![TypedLoopOp::AddI64, TypedLoopOp::ReturnI64];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_f64_stack_underflow() {
        let ops = vec![TypedLoopOp::NegF64, TypedLoopOp::ReturnF64];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_bool_stack_underflow() {
        let ops = vec![TypedLoopOp::JumpIfZero(TypedLoopTarget::Exit)];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_complex_stack_underflow() {
        let ops = vec![TypedLoopOp::ComplexFieldF64(0), TypedLoopOp::ReturnF64];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_i64_stack_over_cap() {
        let mut ops: Vec<TypedLoopOp> = (0..=TYPED_LOOP_STACK_CAP)
            .map(|i| TypedLoopOp::PushI64(i as i64))
            .collect();
        ops.push(TypedLoopOp::ReturnI64);
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_f64_stack_over_cap() {
        let mut ops: Vec<TypedLoopOp> = (0..=TYPED_LOOP_STACK_CAP)
            .map(|i| TypedLoopOp::PushF64(i as f64))
            .collect();
        ops.push(TypedLoopOp::ReturnF64);
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_complex_mini_stack_over_cap() {
        let mut ops: Vec<TypedLoopOp> = (0..=COMPLEX_MINI_STACK_CAP)
            .map(TypedLoopOp::PushComplexParam)
            .collect();
        ops.push(TypedLoopOp::ComplexFieldF64(0));
        ops.push(TypedLoopOp::ReturnF64);
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_over_long_op_stream() {
        // Longer than the fixed entry-depth array can model: decline, do not
        // index past it.
        let ops: Vec<TypedLoopOp> = (0..INLINE_MAX_RESULT_OPS + 1)
            .map(|_| TypedLoopOp::AddConstI64Slot(0, 1))
            .collect();
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_empty_ops() {
        assert!(certify_typed_ops_trusted(&[]));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_exit_with_nonempty_stack() {
        // `Exit` breaks out of `'loop_body`; anything still on a stack is
        // discarded (the stack pointers are function-local), exactly as in the
        // checked executor. So this is safe and must NOT block certification —
        // the rule is depth AGREEMENT at `Op(t)` targets, not "empty at every
        // branch".
        let ops = vec![
            TypedLoopOp::PushI64(1),
            TypedLoopOp::Jump(TypedLoopTarget::Exit),
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_inlined_callee_return_at_depth_one() {
        // The shape the #10516 inliner actually emits, and the reason an
        // "empty at every branch" rule is too strong: a spliced callee's
        // `Return` becomes a `Jump` past the body that LEAVES THE RETURN VALUE
        // on the i64 stack (the call op's stack contract). Its exit depth (1)
        // matches its target's entry depth (1), so it is sound and must
        // certify — under the stricter rule the coprime-pi kernel's hottest
        // block never reached the trusted executor at all (measured: 4999/4999
        // entries fell back to the checked path, i.e. the optimization was a
        // silent no-op).
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::Jump(TypedLoopTarget::Op(2)),
            TypedLoopOp::StoreI64Slot(1),
            TypedLoopOp::Jump(TypedLoopTarget::Exit),
        ];
        assert!(
            certify_typed_ops_trusted(&ops),
            "a jump whose exit depth matches its target's entry depth is sound \
             and must certify — this is the inlined-callee return shape"
        );
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_jump_into_nonzero_depth_point() {
        // THE soundness case. Every linear depth is in range and the branch
        // sits at depth 0, so an "empty at every branch" check would certify
        // this. But the back-edge re-enters op 1 with an EMPTY i64 stack, so
        // `AddI64` pops 2 off a stack holding 1 — in the trusted executor that
        // is `*sp -= 1` underflowing `usize` and a `get_unchecked` at a wild
        // index. Depth agreement rejects it: exit depth 0 != d[1] = 1.
        let ops = vec![
            TypedLoopOp::PushI64(1),
            TypedLoopOp::PushI64(2),
            TypedLoopOp::AddI64,
            TypedLoopOp::StoreI64Slot(0),
            TypedLoopOp::Jump(TypedLoopTarget::Op(1)),
        ];
        assert!(
            !certify_typed_ops_trusted(&ops),
            "a jump into a non-zero-depth point must NOT certify — the linear \
             walk does not model the depth control actually arrives with"
        );
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_forward_jump_over_a_push() {
        // The forward-jump form of the same hole, raised independently by the
        // codex adversarial review of this diff. Op 0 jumps straight to op 2,
        // skipping the `PushI64` the linear walk credits op 2's store with
        // popping.
        let ops = vec![
            TypedLoopOp::Jump(TypedLoopTarget::Op(2)),
            TypedLoopOp::PushI64(7),
            TypedLoopOp::StoreI64Slot(0),
            TypedLoopOp::Jump(TypedLoopTarget::Exit),
        ];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_rejects_out_of_range_jump_target() {
        // The executor bails on an out-of-range target in both modes, so this
        // is not itself unsafe — but the certifier declines anyway: "when in
        // doubt, do not certify".
        let ops = vec![TypedLoopOp::Jump(TypedLoopTarget::Op(99))];
        assert!(!certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_accepts_jump_to_zero_depth_point() {
        let ops = vec![
            TypedLoopOp::PushI64(1),
            TypedLoopOp::PushI64(2),
            TypedLoopOp::AddI64,
            TypedLoopOp::StoreI64Slot(0),
            TypedLoopOp::Jump(TypedLoopTarget::Op(0)),
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn certify_typed_ops_trusted_allows_exit_and_loopback_targets() {
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::LoadI64Slot(1),
            TypedLoopOp::JumpIfI64(I64Relation::Ge, TypedLoopTarget::Exit),
            TypedLoopOp::AddConstI64Slot(0, 1),
            TypedLoopOp::Jump(TypedLoopTarget::LoopBack),
        ];
        assert!(certify_typed_ops_trusted(&ops));
    }

    #[test]
    fn trusted_loopback_resets_every_operand_stack_pointer_10565() {
        // Codex adversarial review, round 2, residual item: the certifier
        // EXEMPTS `LoopBack` from the depth-agreement rule, on the assumption
        // that `'loop_body` re-declares i64_sp / f64_sp / bool_sp / complex_sp
        // (and the array/string Vec stacks) at 0 on every iteration. If a
        // refactor ever hoisted those out of the loop, depth would accumulate
        // across iterations and a certified stream could overflow an UNCHECKED
        // stack — so pin the assumption with an executable test rather than a
        // comment.
        //
        // This stream reaches `LoopBack` with a deliberately non-empty i64
        // stack, and loops `TYPED_LOOP_STACK_CAP * 4` times. If the stack
        // pointers survived a loop-back, i64_sp would run far past
        // TYPED_LOOP_STACK_CAP and the trusted executor would write out of
        // bounds (caught here by the `debug_assert` in `push_stack_unchecked`,
        // and by ASAN/UB in a release build).
        let iterations = (TYPED_LOOP_STACK_CAP * 4) as i64;
        let ops = vec![
            // i64[0] += 1
            TypedLoopOp::AddConstI64Slot(0, 1),
            // leave a value on the i64 stack, then loop back with it still there
            TypedLoopOp::PushI64(7),
            // exit once i64[0] >= i64[1]
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::LoadI64Slot(1),
            TypedLoopOp::JumpIfI64(I64Relation::Ge, TypedLoopTarget::Exit),
            TypedLoopOp::Jump(TypedLoopTarget::LoopBack),
        ];
        // The stream is certifiable precisely BECAUSE LoopBack/Exit are exempt
        // (both are reached with a non-empty i64 stack here).
        assert!(
            certify_typed_ops_trusted(&ops),
            "LoopBack/Exit with a non-empty stack must still certify"
        );

        let mut rng = StableRng::new(0);
        let mut st = TypedOpsState::new(0, 0);
        st.i64_locals[0] = 0;
        st.i64_init[0] = true;
        st.i64_locals[1] = iterations;
        st.i64_init[1] = true;

        let outcome = super::Vm::<StableRng>::run_typed_ops_core::<true>(
            &ops,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &mut st,
            &mut rng,
        )
        .expect("trusted core should not error");

        assert!(matches!(outcome, TypedOpsOutcome::Completed));
        // Ran the full iteration count without the stack pointers accumulating.
        assert_eq!(st.i64_locals[0], iterations);
    }

    #[test]
    fn run_typed_ops_core_trusted_and_checked_agree_on_certified_stream() {
        // One source, two monomorphizations: on a certified stream the trusted
        // and checked executors must produce the same outcome.
        let ops = vec![
            TypedLoopOp::LoadI64Slot(0),
            TypedLoopOp::PushI64(1),
            TypedLoopOp::AddI64,
            TypedLoopOp::ReturnI64,
        ];
        assert!(certify_typed_ops_trusted(&ops));

        let mut rng = StableRng::new(0);
        let mut st_checked = TypedOpsState::new(0, 0);
        st_checked.i64_locals[0] = 41;
        st_checked.i64_init[0] = true;
        let checked = super::Vm::<StableRng>::run_typed_ops_core::<false>(
            &ops,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &mut st_checked,
            &mut rng,
        )
        .expect("checked core should not error");

        let mut st_trusted = TypedOpsState::new(0, 0);
        st_trusted.i64_locals[0] = 41;
        st_trusted.i64_init[0] = true;
        let trusted = super::Vm::<StableRng>::run_typed_ops_core::<true>(
            &ops,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &mut st_trusted,
            &mut rng,
        )
        .expect("trusted core should not error");

        match (checked, trusted) {
            (
                TypedOpsOutcome::EarlyReturn(Value::I64(a)),
                TypedOpsOutcome::EarlyReturn(Value::I64(b)),
            ) => {
                assert_eq!(a, 42);
                assert_eq!(b, 42);
            }
            _ => panic!("checked and trusted must agree on EarlyReturn(I64(42))"),
        }
    }
}
