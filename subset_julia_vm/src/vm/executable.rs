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

use super::value::{ArrayElementType, ArrayRef};
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

    pub(crate) fn from_bytecode(code: &[Instr], functions: &[FunctionInfo]) -> Self {
        let mut executable = Self {
            block_by_ip: vec![NO_BLOCK; code.len()],
            blocks: Vec::new(),
            block_ips: Vec::new(),
        };
        for function in functions {
            if function.code_start >= function.code_end || function.code_end > code.len() {
                continue;
            }
            executable.predecode_range(code, function.code_start, function.code_end);
        }
        executable.block_ips.sort_unstable();
        executable.block_ips.dedup();
        executable
    }

    pub(crate) fn append_bytecode(&mut self, code: &[Instr], start: usize, end: usize) {
        if end > self.block_by_ip.len() {
            self.block_by_ip.resize(end, NO_BLOCK);
        }
        self.predecode_range(code, start, end);
        self.block_ips.sort_unstable();
        self.block_ips.dedup();
    }

    fn predecode_range(&mut self, code: &[Instr], start: usize, end: usize) {
        let mut ip = start;
        while ip < end {
            // Run the recognizer pipeline (Issue #6829): the first registered
            // recognizer that matches this `ip` produces the executable block.
            if let Some(block) = LOOP_RECOGNIZERS
                .iter()
                .find_map(|recognize| recognize(code, ip, end))
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

    #[cfg(test)]
    fn has_complex_f64_mandelbrot_escape_loop(&self) -> bool {
        self.blocks
            .iter()
            .any(|block| matches!(block, ExecutableBlock::ComplexF64MandelbrotEscape(_)))
    }
}

#[derive(Debug, Clone)]
enum ExecutableBlock {
    EuclideanModuloI64Loop(EuclideanModuloI64LoopBlock),
    Typed(TypedLoopBlock),
    ComplexF64MandelbrotEscape(ComplexF64MandelbrotEscapeLoopBlock),
}

/// A predecode *recognizer*: the pattern-matcher + validator stage of the
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
type LoopRecognizer = fn(&[Instr], usize, usize) -> Option<ExecutableBlock>;

fn recognize_euclidean_modulo_i64_loop(
    code: &[Instr],
    ip: usize,
    end: usize,
) -> Option<ExecutableBlock> {
    try_predecode_euclidean_modulo_i64_loop(code, ip, end)
        .map(ExecutableBlock::EuclideanModuloI64Loop)
}

fn recognize_complex_f64_mandelbrot_escape_loop(
    code: &[Instr],
    ip: usize,
    end: usize,
) -> Option<ExecutableBlock> {
    try_predecode_complex_f64_mandelbrot_escape_loop(code, ip, end)
        .map(ExecutableBlock::ComplexF64MandelbrotEscape)
}

fn recognize_typed_loop(code: &[Instr], ip: usize, end: usize) -> Option<ExecutableBlock> {
    try_predecode_typed_loop(code, ip, end).map(ExecutableBlock::Typed)
}

/// Ordered predecode recognizer registry (Issue #6829). The first recognizer to
/// match a given `ip` wins; order encodes precedence (the specific euclidean /
/// mandelbrot shapes are tried before the general typed loop).
const LOOP_RECOGNIZERS: &[LoopRecognizer] = &[
    recognize_euclidean_modulo_i64_loop,
    recognize_complex_f64_mandelbrot_escape_loop,
    recognize_typed_loop,
];

#[derive(Debug, Clone)]
pub(crate) enum ExecutableBlockResult {
    NotExecuted,
    Continue,
    Exit(Value),
}

#[derive(Debug, Clone)]
struct EuclideanModuloI64LoopBlock {
    header_ip: usize,
    exit_ip: usize,
    a_slot: usize,
    b_slot: usize,
    tmp_slot: usize,
}

#[derive(Debug, Clone)]
struct TypedLoopBlock {
    exit_ip: usize,
    array_slots: Vec<TypedLoopSlot>,
    f64_slots: Vec<TypedLoopSlot>,
    i64_slots: Vec<TypedLoopSlot>,
    ops: Vec<TypedLoopOp>,
}

#[derive(Debug, Clone)]
pub(crate) struct I64FunctionBlock {
    slots: Vec<I64FunctionSlot>,
    ops: Vec<I64FunctionOp>,
    callees: Vec<I64FunctionBlock>,
}

#[derive(Debug, Clone)]
struct ComplexF64MandelbrotEscapeLoopBlock {
    c_slot: usize,
    maxiter_slot: usize,
    z_slot: usize,
    k_slot: usize,
}

#[derive(Debug, Clone)]
struct TypedLoopSlot {
    slot: usize,
    live_in: bool,
    written_in_loop: bool,
}

#[derive(Debug, Clone)]
struct I64FunctionSlot {
    slot: usize,
    param_index: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
enum TypedLoopOp {
    LoadArraySlot(usize),
    IndexStoreI64,
    IndexStoreF64,
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
    CmpI64(I64Relation),
    CmpF64(F64Relation),
    JumpIfZero(TypedLoopTarget),
    JumpIfI64(I64Relation, TypedLoopTarget),
    JumpIfI64Slots(usize, usize, I64Relation, TypedLoopTarget),
    AddConstI64SlotAndJumpIfLe(usize, i64, usize, TypedLoopTarget),
    JumpIfF64(F64Relation, TypedLoopTarget),
    JumpIfNotF64(F64Relation, TypedLoopTarget),
    Jump(TypedLoopTarget),
}

#[derive(Debug, Clone, Copy)]
enum I64FunctionOp {
    PushI64(i64),
    LoadI64Slot(usize),
    StoreI64Slot(usize),
    AddI64,
    SubI64,
    MulI64,
    ModI64,
    AbsI64,
    LoadAddI64Slot(usize),
    LoadSubI64Slot(usize),
    LoadMulI64Slot(usize),
    LoadModI64Slot(usize),
    IncI64Slot(usize),
    DecI64Slot(usize),
    AddConstI64Slot(usize, i64),
    CallI64Function(usize, usize),
    CmpI64(I64Relation),
    JumpIfZero(usize),
    JumpIfI64(I64Relation, usize),
    JumpIfI64Slots(usize, usize, I64Relation, usize),
    AddConstI64SlotAndJumpIfLe(usize, i64, usize, usize),
    Jump(usize),
    ReturnI64,
}

#[derive(Debug, Clone, Copy)]
enum TypedLoopTarget {
    Op(usize),
    Exit,
    LoopBack,
}

#[derive(Debug, Clone, Copy)]
enum I64Relation {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, Copy)]
enum F64Relation {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

// Issue #8183: dense Float64 ODE / iterated-map bodies (Aizawa attractor ≈68
// ops, Barnsley fern ≈92 ops) are larger than the original 64-op window. Raised
// to 128 so these scalar hot loops are recognized as native typed loops. The
// cap only bounds the one-time predecode scan and the heap `ops` Vec; the
// per-iteration fixed stacks are sized by `TYPED_LOOP_STACK_CAP`.
const MAX_TYPED_LOOP_OPS: usize = 128;
const MAX_I64_FUNCTION_OPS: usize = 128;
const MAX_COMPLEX_MANDELBROT_ESCAPE_OPS: usize = 128;
const TYPED_LOOP_STACK_CAP: usize = 16;
// Issue #8183: the Aizawa ODE step keeps 16 live Float64 locals; raised from 16
// to 24 for headroom so such bodies clear the slot-count guard.
const TYPED_LOOP_SLOT_CAP: usize = 24;
const I64_FUNCTION_SLOT_CAP: usize = 16;
const I64_FUNCTION_CALLEE_CAP: usize = 8;
const MAX_I64_FUNCTION_CALL_DEPTH: usize = 4;

fn try_predecode_euclidean_modulo_i64_loop(
    code: &[Instr],
    header_ip: usize,
    function_end: usize,
) -> Option<EuclideanModuloI64LoopBlock> {
    if header_ip.checked_add(12)? > function_end || header_ip + 12 > code.len() {
        return None;
    }

    let b_slot = load_slot_index(code.get(header_ip)?)?;
    if !matches!(code.get(header_ip + 1), Some(Instr::PushI64(0))) {
        return None;
    }

    let (body_ip, exit_ip) = match (code.get(header_ip + 2)?, code.get(header_ip + 3)) {
        (Instr::JumpIfEqI64(target), _) => (header_ip + 3, *target),
        (
            Instr::CallDynamicBinaryBoth(Intrinsic::NeFloat | Intrinsic::NeInt, _) | Instr::NeI64,
            Some(Instr::JumpIfZero(target)),
        ) => (header_ip + 4, *target),
        _ => return None,
    };

    match code.get(body_ip) {
        Some(instr) if load_slot_index(instr) == Some(b_slot) => {}
        _ => return None,
    }
    let tmp_slot = store_slot_index(code.get(body_ip + 1)?)?;
    let a_slot = load_slot_index(code.get(body_ip + 2)?)?;

    let after_mod_ip = if matches!(
        code.get(body_ip + 3),
        Some(Instr::LoadModI64Slot(slot)) if *slot == b_slot
    ) {
        body_ip + 4
    } else {
        match code.get(body_ip + 3) {
            Some(instr) if load_slot_index(instr) == Some(b_slot) => {}
            _ => return None,
        }
        if !matches!(
            code.get(body_ip + 4),
            Some(Instr::CallDynamicBinaryBoth(Intrinsic::SremInt, _) | Instr::ModI64)
        ) {
            return None;
        }
        body_ip + 5
    };

    match code.get(after_mod_ip) {
        Some(instr) if store_slot_index(instr) == Some(b_slot) => {}
        _ => return None,
    }
    match code.get(after_mod_ip + 1) {
        Some(instr) if load_slot_index(instr) == Some(tmp_slot) => {}
        _ => return None,
    }
    match code.get(after_mod_ip + 2) {
        Some(instr) if store_slot_index(instr) == Some(a_slot) => {}
        _ => return None,
    }
    match code.get(after_mod_ip + 3) {
        Some(Instr::Jump(target)) if *target == header_ip => {}
        _ => return None,
    }
    if exit_ip != after_mod_ip + 4 {
        return None;
    }

    Some(EuclideanModuloI64LoopBlock {
        header_ip,
        exit_ip,
        a_slot,
        b_slot,
        tmp_slot,
    })
}

fn load_slot_index(instr: &Instr) -> Option<usize> {
    match instr {
        Instr::LoadSlot(slot) | Instr::LoadSlotI64(slot) => Some(*slot),
        _ => None,
    }
}

fn store_slot_index(instr: &Instr) -> Option<usize> {
    match instr {
        Instr::StoreSlot(slot) | Instr::StoreSlotI64(slot) => Some(*slot),
        _ => None,
    }
}

fn load_complex_slot_index(instr: &Instr) -> Option<usize> {
    match instr {
        Instr::LoadSlot(slot) | Instr::LoadSlotStruct(slot) => Some(*slot),
        _ => None,
    }
}

fn store_complex_slot_index(instr: &Instr) -> Option<usize> {
    match instr {
        Instr::StoreSlot(slot) | Instr::StoreSlotStruct(slot) => Some(*slot),
        _ => None,
    }
}

/// Match the counted-loop header of the complex-f64 Mandelbrot escape loop,
/// returning `(k_slot, maxiter_slot, exit_ip, body_ip)` where `body_ip` is the
/// first instruction after the guard.
///
/// Accepts both the *unfused* shape the specializer emits pre-peephole
/// (`Load k; Load maxiter; LeI64; JumpIfZero exit`, body at `+4`) and the
/// *pop-based fused* shape the post-slotize peephole pass now produces on
/// specialized bodies (`Load k; Load maxiter; JumpIfGtI64 exit`, body at `+3`).
/// Running peephole on specialized bodies (Issue #8205) fused this guard, and
/// without the fused arm the Mandelbrot body fell through to the generic
/// interpreter — the recognizer pattern had been coupled to unfused specializer
/// codegen (Issue #8192). The fully-typed `JumpIfGtI64Slots` form (a typed
/// `maxiter`) pairs with an `AddConstI64SlotAndJumpIfLe` back-edge that
/// `match_i64_increment_backedge` does not model, so it is intentionally left
/// unrecognized here rather than half-matched.
fn match_complex_mandelbrot_loop_header(
    code: &[Instr],
    header_ip: usize,
) -> Option<(usize, usize, usize, usize)> {
    let k_slot = load_slot_index(code.get(header_ip)?)?;
    let maxiter_slot = load_slot_index(code.get(header_ip + 1)?)?;
    match code.get(header_ip + 2)? {
        // Fused pop-based guard: branch to the exit when `k > maxiter`.
        Instr::JumpIfGtI64(exit_ip) => Some((k_slot, maxiter_slot, *exit_ip, header_ip + 3)),
        // Unfused guard: `k <= maxiter` then branch-if-false to the exit.
        Instr::LeI64 => match code.get(header_ip + 3)? {
            Instr::JumpIfZero(exit_ip) => Some((k_slot, maxiter_slot, *exit_ip, header_ip + 4)),
            _ => None,
        },
        _ => None,
    }
}

fn try_predecode_complex_f64_mandelbrot_escape_loop(
    code: &[Instr],
    header_ip: usize,
    function_end: usize,
) -> Option<ComplexF64MandelbrotEscapeLoopBlock> {
    if header_ip.checked_add(8)? > function_end {
        return None;
    }

    let (k_slot, maxiter_slot, end_ip, body_ip) =
        match_complex_mandelbrot_loop_header(code, header_ip)?;
    if end_ip <= header_ip
        || end_ip.checked_add(2)? > function_end
        || end_ip - header_ip > MAX_COMPLEX_MANDELBROT_ESCAPE_OPS
    {
        return None;
    }
    if load_slot_index(code.get(end_ip)?) != Some(maxiter_slot)
        || !matches!(code.get(end_ip + 1), Some(Instr::ReturnI64))
    {
        return None;
    }

    let (after_abs2_branch, z_slot) = match_complex_abs2_gt_four_branch(code, body_ip, k_slot)?;
    let (after_update, c_slot) =
        match_complex_square_plus_c_update(code, after_abs2_branch, z_slot)?;
    match_i64_increment_backedge(code, after_update, k_slot, header_ip, end_ip)?;

    Some(ComplexF64MandelbrotEscapeLoopBlock {
        c_slot,
        maxiter_slot,
        z_slot,
        k_slot,
    })
}

fn match_complex_abs2_gt_four_branch(
    code: &[Instr],
    start_ip: usize,
    k_slot: usize,
) -> Option<(usize, usize)> {
    let z_slot = load_complex_slot_index(code.get(start_ip)?)?;
    let abs_temp = store_any_name(code.get(start_ip + 1)?)?;
    let mut ip = start_ip + 2;
    ip = match_load_temp_field(code, ip, abs_temp, 0)?;
    if !matches!(code.get(ip), Some(Instr::DupF64))
        || !matches!(code.get(ip + 1), Some(Instr::MulF64))
    {
        return None;
    }
    ip += 2;
    ip = match_load_temp_field(code, ip, abs_temp, 1)?;
    if !matches!(code.get(ip), Some(Instr::DupF64))
        || !matches!(code.get(ip + 1), Some(Instr::MulF64))
        || !matches!(code.get(ip + 2), Some(Instr::AddF64))
        || !matches!(code.get(ip + 3), Some(Instr::PushF64(value)) if *value == 4.0)
    {
        return None;
    }
    ip += 4;
    // The `abs2(z) > 4.0` escape guard is either unfused (`GtF64; JumpIfZero
    // exit`) or fused by the post-slotize peephole pass into a single
    // `JumpIfNotGtF64 exit`. Running peephole on specialized bodies (Issue
    // #8205) introduced the fused form here; without this arm the Mandelbrot
    // body fell through to the generic interpreter (Issue #8192).
    let after_return = match code.get(ip)? {
        Instr::GtF64 => match code.get(ip + 1)? {
            Instr::JumpIfZero(target) => {
                ip += 2;
                *target
            }
            _ => return None,
        },
        Instr::JumpIfNotGtF64(target) => {
            ip += 1;
            *target
        }
        _ => return None,
    };
    if load_slot_index(code.get(ip)?) != Some(k_slot)
        || !matches!(code.get(ip + 1), Some(Instr::ReturnI64))
        || after_return != ip + 2
    {
        return None;
    }
    Some((after_return, z_slot))
}

fn match_complex_square_plus_c_update(
    code: &[Instr],
    start_ip: usize,
    z_slot: usize,
) -> Option<(usize, usize)> {
    let square_source_slot = load_complex_slot_index(code.get(start_ip)?)?;
    if square_source_slot != z_slot {
        return None;
    }
    let square_temp = store_any_name(code.get(start_ip + 1)?)?;
    let mut ip = start_ip + 2;

    ip = match_load_temp_field(code, ip, square_temp, 0)?;
    if !matches!(code.get(ip), Some(Instr::DupF64))
        || !matches!(code.get(ip + 1), Some(Instr::MulF64))
    {
        return None;
    }
    ip += 2;
    ip = match_load_temp_field(code, ip, square_temp, 1)?;
    if !matches!(code.get(ip), Some(Instr::DupF64))
        || !matches!(code.get(ip + 1), Some(Instr::MulF64))
        || !matches!(code.get(ip + 2), Some(Instr::SubF64))
    {
        return None;
    }
    ip += 3;

    ip = match_load_temp_field(code, ip, square_temp, 0)?;
    ip = match_load_temp_field(code, ip, square_temp, 1)?;
    if !matches!(code.get(ip), Some(Instr::MulF64))
        || !matches!(code.get(ip + 1), Some(Instr::PushF64(value)) if *value == 2.0)
        || !matches!(code.get(ip + 2), Some(Instr::MulF64))
        || !is_new_complex_f64(code.get(ip + 3)?)
    {
        return None;
    }
    ip += 4;

    let left_temp = store_any_name(code.get(ip)?)?;
    ip += 1;
    let c_slot = load_complex_slot_index(code.get(ip)?)?;
    let right_temp = store_any_name(code.get(ip + 1)?)?;
    ip += 2;

    ip = match_load_temp_field(code, ip, left_temp, 0)?;
    ip = match_load_temp_field(code, ip, right_temp, 0)?;
    if !matches!(code.get(ip), Some(Instr::AddF64)) {
        return None;
    }
    ip += 1;
    ip = match_load_temp_field(code, ip, left_temp, 1)?;
    ip = match_load_temp_field(code, ip, right_temp, 1)?;
    if !matches!(code.get(ip), Some(Instr::AddF64)) || !is_new_complex_f64(code.get(ip + 1)?) {
        return None;
    }
    ip += 2;

    if store_complex_slot_index(code.get(ip)?) != Some(z_slot) {
        return None;
    }
    Some((ip + 1, c_slot))
}

fn match_i64_increment_backedge(
    code: &[Instr],
    ip: usize,
    k_slot: usize,
    header_ip: usize,
    end_ip: usize,
) -> Option<()> {
    if load_slot_index(code.get(ip)?) == Some(k_slot)
        && matches!(code.get(ip + 1), Some(Instr::PushI64(1)))
        && matches!(code.get(ip + 2), Some(Instr::AddI64))
        && store_slot_index(code.get(ip + 3)?) == Some(k_slot)
        && matches!(code.get(ip + 4), Some(Instr::Jump(target)) if *target == header_ip)
        && ip + 5 == end_ip
    {
        return Some(());
    }
    if matches!(code.get(ip), Some(Instr::PushI64(1)))
        && matches!(code.get(ip + 1), Some(Instr::IncVarI64Slot(slot)) if *slot == k_slot)
        && matches!(code.get(ip + 2), Some(Instr::Jump(target)) if *target == header_ip)
        && ip + 3 == end_ip
    {
        return Some(());
    }
    if matches!(code.get(ip), Some(Instr::AddConstI64Slot(slot, 1)) if *slot == k_slot)
        && matches!(code.get(ip + 1), Some(Instr::Jump(target)) if *target == header_ip)
        && ip + 2 == end_ip
    {
        return Some(());
    }
    None
}

fn match_load_temp_field(
    code: &[Instr],
    ip: usize,
    temp_name: &str,
    field: usize,
) -> Option<usize> {
    if load_any_name(code.get(ip)?) == Some(temp_name)
        && matches!(code.get(ip + 1), Some(Instr::GetField(actual)) if *actual == field)
    {
        Some(ip + 2)
    } else {
        None
    }
}

fn load_any_name(instr: &Instr) -> Option<&str> {
    match instr {
        Instr::LoadAny(name) => Some(name),
        _ => None,
    }
}

fn store_any_name(instr: &Instr) -> Option<&str> {
    match instr {
        Instr::StoreAny(name) => Some(name),
        _ => None,
    }
}

fn is_new_complex_f64(instr: &Instr) -> bool {
    matches!(instr, Instr::NewParametricStruct(name, 2) if name == "Complex")
}

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

fn try_predecode_typed_loop(
    code: &[Instr],
    header_ip: usize,
    function_end: usize,
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
            if let Some(block) =
                try_predecode_typed_loop_range(code, header_ip, jump_ip + 1, &mut reject)
            {
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
    header_ip: usize,
    end_ip: usize,
    reject: &mut Option<TypedLoopReject>,
) -> Option<TypedLoopBlock> {
    if end_ip <= header_ip || end_ip > code.len() {
        return None;
    }
    if end_ip - header_ip > MAX_TYPED_LOOP_OPS {
        *reject = Some(TypedLoopReject::OpCountOverCap);
        return None;
    }

    let mut builder = TypedLoopBuilder::default();
    let mut has_exit = false;
    for ip in header_ip..end_ip {
        let instr = code.get(ip)?;
        match instr {
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
                builder.ops.push(TypedLoopOp::LoadArraySlot(local));
            }
            Instr::StoreSlotArray(slot) => {
                // Workaround: skip typed executable StoreSlotArray for generic VM fallback (Issue #7538).
                // The regular VM StoreSlotArray path can fall back
                // to storing arbitrary Value payloads when a statically
                // array-typed slot receives a macro/runtime Expr array. The
                // typed executable array stack has no equivalent fallback, so
                // let the normal interpreter handle this instruction.
                let _ = slot;
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
    if builder.array_slots.len() > TYPED_LOOP_SLOT_CAP
        || builder.f64_slots.len() > TYPED_LOOP_SLOT_CAP
        || builder.i64_slots.len() > TYPED_LOOP_SLOT_CAP
    {
        *reject = Some(TypedLoopReject::SlotCountOverCap);
        return None;
    }
    Some(TypedLoopBlock {
        exit_ip: end_ip,
        array_slots: builder.array_slots,
        f64_slots: builder.f64_slots,
        i64_slots: builder.i64_slots,
        ops: builder.ops,
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

fn try_predecode_i64_function(
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

    let mut builder = I64FunctionBuilder::new(param_slots);
    let mut has_return = false;
    for ip in entry_ip..end_ip {
        let instr = code.get(ip)?;
        match instr {
            Instr::PushI64(value) => {
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::PushI64(*value));
            }
            Instr::LoadSlot(slot) | Instr::LoadSlotI64(slot) => {
                let local = builder.i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::LoadI64Slot(local));
            }
            Instr::StoreSlot(slot) | Instr::StoreSlotI64(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.ops.push(I64FunctionOp::StoreI64Slot(local));
            }
            Instr::AddI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::AddI64);
            }
            Instr::SubI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::SubI64);
            }
            Instr::MulI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::MulI64);
            }
            Instr::ModI64 => {
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::ModI64);
            }
            Instr::LoadAddI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::LoadAddI64Slot(local));
            }
            Instr::LoadSubI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::LoadSubI64Slot(local));
            }
            Instr::LoadMulI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::LoadMulI64Slot(local));
            }
            Instr::LoadModI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.push_i64()?;
                builder.ops.push(I64FunctionOp::LoadModI64Slot(local));
            }
            Instr::IncVarI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.ops.push(I64FunctionOp::IncI64Slot(local));
            }
            Instr::DecVarI64Slot(slot) => {
                builder.pop_i64()?;
                let local = builder.i64_slot(*slot);
                builder.ops.push(I64FunctionOp::DecI64Slot(local));
            }
            Instr::AddConstI64Slot(slot, delta) => {
                let local = builder.i64_slot(*slot);
                builder
                    .ops
                    .push(I64FunctionOp::AddConstI64Slot(local, *delta));
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
                builder
                    .ops
                    .push(I64FunctionOp::CmpI64(i64_relation(instr)?));
            }
            Instr::CallDynamicBinaryBoth(intrinsic, _) => {
                if let Some(op) = i64_function_arithmetic_intrinsic(intrinsic) {
                    builder.pop_i64()?;
                    builder.pop_i64()?;
                    builder.push_i64()?;
                    builder.ops.push(op);
                } else if let Some(relation) = i64_function_relation_intrinsic(intrinsic) {
                    builder.pop_i64()?;
                    builder.pop_i64()?;
                    builder.push_bool()?;
                    builder.ops.push(I64FunctionOp::CmpI64(relation));
                } else {
                    return None;
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
                    builder.pop_i64()?;
                }
                builder.push_i64()?;
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
                    let local = builder.i64_slot(*slot);
                    builder.push_i64()?;
                    builder.ops.push(I64FunctionOp::LoadI64Slot(local));
                }
                for _ in 0..arg_count {
                    builder.pop_i64()?;
                }
                builder.push_i64()?;
                builder.ops.push(op);
            }
            Instr::JumpIfZero(target) => {
                let target = i64_function_target(entry_ip, end_ip, *target)?;
                builder.pop_bool()?;
                builder.ops.push(I64FunctionOp::JumpIfZero(target));
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
                let target = i64_function_target(entry_ip, end_ip, *target)?;
                builder.pop_i64()?;
                builder.pop_i64()?;
                builder
                    .ops
                    .push(I64FunctionOp::JumpIfI64(i64_relation(instr)?, target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target) => {
                let target = i64_function_target(entry_ip, end_ip, *target)?;
                let lhs_local = builder.i64_slot(*lhs_slot);
                let rhs_local = builder.i64_slot(*rhs_slot);
                builder.ops.push(I64FunctionOp::JumpIfI64Slots(
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
                let target = i64_function_target(entry_ip, end_ip, *target)?;
                let local = builder.i64_slot(*slot);
                let stop_local = builder.i64_slot(*stop_slot);
                builder.ops.push(I64FunctionOp::AddConstI64SlotAndJumpIfLe(
                    local, *delta, stop_local, target,
                ));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::Jump(target) => {
                let target = i64_function_target(entry_ip, end_ip, *target)?;
                builder.ops.push(I64FunctionOp::Jump(target));
                if !builder.stack_is_empty() {
                    return None;
                }
            }
            Instr::ReturnI64 => {
                builder.pop_i64()?;
                if !builder.stack_is_empty() {
                    return None;
                }
                builder.ops.push(I64FunctionOp::ReturnI64);
                has_return = true;
            }
            _ => return None,
        }
    }

    if !has_return || builder.slots.len() > I64_FUNCTION_SLOT_CAP {
        return None;
    }
    Some(I64FunctionBlock {
        slots: builder.slots,
        ops: builder.ops,
        callees: builder.callees,
    })
}

fn i64_function_target(entry_ip: usize, end_ip: usize, target_ip: usize) -> Option<usize> {
    if target_ip >= entry_ip && target_ip < end_ip {
        return Some(target_ip - entry_ip);
    }
    None
}

fn i64_function_arithmetic_intrinsic(intrinsic: &Intrinsic) -> Option<I64FunctionOp> {
    match intrinsic {
        Intrinsic::AddFloat | Intrinsic::AddInt => Some(I64FunctionOp::AddI64),
        Intrinsic::SubFloat | Intrinsic::SubInt => Some(I64FunctionOp::SubI64),
        Intrinsic::MulFloat | Intrinsic::MulInt => Some(I64FunctionOp::MulI64),
        Intrinsic::SremInt => Some(I64FunctionOp::ModI64),
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
    builder: &mut I64FunctionBuilder<'_>,
) -> Option<I64FunctionOp> {
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
    Some(I64FunctionOp::CallI64Function(callee_index, arg_count))
}

fn i64_function_base_unary_call(
    functions: &[Rc<FunctionInfo>],
    base_function_count: usize,
    target_index: usize,
    arg_count: usize,
) -> Option<I64FunctionOp> {
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
        return Some(I64FunctionOp::AbsI64);
    }
    None
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
    ops: Vec<TypedLoopOp>,
    array_depth: usize,
    f64_depth: usize,
    i64_depth: usize,
    bool_depth: usize,
}

impl TypedLoopBuilder {
    fn read_array_slot(&mut self, slot: usize) -> usize {
        read_typed_slot(&mut self.array_slots, slot)
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

    fn stack_is_empty(&self) -> bool {
        self.array_depth == 0 && self.f64_depth == 0 && self.i64_depth == 0 && self.bool_depth == 0
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

struct I64FunctionBuilder<'a> {
    param_slots: &'a [usize],
    slots: Vec<I64FunctionSlot>,
    ops: Vec<I64FunctionOp>,
    callees: Vec<I64FunctionBlock>,
    i64_depth: usize,
    bool_depth: usize,
}

impl<'a> I64FunctionBuilder<'a> {
    fn new(param_slots: &'a [usize]) -> Self {
        Self {
            param_slots,
            slots: Vec::new(),
            ops: Vec::new(),
            callees: Vec::new(),
            i64_depth: 0,
            bool_depth: 0,
        }
    }

    fn i64_slot(&mut self, slot: usize) -> usize {
        if let Some(index) = self.slots.iter().position(|entry| entry.slot == slot) {
            return index;
        }
        let index = self.slots.len();
        self.slots.push(I64FunctionSlot {
            slot,
            param_index: self
                .param_slots
                .iter()
                .position(|param_slot| *param_slot == slot),
        });
        index
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

    fn stack_is_empty(&self) -> bool {
        self.i64_depth == 0 && self.bool_depth == 0
    }

    fn add_callee(&mut self, block: I64FunctionBlock) -> Option<usize> {
        if self.callees.len() >= I64_FUNCTION_CALLEE_CAP {
            return None;
        }
        let index = self.callees.len();
        self.callees.push(block);
        Some(index)
    }
}

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
            ExecutableBlock::EuclideanModuloI64Loop(block) => {
                self.execute_euclidean_modulo_i64_loop_block(&block)
            }
            ExecutableBlock::Typed(block) => self.execute_typed_loop_block(&block),
            ExecutableBlock::ComplexF64MandelbrotEscape(block) => {
                self.execute_complex_f64_mandelbrot_escape_loop_block(&block)
            }
        }
    }

    #[inline]
    pub(crate) fn try_execute_euclidean_modulo_i64_function_call(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[Value],
    ) -> Option<Value> {
        let (a_arg, b_arg) =
            self.euclidean_modulo_i64_function_arg_indices(entry_ip, end_ip, param_slots)?;
        let Value::I64(a) = *args.get(a_arg)? else {
            return None;
        };
        let Value::I64(b) = *args.get(b_arg)? else {
            return None;
        };
        Some(Value::I64(self.execute_euclidean_modulo_i64_values(a, b)?))
    }

    pub(crate) fn try_execute_euclidean_modulo_i64_function_call_i64_args(
        &mut self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
        args: &[i64],
    ) -> Option<i64> {
        let (a_arg, b_arg) =
            self.euclidean_modulo_i64_function_arg_indices(entry_ip, end_ip, param_slots)?;
        let a = *args.get(a_arg)?;
        let b = *args.get(b_arg)?;
        self.execute_euclidean_modulo_i64_values(a, b)
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
        }
        let block = self.i64_function_cache.get(&entry_ip)?.as_ref()?;
        Self::execute_i64_function_block(block, args)
    }

    fn euclidean_modulo_i64_function_arg_indices(
        &self,
        entry_ip: usize,
        end_ip: usize,
        param_slots: &[usize],
    ) -> Option<(usize, usize)> {
        let code = self.code.as_ref();
        let block = try_predecode_euclidean_modulo_i64_loop(code, entry_ip, end_ip)?;
        if block.exit_ip + 2 > end_ip {
            return None;
        }
        if load_slot_index(code.get(block.exit_ip)?) != Some(block.a_slot)
            || !matches!(code.get(block.exit_ip + 1), Some(Instr::ReturnI64))
        {
            return None;
        }

        let a_arg = param_slots.iter().position(|slot| *slot == block.a_slot)?;
        let b_arg = param_slots.iter().position(|slot| *slot == block.b_slot)?;
        Some((a_arg, b_arg))
    }

    fn execute_euclidean_modulo_i64_values(&mut self, mut a: i64, mut b: i64) -> Option<i64> {
        profiler::record_event("ExecutableBlock::EuclideanModuloI64Function");
        while b != 0 {
            if a == i64::MIN && b == -1 {
                return None;
            }
            let old_b = b;
            b = a.wrapping_rem(b);
            a = old_b;
        }
        Some(a)
    }

    fn execute_i64_function_block(block: &I64FunctionBlock, args: &[i64]) -> Option<i64> {
        if block.slots.len() > I64_FUNCTION_SLOT_CAP || block.ops.len() > MAX_I64_FUNCTION_OPS {
            return None;
        }

        let mut locals = [0_i64; I64_FUNCTION_SLOT_CAP];
        let mut local_init = [false; I64_FUNCTION_SLOT_CAP];
        for (local, slot) in block.slots.iter().enumerate() {
            if let Some(param_index) = slot.param_index {
                locals[local] = *args.get(param_index)?;
                local_init[local] = true;
            }
        }

        profiler::record_event("ExecutableBlock::I64Function");

        let mut i64_stack = [0_i64; TYPED_LOOP_STACK_CAP];
        let mut bool_stack = [false; TYPED_LOOP_STACK_CAP];
        let mut i64_sp = 0usize;
        let mut bool_sp = 0usize;
        let mut op_pc = 0usize;

        while op_pc < block.ops.len() {
            match block.ops[op_pc] {
                I64FunctionOp::PushI64(value) => {
                    push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    op_pc += 1;
                }
                I64FunctionOp::LoadI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    push_i64_stack(&mut i64_stack, &mut i64_sp, locals[local]);
                    op_pc += 1;
                }
                I64FunctionOp::StoreI64Slot(local) => {
                    let value = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    locals[local] = value;
                    local_init[local] = true;
                    op_pc += 1;
                }
                I64FunctionOp::AddI64 => {
                    let (lhs, rhs) = pop2_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_add(rhs));
                    op_pc += 1;
                }
                I64FunctionOp::SubI64 => {
                    let (lhs, rhs) = pop2_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_sub(rhs));
                    op_pc += 1;
                }
                I64FunctionOp::MulI64 => {
                    let (lhs, rhs) = pop2_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_mul(rhs));
                    op_pc += 1;
                }
                I64FunctionOp::ModI64 => {
                    let (lhs, rhs) = pop2_i64_stack(&i64_stack, &mut i64_sp)?;
                    let value = checked_i64_rem(lhs, rhs)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    op_pc += 1;
                }
                I64FunctionOp::AbsI64 => {
                    let value = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, value.wrapping_abs());
                    op_pc += 1;
                }
                I64FunctionOp::LoadAddI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_add(locals[local]));
                    op_pc += 1;
                }
                I64FunctionOp::LoadSubI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_sub(locals[local]));
                    op_pc += 1;
                }
                I64FunctionOp::LoadMulI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_mul(locals[local]));
                    op_pc += 1;
                }
                I64FunctionOp::LoadModI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let lhs = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    let value = checked_i64_rem(lhs, locals[local])?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    op_pc += 1;
                }
                I64FunctionOp::IncI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let delta = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    locals[local] = locals[local].wrapping_add(delta);
                    op_pc += 1;
                }
                I64FunctionOp::DecI64Slot(local) => {
                    if !local_init[local] {
                        return None;
                    }
                    let delta = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    locals[local] = locals[local].wrapping_sub(delta);
                    op_pc += 1;
                }
                I64FunctionOp::AddConstI64Slot(local, delta) => {
                    if !local_init[local] {
                        return None;
                    }
                    locals[local] = locals[local].wrapping_add(delta);
                    op_pc += 1;
                }
                I64FunctionOp::CallI64Function(callee_index, arg_count) => {
                    if arg_count > TYPED_LOOP_STACK_CAP {
                        return None;
                    }
                    let callee = block.callees.get(callee_index)?;
                    let mut call_args = [0_i64; TYPED_LOOP_STACK_CAP];
                    for index in (0..arg_count).rev() {
                        call_args[index] = pop_i64_stack(&i64_stack, &mut i64_sp)?;
                    }
                    profiler::record_event("ExecutableBlock::I64FunctionNestedCall");
                    let value = Self::execute_i64_function_block(callee, &call_args[..arg_count])?;
                    push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    op_pc += 1;
                }
                I64FunctionOp::CmpI64(relation) => {
                    let (lhs, rhs) = pop2_i64_stack(&i64_stack, &mut i64_sp)?;
                    push_bool_stack(
                        &mut bool_stack,
                        &mut bool_sp,
                        eval_i64_relation(lhs, rhs, relation),
                    );
                    op_pc += 1;
                }
                I64FunctionOp::JumpIfZero(target) => {
                    let cond = pop_bool_stack(&bool_stack, &mut bool_sp)?;
                    op_pc = if cond { op_pc + 1 } else { target };
                }
                I64FunctionOp::JumpIfI64(relation, target) => {
                    let (lhs, rhs) = pop2_i64_stack(&i64_stack, &mut i64_sp)?;
                    op_pc = if eval_i64_relation(lhs, rhs, relation) {
                        target
                    } else {
                        op_pc + 1
                    };
                }
                I64FunctionOp::JumpIfI64Slots(lhs_local, rhs_local, relation, target) => {
                    if !local_init[lhs_local] || !local_init[rhs_local] {
                        return None;
                    }
                    op_pc = if eval_i64_relation(locals[lhs_local], locals[rhs_local], relation) {
                        target
                    } else {
                        op_pc + 1
                    };
                }
                I64FunctionOp::AddConstI64SlotAndJumpIfLe(local, delta, stop_local, target) => {
                    if !local_init[local] || !local_init[stop_local] {
                        return None;
                    }
                    locals[local] = locals[local].wrapping_add(delta);
                    op_pc = if locals[local] <= locals[stop_local] {
                        target
                    } else {
                        op_pc + 1
                    };
                }
                I64FunctionOp::Jump(target) => {
                    op_pc = target;
                }
                I64FunctionOp::ReturnI64 => {
                    return pop_i64_stack(&i64_stack, &mut i64_sp);
                }
            }
        }
        None
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

    fn execute_euclidean_modulo_i64_loop_block(
        &mut self,
        block: &EuclideanModuloI64LoopBlock,
    ) -> Result<ExecutableBlockResult, super::VmError> {
        let Some(frame) = self.frames.last_mut() else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };

        let Some(mut a) = load_i64_slot(frame, block.a_slot) else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        let Some(mut b) = load_i64_slot(frame, block.b_slot) else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };

        profiler::record_event("ExecutableBlock::EuclideanModuloI64Loop");

        let mut tmp = None;
        while b != 0 {
            let old_b = b;
            if a == i64::MIN && b == -1 {
                sync_euclidean_modulo_i64_slots(frame, block, a, b, tmp);
                self.ip = block.header_ip;
                return Ok(ExecutableBlockResult::NotExecuted);
            }
            b = a.wrapping_rem(b);
            a = old_b;
            tmp = Some(old_b);
        }

        sync_euclidean_modulo_i64_slots(frame, block, a, b, tmp);
        self.ip = block.exit_ip;
        Ok(ExecutableBlockResult::Continue)
    }

    fn execute_typed_loop_block(
        &mut self,
        block: &TypedLoopBlock,
    ) -> Result<ExecutableBlockResult, super::VmError> {
        let Some(frame) = self.frames.last_mut() else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        if block.array_slots.len() > TYPED_LOOP_SLOT_CAP
            || block.f64_slots.len() > TYPED_LOOP_SLOT_CAP
            || block.i64_slots.len() > TYPED_LOOP_SLOT_CAP
        {
            return Ok(ExecutableBlockResult::NotExecuted);
        }

        let mut array_locals = vec![None; block.array_slots.len()];
        let mut array_init = vec![false; block.array_slots.len()];
        let mut f64_locals = [0.0; TYPED_LOOP_SLOT_CAP];
        let mut i64_locals = [0_i64; TYPED_LOOP_SLOT_CAP];
        let mut f64_init = [false; TYPED_LOOP_SLOT_CAP];
        let mut i64_init = [false; TYPED_LOOP_SLOT_CAP];

        for (local, slot) in block.array_slots.iter().enumerate() {
            if slot.live_in {
                let Some(value) = load_array_slot(frame, slot.slot) else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                if !typed_loop_array_guard(&value) {
                    return Ok(ExecutableBlockResult::NotExecuted);
                }
                array_locals[local] = Some(value);
                array_init[local] = true;
            }
        }
        for (local, slot) in block.f64_slots.iter().enumerate() {
            if slot.live_in {
                let Some(value) = load_f64_slot(frame, slot.slot) else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                f64_locals[local] = value;
                f64_init[local] = true;
            }
        }
        for (local, slot) in block.i64_slots.iter().enumerate() {
            if slot.live_in {
                let Some(value) = load_i64_slot(frame, slot.slot) else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                i64_locals[local] = value;
                i64_init[local] = true;
            }
        }

        profiler::record_event("ExecutableBlock::TypedLoop");

        let mut f64_stack = [0.0; TYPED_LOOP_STACK_CAP];
        let mut i64_stack = [0_i64; TYPED_LOOP_STACK_CAP];
        let mut bool_stack = [false; TYPED_LOOP_STACK_CAP];

        'loop_body: loop {
            let mut array_stack: Vec<ArrayRef> = Vec::with_capacity(TYPED_LOOP_STACK_CAP);
            let mut f64_sp = 0usize;
            let mut i64_sp = 0usize;
            let mut bool_sp = 0usize;
            let mut op_pc = 0usize;

            macro_rules! jump_to {
                ($target:expr) => {
                    match $target {
                        TypedLoopTarget::Exit => break 'loop_body,
                        TypedLoopTarget::LoopBack => continue 'loop_body,
                        TypedLoopTarget::Op(target) => {
                            if target >= block.ops.len() {
                                return Ok(ExecutableBlockResult::NotExecuted);
                            }
                            op_pc = target;
                            continue;
                        }
                    }
                };
            }

            while op_pc < block.ops.len() {
                let op = &block.ops[op_pc];
                match *op {
                    TypedLoopOp::LoadArraySlot(local) => {
                        if !array_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(value) = array_locals[local].as_ref().cloned() else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_array_stack(&mut array_stack, value)?;
                    }
                    TypedLoopOp::IndexStoreI64 => {
                        let Some(value) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        let Some(index) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        let Some(array) = pop_array_stack(&mut array_stack) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        typed_loop_index_store(
                            &array,
                            index,
                            Value::I64(value),
                            ArrayElementType::I64,
                        )?;
                        push_array_stack(&mut array_stack, array)?;
                    }
                    TypedLoopOp::IndexStoreF64 => {
                        let Some(value) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        let Some(index) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        let Some(array) = pop_array_stack(&mut array_stack) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        typed_loop_index_store(
                            &array,
                            index,
                            Value::F64(value),
                            ArrayElementType::F64,
                        )?;
                        push_array_stack(&mut array_stack, array)?;
                    }
                    TypedLoopOp::PushF64(value) => {
                        push_f64_stack(&mut f64_stack, &mut f64_sp, value);
                    }
                    TypedLoopOp::RandF64 => {
                        push_f64_stack(&mut f64_stack, &mut f64_sp, self.rng.next_f64());
                    }
                    TypedLoopOp::DupF64 => {
                        let Some(value) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, value);
                        push_f64_stack(&mut f64_stack, &mut f64_sp, value);
                    }
                    TypedLoopOp::LoadF64Slot(local) => {
                        if !f64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        push_f64_stack(&mut f64_stack, &mut f64_sp, f64_locals[local]);
                    }
                    TypedLoopOp::StoreF64Slot(local) => {
                        let Some(value) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        f64_locals[local] = value;
                        f64_init[local] = true;
                    }
                    TypedLoopOp::LoadSquareF64Slot(local) => {
                        if !f64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let value = f64_locals[local];
                        push_f64_stack(&mut f64_stack, &mut f64_sp, value * value);
                    }
                    TypedLoopOp::LoadAddF64Slot(local) => {
                        if !f64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs + f64_locals[local]);
                    }
                    TypedLoopOp::LoadSubF64Slot(local) => {
                        if !f64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs - f64_locals[local]);
                    }
                    TypedLoopOp::LoadMulF64Slot(local) => {
                        if !f64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs * f64_locals[local]);
                    }
                    TypedLoopOp::AddF64 => {
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs + rhs);
                    }
                    TypedLoopOp::SubF64 => {
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs - rhs);
                    }
                    TypedLoopOp::MulF64 => {
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs * rhs);
                    }
                    TypedLoopOp::DivF64 => {
                        // IEEE 754 division (matches the interpreter's `DivF64`):
                        // x/0.0 = ±Inf, 0.0/0.0 = NaN — no bail needed.
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs / rhs);
                    }
                    TypedLoopOp::LoadDivF64Slot(local) => {
                        if !f64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, lhs / f64_locals[local]);
                    }
                    TypedLoopOp::NegF64 => {
                        let Some(value) = pop_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, -value);
                    }
                    TypedLoopOp::PushI64(value) => {
                        push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::DupI64 => {
                        let Some(value) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                        push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::ToF64 => {
                        let Some(value) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_f64_stack(&mut f64_stack, &mut f64_sp, value as f64);
                    }
                    TypedLoopOp::LoadI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        push_i64_stack(&mut i64_stack, &mut i64_sp, i64_locals[local]);
                    }
                    TypedLoopOp::LoadI64SlotToF64(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        push_f64_stack(&mut f64_stack, &mut f64_sp, i64_locals[local] as f64);
                    }
                    TypedLoopOp::StoreI64Slot(local) => {
                        let Some(value) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        i64_locals[local] = value;
                        i64_init[local] = true;
                    }
                    TypedLoopOp::AddI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_add(rhs));
                    }
                    TypedLoopOp::SubI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_sub(rhs));
                    }
                    TypedLoopOp::MulI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(&mut i64_stack, &mut i64_sp, lhs.wrapping_mul(rhs));
                    }
                    TypedLoopOp::ModI64 => {
                        let Some((lhs, rhs)) = pop2_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        // Bail to the interpreter on the cases it would raise on /
                        // wrap (`rhs == 0`, `i64::MIN % -1`); the frame is untouched
                        // mid-block, so re-running from the header is correct.
                        let Some(value) = checked_i64_rem(lhs, rhs) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::LoadAddI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_add(i64_locals[local]),
                        );
                    }
                    TypedLoopOp::LoadSubI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_sub(i64_locals[local]),
                        );
                    }
                    TypedLoopOp::LoadMulI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(
                            &mut i64_stack,
                            &mut i64_sp,
                            lhs.wrapping_mul(i64_locals[local]),
                        );
                    }
                    TypedLoopOp::LoadModI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(lhs) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        let Some(value) = checked_i64_rem(lhs, i64_locals[local]) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_i64_stack(&mut i64_stack, &mut i64_sp, value);
                    }
                    TypedLoopOp::IncI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(delta) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        i64_locals[local] = i64_locals[local].wrapping_add(delta);
                    }
                    TypedLoopOp::DecI64Slot(local) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        let Some(delta) = pop_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        i64_locals[local] = i64_locals[local].wrapping_sub(delta);
                    }
                    TypedLoopOp::AddConstI64Slot(local, delta) => {
                        if !i64_init[local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        i64_locals[local] = i64_locals[local].wrapping_add(delta);
                    }
                    TypedLoopOp::CmpI64(relation) => {
                        let Some((lhs, rhs)) = pop2_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_bool_stack(
                            &mut bool_stack,
                            &mut bool_sp,
                            eval_i64_relation(lhs, rhs, relation),
                        );
                    }
                    TypedLoopOp::CmpF64(relation) => {
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        push_bool_stack(
                            &mut bool_stack,
                            &mut bool_sp,
                            eval_f64_relation(lhs, rhs, relation),
                        );
                    }
                    TypedLoopOp::JumpIfZero(target) => {
                        let Some(cond) = pop_bool_stack(&bool_stack, &mut bool_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        if !cond {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfI64(relation, target) => {
                        let Some((lhs, rhs)) = pop2_i64_stack(&i64_stack, &mut i64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        if eval_i64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfI64Slots(lhs_local, rhs_local, relation, target) => {
                        if !i64_init[lhs_local] || !i64_init[rhs_local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        if eval_i64_relation(i64_locals[lhs_local], i64_locals[rhs_local], relation)
                        {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::AddConstI64SlotAndJumpIfLe(local, delta, stop_local, target) => {
                        if !i64_init[local] || !i64_init[stop_local] {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        }
                        i64_locals[local] = i64_locals[local].wrapping_add(delta);
                        if i64_locals[local] <= i64_locals[stop_local] {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfF64(relation, target) => {
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        if eval_f64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::JumpIfNotF64(relation, target) => {
                        let Some((lhs, rhs)) = pop2_f64_stack(&f64_stack, &mut f64_sp) else {
                            return Ok(ExecutableBlockResult::NotExecuted);
                        };
                        if !eval_ordered_f64_relation(lhs, rhs, relation) {
                            jump_to!(target);
                        }
                    }
                    TypedLoopOp::Jump(target) => jump_to!(target),
                }
                op_pc += 1;
            }
            break;
        }

        for (local, slot) in block.array_slots.iter().enumerate() {
            if array_init[local] {
                let Some(value) = array_locals[local].as_ref().cloned() else {
                    return Ok(ExecutableBlockResult::NotExecuted);
                };
                let _ = frame.set_slot_array(slot.slot, value);
            }
        }
        for (local, slot) in block.f64_slots.iter().enumerate() {
            if f64_init[local] {
                let _ = frame.set_slot_f64(slot.slot, f64_locals[local]);
            }
        }
        for (local, slot) in block.i64_slots.iter().enumerate() {
            if i64_init[local] {
                let _ = frame.set_slot_i64(slot.slot, i64_locals[local]);
            }
        }

        self.ip = block.exit_ip;
        Ok(ExecutableBlockResult::Continue)
    }

    fn execute_complex_f64_mandelbrot_escape_loop_block(
        &mut self,
        block: &ComplexF64MandelbrotEscapeLoopBlock,
    ) -> Result<ExecutableBlockResult, super::VmError> {
        let Some(frame) = self.frames.last() else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        let Some((cr, ci)) = load_complex_f64_slot(frame, &self.struct_heap, block.c_slot) else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        let Some((mut zr, mut zi)) = load_complex_f64_slot(frame, &self.struct_heap, block.z_slot)
        else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        let Some(maxiter) = load_i64_slot(frame, block.maxiter_slot) else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };
        let Some(mut k) = load_i64_slot(frame, block.k_slot) else {
            return Ok(ExecutableBlockResult::NotExecuted);
        };

        profiler::record_event("ExecutableBlock::ComplexF64MandelbrotEscapeLoop");

        while k <= maxiter {
            if zr * zr + zi * zi > 4.0 {
                return self.route_executable_value_return(Value::I64(k));
            }
            let next_zr = zr * zr - zi * zi + cr;
            let next_zi = 2.0 * zr * zi + ci;
            zr = next_zr;
            zi = next_zi;
            k = k.wrapping_add(1);
        }

        self.route_executable_value_return(Value::I64(maxiter))
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

fn load_complex_f64_slot(
    frame: &super::frame::Frame,
    heap: &[super::StructInstance],
    slot: usize,
) -> Option<(f64, f64)> {
    match frame.locals_slots.get(slot)?.as_ref()? {
        Value::StructRef(idx) => heap.get(*idx)?.as_complex_parts(),
        Value::Struct(value) => value.as_complex_parts(),
        _ => None,
    }
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

fn typed_loop_array_guard(array: &ArrayRef) -> bool {
    let borrow = array.borrow();
    borrow.shape.len() == 1
        && matches!(
            borrow.element_type(),
            ArrayElementType::I64 | ArrayElementType::F64
        )
}

fn typed_loop_index_store(
    array: &ArrayRef,
    index: i64,
    value: Value,
    expected_element_type: ArrayElementType,
) -> Result<(), super::VmError> {
    let mut borrow = array.borrow_mut();
    if borrow.shape.len() != 1 || borrow.element_type() != expected_element_type {
        return Err(super::VmError::TypeError(
            "typed loop IndexStore shape guard failed".to_string(),
        ));
    }
    borrow.set(&[index], value)
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

fn push_f64_stack(stack: &mut [f64; TYPED_LOOP_STACK_CAP], sp: &mut usize, value: f64) {
    debug_assert!(*sp < TYPED_LOOP_STACK_CAP);
    stack[*sp] = value;
    *sp += 1;
}

fn pop_f64_stack(stack: &[f64; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<f64> {
    if *sp == 0 {
        return None;
    }
    *sp -= 1;
    Some(stack[*sp])
}

fn pop2_f64_stack(stack: &[f64; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<(f64, f64)> {
    let rhs = pop_f64_stack(stack, sp)?;
    let lhs = pop_f64_stack(stack, sp)?;
    Some((lhs, rhs))
}

fn push_i64_stack(stack: &mut [i64; TYPED_LOOP_STACK_CAP], sp: &mut usize, value: i64) {
    debug_assert!(*sp < TYPED_LOOP_STACK_CAP);
    stack[*sp] = value;
    *sp += 1;
}

fn pop_i64_stack(stack: &[i64; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<i64> {
    if *sp == 0 {
        return None;
    }
    *sp -= 1;
    Some(stack[*sp])
}

fn pop2_i64_stack(stack: &[i64; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<(i64, i64)> {
    let rhs = pop_i64_stack(stack, sp)?;
    let lhs = pop_i64_stack(stack, sp)?;
    Some((lhs, rhs))
}

fn push_bool_stack(stack: &mut [bool; TYPED_LOOP_STACK_CAP], sp: &mut usize, value: bool) {
    debug_assert!(*sp < TYPED_LOOP_STACK_CAP);
    stack[*sp] = value;
    *sp += 1;
}

fn pop_bool_stack(stack: &[bool; TYPED_LOOP_STACK_CAP], sp: &mut usize) -> Option<bool> {
    if *sp == 0 {
        return None;
    }
    *sp -= 1;
    Some(stack[*sp])
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

fn sync_euclidean_modulo_i64_slots(
    frame: &mut super::frame::Frame,
    block: &EuclideanModuloI64LoopBlock,
    a: i64,
    b: i64,
    tmp: Option<i64>,
) {
    let _ = frame.set_slot_i64(block.a_slot, a);
    let _ = frame.set_slot_i64(block.b_slot, b);
    if let Some(tmp) = tmp {
        let _ = frame.set_slot_i64(block.tmp_slot, tmp);
    }
}

#[cfg(test)]
mod tests {
    use crate::compile::compile_with_cache;
    use crate::lowering::Lowering;
    use crate::parser::Parser;
    use crate::rng::StableRng;
    use crate::vm::value::Value;

    use super::*;

    fn compile_source(src: &str) -> super::super::CompiledProgram {
        let mut parser = Parser::new().expect("parser");
        let outcome = parser.parse(src).expect("parse");
        let mut lowering = Lowering::new(src);
        let program = lowering.lower(outcome).expect("lower");
        compile_with_cache(&program).expect("compile")
    }

    #[test]
    fn loop_recognizer_registry_is_the_predecode_pipeline() {
        // Issue #6829: predecode is driven by the ordered `LOOP_RECOGNIZERS`
        // registry, so adding a new optimized shape is a registry append, not a
        // predecode-control-flow edit. Every registered recognizer follows the
        // same `(code, ip, end) -> Option<ExecutableBlock>` match/validate/build
        // contract. Here we confirm the registry exists and that a known shape
        // is recognized *through* it (the gcd loop -> a euclidean block).
        assert_eq!(LOOP_RECOGNIZERS.len(), 3);
        let gcd = compile_source(
            "function g(a, b)\n    while b != 0\n        t = b\n        b = a % b\n        a = t\n    end\n    a\nend\n\ng(48, 18)\n",
        );
        let matched = (0..gcd.code.len()).any(|ip| {
            LOOP_RECOGNIZERS
                .iter()
                .any(|recognize| recognize(&gcd.code, ip, gcd.code.len()).is_some())
        });
        assert!(matched, "gcd loop should be recognized via the registry");
    }

    #[test]
    fn predecodes_euclidean_modulo_i64_loop_pattern() {
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
        let executable = ExecutableProgram::from_bytecode(&compiled.code, &compiled.functions);
        assert!(executable.len() >= 1);
    }

    #[test]
    fn euclidean_modulo_i64_loop_block_executes_and_returns_expected_value() {
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
    fn euclidean_modulo_i64_loop_block_handles_zero_second_operand() {
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
        let executable = ExecutableProgram::from_bytecode(&compiled.code, &compiled.functions);
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
        let executable = ExecutableProgram::from_bytecode(&compiled.code, &compiled.functions);
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
        let block = try_predecode_typed_loop_range(&code, 0, code.len(), &mut reject);
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
        let block = try_predecode_typed_loop_range(&code, 0, MAX_TYPED_LOOP_OPS + 2, &mut reject);
        assert!(block.is_none());
        assert!(matches!(reject, Some(TypedLoopReject::OpCountOverCap)));
    }

    #[test]
    fn typed_loop_reject_reason_no_exit_issue_8193() {
        // A balanced body whose only branch loops back to the header (no branch
        // leaves the loop) is reported as `NoExit`.
        let code = vec![Instr::Jump(0)];
        let mut reject = None;
        let block = try_predecode_typed_loop_range(&code, 0, code.len(), &mut reject);
        assert!(block.is_none());
        assert!(matches!(reject, Some(TypedLoopReject::NoExit)));
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
                    if try_predecode_typed_loop_range(code, header, jump_ip + 1, &mut reject)
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
        let executable = ExecutableProgram::from_bytecode(&compiled.code, &compiled.functions);
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
        let executable = ExecutableProgram::from_bytecode(&compiled.code, &compiled.functions);
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
    fn complex_mandelbrot_escape_runtime_specialization_adds_executable_block_6253() {
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
        assert!(!vm.executable.has_complex_f64_mandelbrot_escape_loop());
        let result = vm.run().expect("run");
        assert!(
            vm.executable.has_complex_f64_mandelbrot_escape_loop(),
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
        use crate::compile::typed_scalar_binary_instr;
        use crate::ir::core::BinaryOp;

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
}
