//! Side-by-side register VM prototype for Issue #8448 / #8558.
//!
//! This module lowers *per-function* stack bytecode into an in-memory register
//! form and interprets it with a portable `match` loop, without changing the
//! production stack VM. The Issue #8558 slice grows the original straight-line
//! `Int64` subset (PR #8528) to real compiled fixtures: conditional and
//! unconditional jumps, typed slot loads/stores, `Float64` arithmetic,
//! comparisons, and function calls.
//!
//! # Register allocation
//!
//! Registers are identified with operand-stack depths: the value a stack
//! instruction would leave at depth `d` lives in register `d`. This makes the
//! lowering total at control-flow merge points (both branch arms leave their
//! result in the same register) and lets the required register count be
//! computed statically per function as the maximum operand depth. Local slots
//! keep their own storage (`slots`), mirroring the stack VM frame; boxed and
//! heap-owned values stay by-reference inside `Value`.
//!
//! # Call boundary (Issues #8558 / #9904)
//!
//! `RegisterInstr::CallStack` first asks the [`RegisterVmHost`] for a
//! register-native callee frame. When the callee has a translatable shared plan,
//! the interpreter pushes that frame onto its own explicit frame stack and
//! continues without re-entering the stack VM or recursing on the host Rust
//! stack. Untranslatable callees still trampoline through the stack VM host.
//! `RegisterInstr::CallIntrinsic` remains a stack-host operand operation.
//!
//! # Totality
//!
//! Translation is total-or-explicit: any stack instruction outside the covered
//! subset makes `from_stack_function` / `from_stack_program` return `Err`
//! naming the instruction. There is no silent per-instruction fallback inside
//! a function; the caller (the `SJULIA_REGISTER_VM=1` gate) keeps whole
//! ineligible functions on the stack VM. The coverage list lives in
//! `docs/vm/REGISTER_VM.md`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ir::core::{BinaryOp, Expr, Literal, NumericConvertTarget, UnaryOp};
use crate::rng::RngLike;
use subset_julia_vm_bytecode::shared_plan::{
    SharedBlockPlan, SharedCopyPlan, SharedFunctionPlan, SharedRootPlan, SharedTermPlan,
};
use subset_julia_vm_bytecode::{
    CompiledProgram, FunctionInfo, I64Cmp, Instr, Intrinsic, Value, VarTypeTag, VmError,
};

/// Static per-function metrics reported for the #8559 measurement matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterVmMetrics {
    /// In-memory register bytecode size (`instructions.len() * stride`).
    pub bytecode_bytes: usize,
    /// Static instruction count. For straight-line programs this equals the
    /// dynamic dispatch count; for loops/calls use
    /// [`RegisterVm::dispatch_count`] after a run.
    pub dispatch_count: usize,
    /// Statically computed operand register count (max operand-stack depth).
    pub frame_registers: usize,
    /// Local slot storage entries carried alongside the registers.
    pub frame_slots: usize,
}

/// Integer binary ops (`i64`, wrapping, `%` = truncated remainder).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I64BinOp {
    Add,
    Sub,
    Mul,
    Rem,
}

/// Float binary ops (IEEE 754 double).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F64BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// Comparison predicates shared by the I64/F64 compare instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl CmpOp {
    fn eval_i64(self, a: i64, b: i64) -> bool {
        match self {
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
        }
    }

    fn negated(self) -> Self {
        match self {
            CmpOp::Lt => CmpOp::Ge,
            CmpOp::Le => CmpOp::Gt,
            CmpOp::Gt => CmpOp::Le,
            CmpOp::Ge => CmpOp::Lt,
            CmpOp::Eq => CmpOp::Ne,
            CmpOp::Ne => CmpOp::Eq,
        }
    }

    fn eval_f64(self, a: f64, b: f64) -> bool {
        match self {
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
            CmpOp::Eq => a == b,
            CmpOp::Ne => a != b,
        }
    }
}

/// Fused F64 branch predicates. The `Not*` forms encode the exact false branch
/// of the ordered comparison (NaN-correct), mirroring the stack VM's
/// `JumpIfNot*F64` instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F64BranchOp {
    Eq,
    Ne,
    NotLt,
    NotGt,
    NotLe,
    NotGe,
}

impl F64BranchOp {
    fn should_jump(self, a: f64, b: f64) -> bool {
        use std::cmp::Ordering;
        match self {
            F64BranchOp::Eq => a == b,
            F64BranchOp::Ne => a != b,
            F64BranchOp::NotLt => !matches!(a.partial_cmp(&b), Some(Ordering::Less)),
            F64BranchOp::NotGt => !matches!(a.partial_cmp(&b), Some(Ordering::Greater)),
            F64BranchOp::NotLe => {
                !matches!(a.partial_cmp(&b), Some(Ordering::Less | Ordering::Equal))
            }
            F64BranchOp::NotGe => {
                !matches!(a.partial_cmp(&b), Some(Ordering::Greater | Ordering::Equal))
            }
        }
    }
}

/// How a `Return` instruction validates/normalizes the returned register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKind {
    /// `ReturnI64` parity: integer-family values are returned preserved
    /// (narrow ints stay narrow), anything else is an internal error.
    I64,
    /// `ReturnF64` parity: numeric values are converted to `Value::F64`.
    F64,
    /// `ReturnAny` parity: the register value is returned as-is.
    Any,
    /// `ReturnNothing` parity: returns `Value::Nothing` without reading a
    /// register.
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegisterInstr {
    ConstI64 {
        dst: usize,
        value: i64,
    },
    ConstF64 {
        dst: usize,
        value: f64,
    },
    ConstBool {
        dst: usize,
        value: bool,
    },
    ConstNothing {
        dst: usize,
    },

    /// Stack `LoadSlotI64` parity: numeric slot values are loaded unwidened
    /// (a narrow int/Bool/float passes through); an unset slot raises
    /// `UndefVarError`, a non-numeric slot is an internal error.
    LoadSlotI64 {
        dst: usize,
        slot: usize,
    },
    /// Stack `StoreSlotI64` parity: the register is converted with `pop_i64`
    /// rules and stored as `Value::I64`.
    StoreSlotI64 {
        slot: usize,
        src: usize,
    },
    /// Stack `LoadSlotF64` parity: `F64` loads directly, `F16`/`F32` pass
    /// through unchanged, integer-family values widen to `F64`.
    LoadSlotF64 {
        dst: usize,
        slot: usize,
    },
    /// Stack `StoreSlotF64` parity: converted with `pop_f64_or_i64` rules and
    /// stored as `Value::F64`.
    StoreSlotF64 {
        slot: usize,
        src: usize,
    },
    /// Stack `LoadSlotI64ToF64` parity: numeric slot value converted to `F64`.
    LoadSlotI64ToF64 {
        dst: usize,
        slot: usize,
    },
    /// Stack `LoadSlot` parity: any slot value is cloned into the register;
    /// an unset slot raises `UndefVarError`.
    LoadSlotAny {
        dst: usize,
        slot: usize,
    },
    /// Stack `StoreSlot` parity: stores the register value after the host's
    /// slot-storage normalization (mutable structs move to the heap).
    StoreSlotAny {
        slot: usize,
        src: usize,
    },

    BinI64 {
        op: I64BinOp,
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    NegI64 {
        dst: usize,
        src: usize,
    },
    /// Wrapping `src + value` (from `IncI64` / unfused `LoadAddConstI64Slot`).
    AddConstI64 {
        dst: usize,
        src: usize,
        value: i64,
    },
    /// Stack `AddConstI64Slot` parity: strict in-place `slot += delta` on an
    /// exact `I64` slot.
    AddConstI64Slot {
        slot: usize,
        delta: i64,
    },
    BinF64 {
        op: F64BinOp,
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    /// Fused `Load{Add,Sub,Mul,Div}F64Slot` parity: `dst = src <op> slot`.
    BinF64Slot {
        op: F64BinOp,
        dst: usize,
        src: usize,
        slot: usize,
    },
    /// Fused Float64 slot operation: `dst = lhs_slot <op> rhs_slot`.
    BinF64Slots {
        op: F64BinOp,
        dst: usize,
        lhs_slot: usize,
        rhs_slot: usize,
    },
    /// Fused Float64 literal RHS: `dst = src <op> value`.
    BinF64Const {
        op: F64BinOp,
        dst: usize,
        src: usize,
        value: f64,
    },
    /// Fused Float64 literal LHS: `dst = value <op> src`.
    BinF64ConstLeft {
        op: F64BinOp,
        dst: usize,
        value: f64,
        src: usize,
    },
    /// Fused Float64 literal/slot operation. `const_on_left` selects
    /// `value <op> slot`; otherwise this is `slot <op> value`.
    BinF64SlotConst {
        op: F64BinOp,
        dst: usize,
        slot: usize,
        value: f64,
        const_on_left: bool,
    },
    /// Fused Float64-family assignment: `slot = lhs <op> rhs`.
    BinF64StoreSlot {
        op: F64BinOp,
        slot: usize,
        lhs: usize,
        rhs: usize,
    },
    /// Fused Float64-family assignment: `slot = value <op> rhs`.
    BinF64ConstLeftStoreSlot {
        op: F64BinOp,
        slot: usize,
        value: f64,
        rhs: usize,
    },
    /// Fused Float64-family assignment:
    /// `slot = value <outer_op> (lhs_slot <inner_op> rhs_slot)`.
    BinF64ConstLeftBinSlotsStoreSlot {
        outer_op: F64BinOp,
        inner_op: F64BinOp,
        slot: usize,
        value: f64,
        lhs_slot: usize,
        rhs_slot: usize,
    },
    NegF64 {
        dst: usize,
        src: usize,
    },
    /// In-place `slot = -slot` for exact Float64-family shared-plan locals.
    NegF64Slot {
        slot: usize,
    },

    /// I64 comparison producing `Value::Bool` (stack `GtI64`-family parity).
    CmpI64 {
        op: CmpOp,
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    /// F64 comparison producing `Value::Bool` (stack `LtF64`-family parity).
    CmpF64 {
        op: CmpOp,
        dst: usize,
        lhs: usize,
        rhs: usize,
    },

    /// Stack `ToF64` parity.
    I64ToF64 {
        dst: usize,
        src: usize,
    },
    /// Stack `ToI64` parity (truncating float → int).
    F64ToI64 {
        dst: usize,
        src: usize,
    },
    BoolToI64 {
        dst: usize,
        src: usize,
    },
    I64ToBool {
        dst: usize,
        src: usize,
    },
    NotBool {
        dst: usize,
        src: usize,
    },
    /// Register copy (stack `Dup`/`DupI64`/`DupF64`).
    Move {
        dst: usize,
        src: usize,
    },

    Jump {
        target: usize,
    },
    /// Stack `JumpIfZero` parity: requires `Value::Bool` (raises the same
    /// "non-boolean used in boolean context" `TypeError` otherwise) and jumps
    /// when false.
    JumpIfFalse {
        src: usize,
        target: usize,
    },
    /// Fused I64 compare-and-branch (stack `JumpIf{Eq,Ne,Lt,Gt,Le,Ge}I64`).
    BranchCmpI64 {
        op: CmpOp,
        lhs: usize,
        rhs: usize,
        target: usize,
    },
    /// Fused F64 compare-and-branch (stack `JumpIf{Eq,Ne}F64` /
    /// `JumpIfNot{Lt,Gt,Le,Ge}F64`).
    BranchCmpF64 {
        op: F64BranchOp,
        lhs: usize,
        rhs: usize,
        target: usize,
    },
    /// Stack `JumpIfGtI64Slots` parity: compare two I64 slots directly and
    /// jump when `lhs > rhs` (integer-family slot values widen).
    BranchGtI64Slots {
        lhs_slot: usize,
        rhs_slot: usize,
        target: usize,
    },
    /// Shared-plan direct slot compare-and-branch for I64 while conditions.
    BranchCmpI64Slots {
        op: CmpOp,
        lhs_slot: usize,
        rhs_slot: usize,
        target: usize,
    },
    /// Stack `AddConstI64SlotAndJumpIfLe` parity: `slot += delta` in place,
    /// jump while `slot <= stop_slot`.
    AddConstI64SlotBranchLe {
        slot: usize,
        delta: i64,
        stop_slot: usize,
        target: usize,
    },
    /// Shared-plan typed loop block. The validated loop body lives in the
    /// program's block table; the instruction stays copyable for dispatch.
    LoopI64Slots {
        block: usize,
    },

    /// Direct function call. Arguments live in `arg_count` consecutive
    /// registers starting at `args_start`; the result lands in `dst`.
    /// Translatable callees run as register-native frames, and other callees
    /// fall back through the stack VM host.
    CallStack {
        func_index: usize,
        args_start: usize,
        arg_count: usize,
        dst: usize,
        inbounds: bool,
    },
    /// Core intrinsic call trampolined into the stack VM host.
    CallIntrinsic {
        intrinsic: Intrinsic,
        args_start: usize,
        arg_count: usize,
        dst: usize,
    },

    Return {
        kind: ReturnKind,
        src: usize,
    },
}

/// Host interface the register interpreter uses to prepare nested register
/// calls, reach stack VM fallback services, and perform the rare non-pure value
/// conversions that need the struct heap.
pub trait RegisterVmHost {
    /// Prepare a register-native callee frame, if the host can translate and
    /// bind this direct call without returning to the stack VM.
    fn prepare_register_call_frame(
        &mut self,
        _func_index: usize,
        _args: &[Value],
        _inbounds: bool,
    ) -> Result<Option<RegisterCallFrame>, VmError> {
        Ok(None)
    }

    /// Run a direct function call to completion on the stack VM and hand back
    /// its return value.
    fn call_function(
        &mut self,
        func_index: usize,
        args: Vec<Value>,
        inbounds: bool,
    ) -> Result<Value, VmError>;

    /// Run a core intrinsic on the stack VM's operand stack.
    fn call_intrinsic(&mut self, intrinsic: Intrinsic, args: Vec<Value>) -> Result<Value, VmError>;

    /// `pop_f64_or_i64` parity for values the pure conversion cannot handle
    /// (BigInt, Rational/Irrational structs, ...).
    fn value_to_f64_slow(&mut self, value: &Value) -> Result<f64, VmError> {
        Err(VmError::TypeError(format!(
            "expected F64-convertible value, got {:?}",
            value
        )))
    }

    /// Stack `StoreSlot` storage normalization (mutable structs move to the
    /// struct heap). Defaults to identity for hosts without a heap.
    fn normalize_for_slot_storage(&mut self, value: Value) -> Value {
        value
    }

    /// Type name used by the boolean-context `TypeError` message, matching
    /// the stack VM's `expect_bool` diagnostics.
    fn bool_context_type_name(&self, value: &Value) -> String {
        format!("{:?}", value.runtime_type())
    }
}

/// A callee frame the host has proven can run on register bytecode.
pub struct RegisterCallFrame {
    pub program: Rc<RegisterProgram>,
    pub slots: Vec<Option<Value>>,
}

/// Host for standalone register programs (unit tests, straight-line
/// fixtures): any call escaping to the stack VM is an explicit error.
pub struct NoStackHost;

impl RegisterVmHost for NoStackHost {
    fn call_function(
        &mut self,
        func_index: usize,
        _args: Vec<Value>,
        _inbounds: bool,
    ) -> Result<Value, VmError> {
        Err(VmError::InternalError(format!(
            "register VM prototype: call to function {func_index} requires a stack VM host"
        )))
    }

    fn call_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        _args: Vec<Value>,
    ) -> Result<Value, VmError> {
        Err(VmError::InternalError(format!(
            "register VM prototype: intrinsic {intrinsic:?} requires a stack VM host"
        )))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegisterProgram {
    instructions: Vec<RegisterInstr>,
    loop_blocks: Vec<RegisterLoopBlock>,
    frame_registers: usize,
    slot_count: usize,
    slot_names: Rc<Vec<String>>,
    name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct RegisterLoopBlock {
    exit_op: CmpOp,
    lhs_i64_index: usize,
    rhs_i64_index: usize,
    exit_pc: usize,
    f64_slots: Vec<usize>,
    f64_live_in: Vec<bool>,
    i64_slots: Vec<usize>,
    ops: Vec<RegisterLoopOp>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum LoopF64Source {
    F64Slot(usize),
    I64Slot(usize),
}

/// Statically-known numeric kind of an expression, used by
/// [`from_shared_plan_with_context`]'s expr lowering to decide whether a
/// structural [`Expr::Convert`] node (Issue #9803) can lower to a native
/// register conversion. Deliberately narrower than `ValueType`: only the two
/// kinds a shared-plan `Expr::Convert` can safely target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisterNumericKind {
    I64,
    F64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RegisterLoopOp {
    LoadSlotF64 {
        dst: usize,
        source: LoopF64Source,
    },
    StoreSlotF64 {
        slot_index: usize,
        src: usize,
    },
    BinF64 {
        op: F64BinOp,
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    BinF64Const {
        op: F64BinOp,
        dst: usize,
        src: usize,
        value: f64,
    },
    BinF64Slots {
        op: F64BinOp,
        dst: usize,
        lhs: LoopF64Source,
        rhs: LoopF64Source,
    },
    BinF64SlotConst {
        op: F64BinOp,
        dst: usize,
        source: LoopF64Source,
        value: f64,
        const_on_left: bool,
    },
    BinF64StoreSlot {
        op: F64BinOp,
        slot_index: usize,
        lhs: usize,
        rhs: usize,
    },
    BinF64ConstLeftStoreSlot {
        op: F64BinOp,
        slot_index: usize,
        value: f64,
        rhs: usize,
    },
    BinF64ConstLeftBinSlotsStoreSlot {
        outer_op: F64BinOp,
        inner_op: F64BinOp,
        slot_index: usize,
        value: f64,
        lhs: LoopF64Source,
        rhs: LoopF64Source,
    },
    NegF64Slot {
        slot_index: usize,
        source: LoopF64Source,
    },
    AddConstI64Slot {
        slot_index: usize,
        delta: i64,
    },
    CopyF64Slot {
        slot_index: usize,
        source: LoopF64Source,
    },
    BinF64SourcesStoreSlot {
        op: F64BinOp,
        slot_index: usize,
        lhs: LoopF64Source,
        rhs: LoopF64Source,
    },
    BinF64SourceSlotConstStoreSlot {
        outer_op: F64BinOp,
        inner_op: F64BinOp,
        slot_index: usize,
        lhs: LoopF64Source,
        rhs: LoopF64Source,
        value: f64,
        const_on_left: bool,
    },
    BinF64SlotsSlotConstStoreSlot {
        outer_op: F64BinOp,
        slots_op: F64BinOp,
        slot_const_op: F64BinOp,
        slot_index: usize,
        lhs: LoopF64Source,
        rhs: LoopF64Source,
        slot_const_source: LoopF64Source,
        value: f64,
        const_on_left: bool,
    },
    BinF64SourceSlotConstSourceStoreSlot {
        outer_op: F64BinOp,
        middle_op: F64BinOp,
        inner_op: F64BinOp,
        slot_index: usize,
        lhs: LoopF64Source,
        inner_source: LoopF64Source,
        value: f64,
        inner_const_on_left: bool,
        tail: LoopF64Source,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RawRegisterLoopOp {
    LoadSlotF64 {
        dst: usize,
        slot: usize,
    },
    StoreSlotF64 {
        slot: usize,
        src: usize,
    },
    BinF64 {
        op: F64BinOp,
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    BinF64Const {
        op: F64BinOp,
        dst: usize,
        src: usize,
        value: f64,
    },
    BinF64Slots {
        op: F64BinOp,
        dst: usize,
        lhs_slot: usize,
        rhs_slot: usize,
    },
    BinF64SlotConst {
        op: F64BinOp,
        dst: usize,
        slot: usize,
        value: f64,
        const_on_left: bool,
    },
    BinF64StoreSlot {
        op: F64BinOp,
        slot: usize,
        lhs: usize,
        rhs: usize,
    },
    BinF64ConstLeftStoreSlot {
        op: F64BinOp,
        slot: usize,
        value: f64,
        rhs: usize,
    },
    BinF64ConstLeftBinSlotsStoreSlot {
        outer_op: F64BinOp,
        inner_op: F64BinOp,
        slot: usize,
        value: f64,
        lhs_slot: usize,
        rhs_slot: usize,
    },
    NegF64Slot {
        slot: usize,
    },
    AddConstI64Slot {
        slot: usize,
        delta: i64,
    },
}

impl RegisterProgram {
    /// Lower a whole compiled program's top-level region (`entry..end`).
    /// Kept for the PR #8528 straight-line entry point.
    pub fn from_stack_program(program: &CompiledProgram) -> Result<Self, String> {
        lower_region(
            &program.code,
            program.entry,
            program.code.len(),
            program.global_slot_count,
            Rc::new(program.global_slot_names.clone()),
            "<main>".to_string(),
        )
    }

    /// Lower one compiled function body (`entry..code_end`) from existing stack
    /// bytecode, using its local slot layout. Kept for standalone metrics and
    /// regression tests; the `SJULIA_REGISTER_VM=1` gate now consumes the
    /// shared SSA plan directly.
    pub fn from_stack_function(code: &[Instr], func: &FunctionInfo) -> Result<Self, String> {
        lower_region(
            code,
            func.entry,
            func.code_end,
            func.local_slot_count,
            Rc::new(func.slot_names.clone()),
            func.name.clone(),
        )
    }

    /// Lower a backend-neutral SSA shared plan directly into register bytecode.
    ///
    /// This is the Issue #9089 entry point: register lowering consumes the same
    /// control-flow/expression plan as stack lowering without translating from
    /// stack [`Instr`] first. Unsupported expression forms are explicit errors
    /// so callers can keep whole functions on the stack VM until the shared
    /// register subset grows.
    pub fn from_shared_plan(
        plan: &SharedFunctionPlan,
        slot_count: usize,
        slot_names: Rc<Vec<String>>,
        name: String,
    ) -> Result<Self, String> {
        let function_indices = HashMap::new();
        Self::from_shared_plan_with_context(
            plan,
            slot_count,
            slot_names,
            &[],
            name,
            &function_indices,
        )
    }

    pub fn from_shared_plan_with_functions(
        plan: &SharedFunctionPlan,
        slot_count: usize,
        slot_names: Rc<Vec<String>>,
        name: String,
        function_indices: &HashMap<String, Vec<usize>>,
    ) -> Result<Self, String> {
        Self::from_shared_plan_with_context(
            plan,
            slot_count,
            slot_names,
            &[],
            name,
            function_indices,
        )
    }

    pub fn from_shared_plan_with_context(
        plan: &SharedFunctionPlan,
        slot_count: usize,
        slot_names: Rc<Vec<String>>,
        slot_types: &[Option<VarTypeTag>],
        name: String,
        function_indices: &HashMap<String, Vec<usize>>,
    ) -> Result<Self, String> {
        SharedPlanLowering::new(
            slot_count,
            slot_names.as_ref().clone(),
            slot_types,
            name,
            function_indices,
        )
        .lower(plan)
    }

    pub fn instructions(&self) -> &[RegisterInstr] {
        &self.instructions
    }

    pub fn slot_count(&self) -> usize {
        self.slot_count
    }

    pub fn frame_registers(&self) -> usize {
        self.frame_registers
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn metrics(&self) -> RegisterVmMetrics {
        RegisterVmMetrics {
            bytecode_bytes: self.instructions.len() * std::mem::size_of::<RegisterInstr>(),
            dispatch_count: self.instructions.len(),
            frame_registers: self.frame_registers,
            frame_slots: self.slot_count,
        }
    }

    fn slot_name(&self, slot: usize) -> String {
        self.slot_names
            .get(slot)
            .cloned()
            .unwrap_or_else(|| format!("slot {slot}"))
    }
}

// ===================== Lowering =====================

struct SharedPlanLowering<'a> {
    instructions: Vec<RegisterInstr>,
    next_register: usize,
    max_registers: usize,
    slot_names: Vec<String>,
    slot_types: Vec<Option<VarTypeTag>>,
    slot_by_name: HashMap<String, usize>,
    function_indices: &'a HashMap<String, Vec<usize>>,
    name: String,
    block_pc: Vec<Option<usize>>,
    pending_jumps: Vec<(usize, u32)>,
}

impl<'a> SharedPlanLowering<'a> {
    fn new(
        slot_count: usize,
        mut slot_names: Vec<String>,
        slot_types: &[Option<VarTypeTag>],
        name: String,
        function_indices: &'a HashMap<String, Vec<usize>>,
    ) -> Self {
        slot_names.truncate(slot_count);
        while slot_names.len() < slot_count {
            slot_names.push(format!("slot {}", slot_names.len()));
        }
        let mut slot_types = slot_types.to_vec();
        slot_types.truncate(slot_count);
        slot_types.resize(slot_count, None);
        let slot_by_name = slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect();
        Self {
            instructions: Vec::new(),
            next_register: 0,
            max_registers: 0,
            slot_names,
            slot_types,
            slot_by_name,
            function_indices,
            name,
            block_pc: Vec::new(),
            pending_jumps: Vec::new(),
        }
    }

    fn lower(mut self, plan: &SharedFunctionPlan) -> Result<RegisterProgram, String> {
        self.block_pc = vec![None; plan.blocks().len()];
        for (block_idx, block) in plan.blocks().iter().enumerate() {
            self.block_pc[block_idx] = Some(self.instructions.len());
            self.lower_block(block)?;
        }
        self.patch_jumps()?;
        let loop_blocks = self.install_loop_blocks();
        Ok(RegisterProgram {
            instructions: self.instructions,
            loop_blocks,
            frame_registers: self.max_registers,
            slot_count: self.slot_names.len(),
            slot_names: Rc::new(self.slot_names),
            name: self.name,
        })
    }

    fn lower_block(&mut self, block: &SharedBlockPlan) -> Result<(), String> {
        for root in block.roots() {
            match root {
                SharedRootPlan::Assign { name, expr, .. } => {
                    self.lower_assignment(name, expr)?;
                }
                SharedRootPlan::Discard { expr, .. } => {
                    let _ = self.lower_expr(expr)?;
                }
            }
        }

        match block.terminator() {
            SharedTermPlan::Return { expr: Some(expr) } => {
                let src = self.lower_expr(expr)?;
                self.emit(RegisterInstr::Return {
                    kind: ReturnKind::Any,
                    src,
                });
            }
            SharedTermPlan::Return { expr: None } => {
                self.emit(RegisterInstr::Return {
                    kind: ReturnKind::Nothing,
                    src: 0,
                });
            }
            SharedTermPlan::Jump { target, copies } => {
                self.lower_copies(copies)?;
                self.emit_pending_jump(RegisterInstr::Jump { target: usize::MAX }, *target);
            }
            SharedTermPlan::Branch {
                cond,
                then_target,
                else_target,
                then_copies,
                else_copies,
            } => {
                let else_pc = self.emit_branch_to_else(cond)?;

                self.lower_copies(then_copies)?;
                self.emit_pending_jump(RegisterInstr::Jump { target: usize::MAX }, *then_target);

                let else_start = self.instructions.len();
                self.patch_single_jump(else_pc, else_start)?;
                self.lower_copies(else_copies)?;
                self.emit_pending_jump(RegisterInstr::Jump { target: usize::MAX }, *else_target);
            }
        }
        Ok(())
    }

    fn lower_copies(&mut self, copies: &[SharedCopyPlan]) -> Result<(), String> {
        for copy in copies {
            self.lower_assignment(&copy.name, &copy.expr)?;
        }
        Ok(())
    }

    fn lower_assignment(&mut self, name: &str, expr: &Expr) -> Result<(), String> {
        let slot = self.slot_for_write(name);
        if self.try_lower_assignment_fusion(slot, expr)? {
            return Ok(());
        }
        let src = self.lower_expr(expr)?;
        self.emit_store_slot(slot, src);
        Ok(())
    }

    fn lower_expr(&mut self, expr: &Expr) -> Result<usize, String> {
        match expr {
            Expr::Literal(literal, _) => self.lower_literal(literal),
            Expr::Var(name, _) => {
                let slot = self.slot_for_read(name)?;
                let dst = self.alloc_register();
                self.emit_load_slot(dst, slot);
                Ok(dst)
            }
            Expr::UnaryOp { op, operand, .. } => {
                let prefer_f64 = self.expr_prefers_f64(operand);
                let src = self.lower_expr(operand)?;
                match op {
                    UnaryOp::Pos => Ok(src),
                    UnaryOp::Neg => {
                        let dst = self.alloc_register();
                        if prefer_f64 {
                            self.emit(RegisterInstr::NegF64 { dst, src });
                        } else {
                            self.emit(RegisterInstr::NegI64 { dst, src });
                        }
                        Ok(dst)
                    }
                    UnaryOp::Not => {
                        let dst = self.alloc_register();
                        self.emit(RegisterInstr::NotBool { dst, src });
                        Ok(dst)
                    }
                }
            }
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                let prefer_f64 = *op == BinaryOp::Div
                    || self.expr_prefers_f64(left)
                    || self.expr_prefers_f64(right);
                match op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Mod => {
                        if prefer_f64 && *op != BinaryOp::Mod {
                            if let Some(fused) = self.lower_bin_f64_slots(*op, left, right) {
                                return Ok(fused);
                            }
                            if let Some(fused) = self.lower_bin_f64_const(*op, left, right)? {
                                return Ok(fused);
                            }
                            if let Some(fused) = self.lower_bin_f64_slot(*op, left, right)? {
                                return Ok(fused);
                            }
                            let lhs = self.lower_expr(left)?;
                            let rhs = self.lower_expr(right)?;
                            let dst = self.alloc_register();
                            let op = match op {
                                BinaryOp::Add => F64BinOp::Add,
                                BinaryOp::Sub => F64BinOp::Sub,
                                BinaryOp::Mul => F64BinOp::Mul,
                                _ => unreachable!(),
                            };
                            self.emit(RegisterInstr::BinF64 { op, dst, lhs, rhs });
                            Ok(dst)
                        } else {
                            let lhs = self.lower_expr(left)?;
                            let rhs = self.lower_expr(right)?;
                            let dst = self.alloc_register();
                            let op = match op {
                                BinaryOp::Add => I64BinOp::Add,
                                BinaryOp::Sub => I64BinOp::Sub,
                                BinaryOp::Mul => I64BinOp::Mul,
                                BinaryOp::Mod => I64BinOp::Rem,
                                _ => unreachable!(),
                            };
                            self.emit(RegisterInstr::BinI64 { op, dst, lhs, rhs });
                            Ok(dst)
                        }
                    }
                    BinaryOp::Div => {
                        if let Some(fused) = self.lower_bin_f64_slots(*op, left, right) {
                            return Ok(fused);
                        }
                        if let Some(fused) = self.lower_bin_f64_const(*op, left, right)? {
                            return Ok(fused);
                        }
                        if let Some(fused) = self.lower_bin_f64_slot(*op, left, right)? {
                            return Ok(fused);
                        }
                        let lhs = self.lower_expr(left)?;
                        let rhs = self.lower_expr(right)?;
                        let dst = self.alloc_register();
                        self.emit(RegisterInstr::BinF64 {
                            op: F64BinOp::Div,
                            dst,
                            lhs,
                            rhs,
                        });
                        Ok(dst)
                    }
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne => {
                        let op = match op {
                            BinaryOp::Lt => CmpOp::Lt,
                            BinaryOp::Le => CmpOp::Le,
                            BinaryOp::Gt => CmpOp::Gt,
                            BinaryOp::Ge => CmpOp::Ge,
                            BinaryOp::Eq => CmpOp::Eq,
                            BinaryOp::Ne => CmpOp::Ne,
                            _ => unreachable!(),
                        };
                        let lhs = self.lower_expr(left)?;
                        let rhs = self.lower_expr(right)?;
                        let dst = self.alloc_register();
                        if prefer_f64 {
                            self.emit(RegisterInstr::CmpF64 { op, dst, lhs, rhs });
                        } else {
                            self.emit(RegisterInstr::CmpI64 { op, dst, lhs, rhs });
                        }
                        Ok(dst)
                    }
                    other => Err(format!(
                        "register VM shared-plan lowering cannot lower binary op: {other:?}"
                    )),
                }
            }
            Expr::Call {
                function,
                args,
                kwargs,
                splat_mask,
                kwargs_splat_mask,
                ..
            } => self.lower_call(function, args, kwargs, splat_mask, kwargs_splat_mask),
            // Structural explicit numeric type-constructor call (Issue
            // #9803): only lower the shapes proven identical to the stack
            // path's `CallBuiltin(BuiltinId::Int64/Float64, 1)` semantics —
            // widening I64->F64 (rounds exactly, matches `Instr::ToF64`) and
            // both identity cases. `Int64(x)` on a statically F64 operand is
            // NOT lowered here: the stack builtin is a *checked* constructor
            // (raises `InexactError` on a non-integral float, matching
            // upstream `Int64(1.5)`) while the register `F64ToI64` op is a
            // truncating conversion — lowering it natively would silently
            // diverge from the stack semantics it must match. Any operand
            // whose numeric kind is not statically known (Any, Bool, other)
            // also falls back, leaving the whole function on the stack VM.
            Expr::Convert {
                target, operand, ..
            } => {
                let operand_kind = self.expr_numeric_kind(operand);
                let src = self.lower_expr(operand)?;
                match (target, operand_kind) {
                    (NumericConvertTarget::Float64, Some(RegisterNumericKind::F64))
                    | (NumericConvertTarget::Int64, Some(RegisterNumericKind::I64)) => Ok(src),
                    (NumericConvertTarget::Float64, Some(RegisterNumericKind::I64)) => {
                        let dst = self.alloc_register();
                        self.emit(RegisterInstr::I64ToF64 { dst, src });
                        Ok(dst)
                    }
                    (NumericConvertTarget::Int64, Some(RegisterNumericKind::F64)) => Err(
                        "register VM shared-plan lowering cannot lower Int64(::Float64): stack \
                         constructor is checked (InexactError) while the register op truncates"
                            .to_string(),
                    ),
                    (_, None) => Err(
                        "register VM shared-plan lowering cannot lower numeric conversion: \
                         operand numeric kind is not statically known"
                            .to_string(),
                    ),
                }
            }
            other => Err(format!(
                "register VM shared-plan lowering cannot lower expression: {other:?}"
            )),
        }
    }

    /// Statically-known numeric kind of `expr`, used to decide which
    /// register-native conversion (if any) a structural [`Expr::Convert`]
    /// node (Issue #9803) may safely lower to. Conservative: `None` means
    /// "not proven I64 or F64", which routes the caller to a stack fallback
    /// rather than a possibly-unsound register op.
    fn expr_numeric_kind(&self, expr: &Expr) -> Option<RegisterNumericKind> {
        match expr {
            Expr::Literal(Literal::Int(_), _) => Some(RegisterNumericKind::I64),
            Expr::Literal(Literal::Float(_) | Literal::Float32(_) | Literal::Float16(_), _) => {
                Some(RegisterNumericKind::F64)
            }
            Expr::Var(name, _) => {
                let slot = *self.slot_by_name.get(name.as_str())?;
                match self.slot_types.get(slot).copied().flatten()? {
                    VarTypeTag::I64 => Some(RegisterNumericKind::I64),
                    VarTypeTag::F64 | VarTypeTag::F32 | VarTypeTag::F16 => {
                        Some(RegisterNumericKind::F64)
                    }
                    _ => None,
                }
            }
            Expr::UnaryOp {
                op: UnaryOp::Neg | UnaryOp::Pos,
                operand,
                ..
            } => self.expr_numeric_kind(operand),
            Expr::Convert { target, .. } => Some(match target {
                NumericConvertTarget::Int64 => RegisterNumericKind::I64,
                NumericConvertTarget::Float64 => RegisterNumericKind::F64,
            }),
            _ => None,
        }
    }

    fn emit_branch_to_else(&mut self, cond: &Expr) -> Result<usize, String> {
        if let Some(branch) = self.lower_i64_slot_false_branch(cond) {
            let pc = self.instructions.len();
            self.instructions.push(branch);
            return Ok(pc);
        }

        let cond = self.lower_expr(cond)?;
        let pc = self.instructions.len();
        self.instructions.push(RegisterInstr::JumpIfFalse {
            src: cond,
            target: usize::MAX,
        });
        Ok(pc)
    }

    fn lower_i64_slot_false_branch(&self, cond: &Expr) -> Option<RegisterInstr> {
        let Expr::BinaryOp {
            op, left, right, ..
        } = cond
        else {
            return None;
        };
        let cmp = Self::cmp_op(*op)?;
        let lhs_slot = self.expr_var_slot(left)?;
        let rhs_slot = self.expr_var_slot(right)?;
        if !self.slot_prefers_i64(lhs_slot) || !self.slot_prefers_i64(rhs_slot) {
            return None;
        }
        Some(RegisterInstr::BranchCmpI64Slots {
            op: cmp.negated(),
            lhs_slot,
            rhs_slot,
            target: usize::MAX,
        })
    }

    fn try_lower_assignment_fusion(&mut self, slot: usize, expr: &Expr) -> Result<bool, String> {
        if self.slot_prefers_i64(slot) {
            if let Some(delta) = self.self_add_const_delta(slot, expr) {
                self.emit(RegisterInstr::AddConstI64Slot { slot, delta });
                return Ok(true);
            }
        }

        if self.slot_prefers_f64(slot) && self.try_lower_f64_bin_store(slot, expr)? {
            return Ok(true);
        }

        if self.slot_prefers_f64(slot) && self.is_self_neg(slot, expr) {
            self.emit(RegisterInstr::NegF64Slot { slot });
            return Ok(true);
        }

        Ok(false)
    }

    fn self_add_const_delta(&self, slot: usize, expr: &Expr) -> Option<i64> {
        let Expr::BinaryOp {
            op, left, right, ..
        } = expr
        else {
            return None;
        };
        match op {
            BinaryOp::Add => {
                if self.expr_var_slot(left) == Some(slot) {
                    return Self::literal_i64(right);
                }
                if self.expr_var_slot(right) == Some(slot) {
                    return Self::literal_i64(left);
                }
                None
            }
            BinaryOp::Sub if self.expr_var_slot(left) == Some(slot) => {
                Self::literal_i64(right).map(i64::wrapping_neg)
            }
            _ => None,
        }
    }

    fn is_self_neg(&self, slot: usize, expr: &Expr) -> bool {
        matches!(
            expr,
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand,
                ..
            } if self.expr_var_slot(operand) == Some(slot)
        )
    }

    fn try_lower_f64_bin_store(&mut self, slot: usize, expr: &Expr) -> Result<bool, String> {
        let Expr::BinaryOp {
            op, left, right, ..
        } = expr
        else {
            return Ok(false);
        };
        let Some(op) = Self::f64_bin_op(*op) else {
            return Ok(false);
        };
        if let Some(value) = Self::literal_f64(left) {
            if let Some((inner_op, lhs_slot, rhs_slot)) = self.f64_slot_pair_binary(right) {
                self.emit(RegisterInstr::BinF64ConstLeftBinSlotsStoreSlot {
                    outer_op: op,
                    inner_op,
                    slot,
                    value,
                    lhs_slot,
                    rhs_slot,
                });
                return Ok(true);
            }
            let rhs = self.lower_expr(right)?;
            self.emit(RegisterInstr::BinF64ConstLeftStoreSlot {
                op,
                slot,
                value,
                rhs,
            });
            return Ok(true);
        }
        let lhs = self.lower_expr(left)?;
        let rhs = self.lower_expr(right)?;
        self.emit(RegisterInstr::BinF64StoreSlot { op, slot, lhs, rhs });
        Ok(true)
    }

    fn f64_slot_pair_binary(&self, expr: &Expr) -> Option<(F64BinOp, usize, usize)> {
        let Expr::BinaryOp {
            op, left, right, ..
        } = expr
        else {
            return None;
        };
        let op = Self::f64_bin_op(*op)?;
        let lhs_slot = self.expr_var_slot(left)?;
        let rhs_slot = self.expr_var_slot(right)?;
        if !self.slot_prefers_numeric_for_f64(lhs_slot)
            || !self.slot_prefers_numeric_for_f64(rhs_slot)
        {
            return None;
        }
        Some((op, lhs_slot, rhs_slot))
    }

    fn lower_bin_f64_slots(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Option<usize> {
        let op = Self::f64_bin_op(op)?;
        let lhs_slot = self.expr_var_slot(left)?;
        let rhs_slot = self.expr_var_slot(right)?;
        if !self.slot_prefers_numeric_for_f64(lhs_slot)
            || !self.slot_prefers_numeric_for_f64(rhs_slot)
        {
            return None;
        }
        let dst = self.alloc_register();
        self.emit(RegisterInstr::BinF64Slots {
            op,
            dst,
            lhs_slot,
            rhs_slot,
        });
        Some(dst)
    }

    fn lower_bin_f64_const(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<Option<usize>, String> {
        let Some(op) = Self::f64_bin_op(op) else {
            return Ok(None);
        };

        if let (Some(a), Some(b)) = (Self::literal_f64(left), Self::literal_f64(right)) {
            let dst = self.alloc_register();
            self.emit(RegisterInstr::ConstF64 {
                dst,
                value: eval_f64_binop(op, a, b),
            });
            return Ok(Some(dst));
        }

        if let Some((slot, const_on_left, value)) = self.f64_slot_const_operands(left, right) {
            let dst = self.alloc_register();
            self.emit(RegisterInstr::BinF64SlotConst {
                op,
                dst,
                slot,
                value,
                const_on_left,
            });
            return Ok(Some(dst));
        }

        if let Some(value) = Self::literal_f64(right) {
            let src = self.lower_expr(left)?;
            let dst = self.alloc_register();
            self.emit(RegisterInstr::BinF64Const {
                op,
                dst,
                src,
                value,
            });
            return Ok(Some(dst));
        }

        if let Some(value) = Self::literal_f64(left) {
            let src = self.lower_expr(right)?;
            let dst = self.alloc_register();
            self.emit(RegisterInstr::BinF64ConstLeft {
                op,
                dst,
                value,
                src,
            });
            return Ok(Some(dst));
        }

        Ok(None)
    }

    fn f64_slot_const_operands(&self, left: &Expr, right: &Expr) -> Option<(usize, bool, f64)> {
        if let (Some(value), Some(slot)) = (Self::literal_f64(left), self.expr_var_slot(right)) {
            if self.slot_prefers_numeric_for_f64(slot) {
                return Some((slot, true, value));
            }
        }
        if let (Some(slot), Some(value)) = (self.expr_var_slot(left), Self::literal_f64(right)) {
            if self.slot_prefers_numeric_for_f64(slot) {
                return Some((slot, false, value));
            }
        }
        None
    }

    fn lower_bin_f64_slot(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<Option<usize>, String> {
        let Some(op) = Self::f64_bin_op(op) else {
            return Ok(None);
        };

        if let Some(slot) = self
            .expr_var_slot(right)
            .filter(|slot| self.slot_prefers_f64(*slot))
        {
            let src = self.lower_expr(left)?;
            let dst = self.alloc_register();
            self.emit(RegisterInstr::BinF64Slot { op, dst, src, slot });
            return Ok(Some(dst));
        }

        Ok(None)
    }

    fn emit_load_slot(&mut self, dst: usize, slot: usize) {
        if self.slot_prefers_f64(slot) {
            self.emit(RegisterInstr::LoadSlotF64 { dst, slot });
        } else if self.slot_prefers_i64(slot) {
            self.emit(RegisterInstr::LoadSlotI64 { dst, slot });
        } else {
            self.emit(RegisterInstr::LoadSlotAny { dst, slot });
        }
    }

    fn emit_store_slot(&mut self, slot: usize, src: usize) {
        if self.slot_prefers_f64(slot) {
            self.emit(RegisterInstr::StoreSlotF64 { slot, src });
        } else if self.slot_prefers_i64(slot) {
            self.emit(RegisterInstr::StoreSlotI64 { slot, src });
        } else {
            self.emit(RegisterInstr::StoreSlotAny { slot, src });
        }
    }

    fn expr_var_slot(&self, expr: &Expr) -> Option<usize> {
        match expr {
            Expr::Var(name, _) => self.slot_by_name.get(name.as_str()).copied(),
            _ => None,
        }
    }

    fn slot_prefers_f64(&self, slot: usize) -> bool {
        self.slot_types
            .get(slot)
            .copied()
            .flatten()
            .is_some_and(|tag| matches!(tag, VarTypeTag::F64 | VarTypeTag::F32 | VarTypeTag::F16))
    }

    fn slot_prefers_i64(&self, slot: usize) -> bool {
        self.slot_types
            .get(slot)
            .copied()
            .flatten()
            .is_some_and(|tag| tag == VarTypeTag::I64)
    }

    fn slot_prefers_numeric_for_f64(&self, slot: usize) -> bool {
        self.slot_prefers_f64(slot) || self.slot_prefers_i64(slot)
    }

    fn literal_i64(expr: &Expr) -> Option<i64> {
        match expr {
            Expr::Literal(Literal::Int(value), _) => Some(*value),
            _ => None,
        }
    }

    fn literal_f64(expr: &Expr) -> Option<f64> {
        match expr {
            Expr::Literal(Literal::Float(value), _) => Some(*value),
            Expr::Literal(Literal::Float32(value), _) => Some(*value as f64),
            Expr::Literal(Literal::Float16(value), _) => Some(value.to_f64()),
            _ => None,
        }
    }

    fn f64_bin_op(op: BinaryOp) -> Option<F64BinOp> {
        match op {
            BinaryOp::Add => Some(F64BinOp::Add),
            BinaryOp::Sub => Some(F64BinOp::Sub),
            BinaryOp::Mul => Some(F64BinOp::Mul),
            BinaryOp::Div => Some(F64BinOp::Div),
            _ => None,
        }
    }

    fn cmp_op(op: BinaryOp) -> Option<CmpOp> {
        match op {
            BinaryOp::Lt => Some(CmpOp::Lt),
            BinaryOp::Le => Some(CmpOp::Le),
            BinaryOp::Gt => Some(CmpOp::Gt),
            BinaryOp::Ge => Some(CmpOp::Ge),
            BinaryOp::Eq => Some(CmpOp::Eq),
            BinaryOp::Ne => Some(CmpOp::Ne),
            _ => None,
        }
    }

    fn expr_prefers_f64(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(Literal::Float(_) | Literal::Float32(_) | Literal::Float16(_), _) => true,
            Expr::Var(name, _) => self
                .slot_by_name
                .get(name.as_str())
                .and_then(|slot| self.slot_types.get(*slot))
                .copied()
                .flatten()
                .is_some_and(|tag| {
                    matches!(tag, VarTypeTag::F64 | VarTypeTag::F32 | VarTypeTag::F16)
                }),
            Expr::UnaryOp { operand, .. } => self.expr_prefers_f64(operand),
            Expr::BinaryOp {
                op, left, right, ..
            } => {
                *op == BinaryOp::Div || self.expr_prefers_f64(left) || self.expr_prefers_f64(right)
            }
            // Structural `Float64(x)` (Issue #9803) always produces an F64
            // value; `Int64(x)` never does.
            Expr::Convert { target, .. } => *target == NumericConvertTarget::Float64,
            _ => false,
        }
    }

    fn lower_call(
        &mut self,
        function: &str,
        args: &[Expr],
        kwargs: &[(crate::ir::core::InternedStr, Expr)],
        splat_mask: &[bool],
        kwargs_splat_mask: &[bool],
    ) -> Result<usize, String> {
        if !kwargs.is_empty() || kwargs_splat_mask.iter().any(|is_splat| *is_splat) {
            return Err(format!(
                "register VM shared-plan lowering cannot lower keyword call: {function}"
            ));
        }
        if splat_mask.iter().any(|is_splat| *is_splat) {
            return Err(format!(
                "register VM shared-plan lowering cannot lower splatted call: {function}"
            ));
        }
        let candidates = self.function_indices.get(function).ok_or_else(|| {
            format!("register VM shared-plan lowering cannot resolve call target: {function}")
        })?;
        let [func_index] = candidates.as_slice() else {
            return Err(format!(
                "register VM shared-plan lowering cannot resolve ambiguous call target: {function}"
            ));
        };

        let mut arg_regs = Vec::with_capacity(args.len());
        for arg in args {
            arg_regs.push(self.lower_expr(arg)?);
        }
        let args_start = self.next_register;
        for src in arg_regs {
            let dst = self.alloc_register();
            self.emit(RegisterInstr::Move { dst, src });
        }
        let dst = self.alloc_register();
        self.emit(RegisterInstr::CallStack {
            func_index: *func_index,
            args_start,
            arg_count: args.len(),
            dst,
            inbounds: false,
        });
        Ok(dst)
    }

    fn lower_literal(&mut self, literal: &Literal) -> Result<usize, String> {
        let dst = self.alloc_register();
        match literal {
            Literal::Int(value) => self.emit(RegisterInstr::ConstI64 { dst, value: *value }),
            Literal::Float(value) => self.emit(RegisterInstr::ConstF64 { dst, value: *value }),
            Literal::Bool(value) => self.emit(RegisterInstr::ConstBool { dst, value: *value }),
            Literal::Nothing => self.emit(RegisterInstr::ConstNothing { dst }),
            other => {
                return Err(format!(
                    "register VM shared-plan lowering cannot lower literal: {other:?}"
                ));
            }
        }
        Ok(dst)
    }

    fn emit(&mut self, instr: RegisterInstr) {
        self.instructions.push(instr);
    }

    fn emit_pending_jump(&mut self, instr: RegisterInstr, target: u32) {
        self.pending_jumps.push((self.instructions.len(), target));
        self.instructions.push(instr);
    }

    fn patch_jumps(&mut self) -> Result<(), String> {
        for (pc, target) in std::mem::take(&mut self.pending_jumps) {
            let target_pc = self
                .block_pc
                .get(target as usize)
                .copied()
                .flatten()
                .ok_or_else(|| {
                    format!("register VM shared-plan lowering: invalid block target {target}")
                })?;
            self.patch_single_jump(pc, target_pc)?;
        }
        Ok(())
    }

    fn patch_single_jump(&mut self, pc: usize, target_pc: usize) -> Result<(), String> {
        match self.instructions.get_mut(pc) {
            Some(RegisterInstr::Jump { target }) => {
                *target = target_pc;
                Ok(())
            }
            Some(RegisterInstr::JumpIfFalse { target, .. }) => {
                *target = target_pc;
                Ok(())
            }
            Some(RegisterInstr::BranchCmpI64Slots { target, .. }) => {
                *target = target_pc;
                Ok(())
            }
            Some(other) => Err(format!(
                "register VM shared-plan lowering: cannot patch non-jump at pc {pc}: {other:?}"
            )),
            None => Err(format!(
                "register VM shared-plan lowering: jump pc out of bounds: {pc}"
            )),
        }
    }

    fn alloc_register(&mut self) -> usize {
        let register = self.next_register;
        self.next_register += 1;
        self.max_registers = self.max_registers.max(self.next_register);
        register
    }

    fn slot_for_read(&self, name: &str) -> Result<usize, String> {
        self.slot_by_name.get(name).copied().ok_or_else(|| {
            format!("register VM shared-plan lowering: read of unknown slot `{name}`")
        })
    }

    fn slot_for_write(&mut self, name: &str) -> usize {
        if let Some(slot) = self.slot_by_name.get(name) {
            return *slot;
        }
        let slot = self.slot_names.len();
        self.slot_names.push(name.to_string());
        self.slot_types.push(None);
        self.slot_by_name.insert(name.to_string(), slot);
        slot
    }

    fn install_loop_blocks(&mut self) -> Vec<RegisterLoopBlock> {
        let mut blocks = Vec::new();
        let mut pc = 0usize;
        while pc < self.instructions.len() {
            if let Some(block) = self.try_build_loop_block(pc) {
                let block_index = blocks.len();
                let exit_pc = block.exit_pc;
                self.instructions[pc] = RegisterInstr::LoopI64Slots { block: block_index };
                blocks.push(block);
                pc = exit_pc;
            } else {
                pc += 1;
            }
        }
        blocks
    }

    fn try_build_loop_block(&self, header_pc: usize) -> Option<RegisterLoopBlock> {
        let RegisterInstr::BranchCmpI64Slots {
            op,
            lhs_slot,
            rhs_slot,
            target: exit_jump_pc,
        } = *self.instructions.get(header_pc)?
        else {
            return None;
        };
        let RegisterInstr::Jump { target: body_start } =
            *self.instructions.get(header_pc.checked_add(1)?)?
        else {
            return None;
        };
        let RegisterInstr::Jump { target: exit_pc } = *self.instructions.get(exit_jump_pc)? else {
            return None;
        };
        if body_start != exit_jump_pc.checked_add(1)? || body_start >= exit_pc {
            return None;
        }
        if !matches!(
            self.instructions.get(exit_pc.checked_sub(1)?)?,
            RegisterInstr::Jump { target } if *target == header_pc
        ) {
            return None;
        }

        let mut raw_ops = Vec::new();
        for instr in &self.instructions[body_start..exit_pc - 1] {
            raw_ops.push(Self::loop_op_from_instr(*instr)?);
        }
        let (f64_slots, f64_slot_map, f64_live_in, i64_slots, i64_slot_map) =
            self.loop_slot_maps(lhs_slot, rhs_slot, &raw_ops);
        let lhs_i64_index = Self::loop_i64_slot_index_from_map(&i64_slot_map, lhs_slot)?;
        let rhs_i64_index = Self::loop_i64_slot_index_from_map(&i64_slot_map, rhs_slot)?;
        let ops = Self::remap_loop_ops(&raw_ops, &f64_slot_map, &i64_slot_map)?;
        Some(RegisterLoopBlock {
            exit_op: op,
            lhs_i64_index,
            rhs_i64_index,
            exit_pc,
            f64_slots,
            f64_live_in,
            i64_slots,
            ops,
        })
    }

    fn loop_slot_maps(
        &self,
        lhs_slot: usize,
        rhs_slot: usize,
        ops: &[RawRegisterLoopOp],
    ) -> (
        Vec<usize>,
        Vec<Option<usize>>,
        Vec<bool>,
        Vec<usize>,
        Vec<Option<usize>>,
    ) {
        let mut f64_slots = Vec::new();
        let mut f64_written = HashSet::new();
        let mut f64_live_in_slots = HashSet::new();
        let mut i64_slots = Vec::new();
        self.add_loop_i64_slot(lhs_slot, &mut i64_slots);
        self.add_loop_i64_slot(rhs_slot, &mut i64_slots);
        for op in ops {
            match *op {
                RawRegisterLoopOp::LoadSlotF64 { slot, .. } => {
                    self.add_loop_numeric_read_slot(
                        slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                }
                RawRegisterLoopOp::StoreSlotF64 { slot, .. }
                | RawRegisterLoopOp::BinF64StoreSlot { slot, .. }
                | RawRegisterLoopOp::BinF64ConstLeftStoreSlot { slot, .. } => {
                    self.add_loop_f64_write_slot(slot, &mut f64_slots, &mut f64_written);
                }
                RawRegisterLoopOp::NegF64Slot { slot } => {
                    self.add_loop_numeric_read_slot(
                        slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                    self.add_loop_f64_write_slot(slot, &mut f64_slots, &mut f64_written);
                }
                RawRegisterLoopOp::BinF64Slots {
                    lhs_slot, rhs_slot, ..
                } => {
                    self.add_loop_numeric_read_slot(
                        lhs_slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                    self.add_loop_numeric_read_slot(
                        rhs_slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                }
                RawRegisterLoopOp::BinF64ConstLeftBinSlotsStoreSlot {
                    slot,
                    lhs_slot,
                    rhs_slot,
                    ..
                } => {
                    self.add_loop_numeric_read_slot(
                        lhs_slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                    self.add_loop_numeric_read_slot(
                        rhs_slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                    self.add_loop_f64_write_slot(slot, &mut f64_slots, &mut f64_written);
                }
                RawRegisterLoopOp::BinF64SlotConst { slot, .. } => {
                    self.add_loop_numeric_read_slot(
                        slot,
                        &mut f64_slots,
                        &mut i64_slots,
                        &f64_written,
                        &mut f64_live_in_slots,
                    );
                }
                RawRegisterLoopOp::AddConstI64Slot { slot, .. } => {
                    self.add_loop_i64_slot(slot, &mut i64_slots);
                }
                RawRegisterLoopOp::BinF64 { .. } | RawRegisterLoopOp::BinF64Const { .. } => {}
            }
        }
        let mut f64_slot_map = vec![None; self.slot_names.len()];
        for (idx, slot) in f64_slots.iter().copied().enumerate() {
            f64_slot_map[slot] = Some(idx);
        }
        let f64_live_in = f64_slots
            .iter()
            .map(|slot| f64_live_in_slots.contains(slot))
            .collect();
        let mut i64_slot_map = vec![None; self.slot_names.len()];
        for (idx, slot) in i64_slots.iter().copied().enumerate() {
            i64_slot_map[slot] = Some(idx);
        }
        (
            f64_slots,
            f64_slot_map,
            f64_live_in,
            i64_slots,
            i64_slot_map,
        )
    }

    fn add_loop_numeric_read_slot(
        &self,
        slot: usize,
        f64_slots: &mut Vec<usize>,
        i64_slots: &mut Vec<usize>,
        f64_written: &HashSet<usize>,
        f64_live_in_slots: &mut HashSet<usize>,
    ) {
        if self.slot_prefers_i64(slot) {
            self.add_loop_i64_slot(slot, i64_slots);
        } else {
            self.add_loop_f64_slot(slot, f64_slots);
            if !f64_written.contains(&slot) {
                f64_live_in_slots.insert(slot);
            }
        }
    }

    fn add_loop_f64_write_slot(
        &self,
        slot: usize,
        f64_slots: &mut Vec<usize>,
        f64_written: &mut HashSet<usize>,
    ) {
        self.add_loop_f64_slot(slot, f64_slots);
        f64_written.insert(slot);
    }

    fn add_loop_f64_slot(&self, slot: usize, slots: &mut Vec<usize>) {
        if !slots.contains(&slot) {
            slots.push(slot);
        }
    }

    fn add_loop_i64_slot(&self, slot: usize, slots: &mut Vec<usize>) {
        if !slots.contains(&slot) {
            slots.push(slot);
        }
    }

    fn remap_loop_ops(
        ops: &[RawRegisterLoopOp],
        f64_slot_map: &[Option<usize>],
        i64_slot_map: &[Option<usize>],
    ) -> Option<Vec<RegisterLoopOp>> {
        let ops = ops
            .iter()
            .copied()
            .map(|op| Self::remap_loop_op(op, f64_slot_map, i64_slot_map))
            .collect::<Option<Vec<_>>>()?;
        Some(Self::fuse_loop_ops(ops))
    }

    fn fuse_loop_ops(ops: Vec<RegisterLoopOp>) -> Vec<RegisterLoopOp> {
        let mut fused = Vec::with_capacity(ops.len());
        let mut idx = 0usize;
        while idx < ops.len() {
            if idx + 4 < ops.len() {
                if let (
                    RegisterLoopOp::LoadSlotF64 {
                        dst: lhs_dst,
                        source: lhs,
                    },
                    RegisterLoopOp::BinF64SlotConst {
                        op: inner_op,
                        dst: inner_dst,
                        source: inner_source,
                        value,
                        const_on_left: inner_const_on_left,
                    },
                    RegisterLoopOp::BinF64 {
                        op: middle_op,
                        dst: middle_dst,
                        lhs: middle_lhs,
                        rhs: middle_rhs,
                    },
                    RegisterLoopOp::LoadSlotF64 {
                        dst: tail_dst,
                        source: tail,
                    },
                    RegisterLoopOp::BinF64StoreSlot {
                        op: outer_op,
                        slot_index,
                        lhs: store_lhs,
                        rhs: store_rhs,
                    },
                ) = (
                    ops[idx],
                    ops[idx + 1],
                    ops[idx + 2],
                    ops[idx + 3],
                    ops[idx + 4],
                ) {
                    if middle_lhs == lhs_dst
                        && middle_rhs == inner_dst
                        && store_lhs == middle_dst
                        && store_rhs == tail_dst
                    {
                        fused.push(RegisterLoopOp::BinF64SourceSlotConstSourceStoreSlot {
                            outer_op,
                            middle_op,
                            inner_op,
                            slot_index,
                            lhs,
                            inner_source,
                            value,
                            inner_const_on_left,
                            tail,
                        });
                        idx += 5;
                        continue;
                    }
                }
            }

            if idx + 2 < ops.len() {
                if let (
                    RegisterLoopOp::LoadSlotF64 {
                        dst: lhs_dst,
                        source: lhs,
                    },
                    RegisterLoopOp::BinF64SlotConst {
                        op: inner_op,
                        dst: rhs_dst,
                        source: rhs,
                        value,
                        const_on_left,
                    },
                    RegisterLoopOp::BinF64StoreSlot {
                        op: outer_op,
                        slot_index,
                        lhs: store_lhs,
                        rhs: store_rhs,
                    },
                ) = (ops[idx], ops[idx + 1], ops[idx + 2])
                {
                    if store_lhs == lhs_dst && store_rhs == rhs_dst {
                        fused.push(RegisterLoopOp::BinF64SourceSlotConstStoreSlot {
                            outer_op,
                            inner_op,
                            slot_index,
                            lhs,
                            rhs,
                            value,
                            const_on_left,
                        });
                        idx += 3;
                        continue;
                    }
                }

                if let (
                    RegisterLoopOp::BinF64Slots {
                        op: slots_op,
                        dst: lhs_dst,
                        lhs,
                        rhs,
                    },
                    RegisterLoopOp::BinF64SlotConst {
                        op: slot_const_op,
                        dst: rhs_dst,
                        source: slot_const_source,
                        value,
                        const_on_left,
                    },
                    RegisterLoopOp::BinF64StoreSlot {
                        op: outer_op,
                        slot_index,
                        lhs: store_lhs,
                        rhs: store_rhs,
                    },
                ) = (ops[idx], ops[idx + 1], ops[idx + 2])
                {
                    if store_lhs == lhs_dst && store_rhs == rhs_dst {
                        fused.push(RegisterLoopOp::BinF64SlotsSlotConstStoreSlot {
                            outer_op,
                            slots_op,
                            slot_const_op,
                            slot_index,
                            lhs,
                            rhs,
                            slot_const_source,
                            value,
                            const_on_left,
                        });
                        idx += 3;
                        continue;
                    }
                }

                if let (
                    RegisterLoopOp::LoadSlotF64 {
                        dst: lhs_dst,
                        source: lhs,
                    },
                    RegisterLoopOp::LoadSlotF64 {
                        dst: rhs_dst,
                        source: rhs,
                    },
                    RegisterLoopOp::BinF64StoreSlot {
                        op,
                        slot_index,
                        lhs: store_lhs,
                        rhs: store_rhs,
                    },
                ) = (ops[idx], ops[idx + 1], ops[idx + 2])
                {
                    if store_lhs == lhs_dst && store_rhs == rhs_dst {
                        fused.push(RegisterLoopOp::BinF64SourcesStoreSlot {
                            op,
                            slot_index,
                            lhs,
                            rhs,
                        });
                        idx += 3;
                        continue;
                    }
                }
            }

            if idx + 1 < ops.len() {
                if let (
                    RegisterLoopOp::LoadSlotF64 { dst, source },
                    RegisterLoopOp::StoreSlotF64 { slot_index, src },
                ) = (ops[idx], ops[idx + 1])
                {
                    if src == dst {
                        fused.push(RegisterLoopOp::CopyF64Slot { slot_index, source });
                        idx += 2;
                        continue;
                    }
                }
            }

            fused.push(ops[idx]);
            idx += 1;
        }
        fused
    }

    fn remap_loop_op(
        op: RawRegisterLoopOp,
        f64_slot_map: &[Option<usize>],
        i64_slot_map: &[Option<usize>],
    ) -> Option<RegisterLoopOp> {
        match op {
            RawRegisterLoopOp::LoadSlotF64 { dst, slot } => Some(RegisterLoopOp::LoadSlotF64 {
                dst,
                source: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, slot)?,
            }),
            RawRegisterLoopOp::StoreSlotF64 { slot, src } => Some(RegisterLoopOp::StoreSlotF64 {
                slot_index: Self::loop_f64_slot_index_from_map(f64_slot_map, slot)?,
                src,
            }),
            RawRegisterLoopOp::BinF64 { op, dst, lhs, rhs } => {
                Some(RegisterLoopOp::BinF64 { op, dst, lhs, rhs })
            }
            RawRegisterLoopOp::BinF64Const {
                op,
                dst,
                src,
                value,
            } => Some(RegisterLoopOp::BinF64Const {
                op,
                dst,
                src,
                value,
            }),
            RawRegisterLoopOp::BinF64Slots {
                op,
                dst,
                lhs_slot,
                rhs_slot,
            } => Some(RegisterLoopOp::BinF64Slots {
                op,
                dst,
                lhs: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, lhs_slot)?,
                rhs: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, rhs_slot)?,
            }),
            RawRegisterLoopOp::BinF64SlotConst {
                op,
                dst,
                slot,
                value,
                const_on_left,
            } => Some(RegisterLoopOp::BinF64SlotConst {
                op,
                dst,
                source: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, slot)?,
                value,
                const_on_left,
            }),
            RawRegisterLoopOp::BinF64StoreSlot { op, slot, lhs, rhs } => {
                Some(RegisterLoopOp::BinF64StoreSlot {
                    op,
                    slot_index: Self::loop_f64_slot_index_from_map(f64_slot_map, slot)?,
                    lhs,
                    rhs,
                })
            }
            RawRegisterLoopOp::BinF64ConstLeftStoreSlot {
                op,
                slot,
                value,
                rhs,
            } => Some(RegisterLoopOp::BinF64ConstLeftStoreSlot {
                op,
                slot_index: Self::loop_f64_slot_index_from_map(f64_slot_map, slot)?,
                value,
                rhs,
            }),
            RawRegisterLoopOp::BinF64ConstLeftBinSlotsStoreSlot {
                outer_op,
                inner_op,
                slot,
                value,
                lhs_slot,
                rhs_slot,
            } => Some(RegisterLoopOp::BinF64ConstLeftBinSlotsStoreSlot {
                outer_op,
                inner_op,
                slot_index: Self::loop_f64_slot_index_from_map(f64_slot_map, slot)?,
                value,
                lhs: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, lhs_slot)?,
                rhs: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, rhs_slot)?,
            }),
            RawRegisterLoopOp::NegF64Slot { slot } => Some(RegisterLoopOp::NegF64Slot {
                slot_index: Self::loop_f64_slot_index_from_map(f64_slot_map, slot)?,
                source: Self::loop_f64_source_from_maps(f64_slot_map, i64_slot_map, slot)?,
            }),
            RawRegisterLoopOp::AddConstI64Slot { slot, delta } => {
                Some(RegisterLoopOp::AddConstI64Slot {
                    slot_index: Self::loop_i64_slot_index_from_map(i64_slot_map, slot)?,
                    delta,
                })
            }
        }
    }

    fn loop_f64_source_from_maps(
        f64_slot_map: &[Option<usize>],
        i64_slot_map: &[Option<usize>],
        slot: usize,
    ) -> Option<LoopF64Source> {
        if let Some(idx) = Self::loop_i64_slot_index_from_map(i64_slot_map, slot) {
            Some(LoopF64Source::I64Slot(idx))
        } else {
            Self::loop_f64_slot_index_from_map(f64_slot_map, slot).map(LoopF64Source::F64Slot)
        }
    }

    fn loop_f64_slot_index_from_map(slot_map: &[Option<usize>], slot: usize) -> Option<usize> {
        slot_map.get(slot).copied().flatten()
    }

    fn loop_i64_slot_index_from_map(slot_map: &[Option<usize>], slot: usize) -> Option<usize> {
        slot_map.get(slot).copied().flatten()
    }

    fn loop_op_from_instr(instr: RegisterInstr) -> Option<RawRegisterLoopOp> {
        match instr {
            RegisterInstr::LoadSlotF64 { dst, slot } => {
                Some(RawRegisterLoopOp::LoadSlotF64 { dst, slot })
            }
            RegisterInstr::StoreSlotF64 { slot, src } => {
                Some(RawRegisterLoopOp::StoreSlotF64 { slot, src })
            }
            RegisterInstr::BinF64 { op, dst, lhs, rhs } => {
                Some(RawRegisterLoopOp::BinF64 { op, dst, lhs, rhs })
            }
            RegisterInstr::BinF64Const {
                op,
                dst,
                src,
                value,
            } => Some(RawRegisterLoopOp::BinF64Const {
                op,
                dst,
                src,
                value,
            }),
            RegisterInstr::BinF64Slots {
                op,
                dst,
                lhs_slot,
                rhs_slot,
            } => Some(RawRegisterLoopOp::BinF64Slots {
                op,
                dst,
                lhs_slot,
                rhs_slot,
            }),
            RegisterInstr::BinF64SlotConst {
                op,
                dst,
                slot,
                value,
                const_on_left,
            } => Some(RawRegisterLoopOp::BinF64SlotConst {
                op,
                dst,
                slot,
                value,
                const_on_left,
            }),
            RegisterInstr::BinF64StoreSlot { op, slot, lhs, rhs } => {
                Some(RawRegisterLoopOp::BinF64StoreSlot { op, slot, lhs, rhs })
            }
            RegisterInstr::BinF64ConstLeftStoreSlot {
                op,
                slot,
                value,
                rhs,
            } => Some(RawRegisterLoopOp::BinF64ConstLeftStoreSlot {
                op,
                slot,
                value,
                rhs,
            }),
            RegisterInstr::BinF64ConstLeftBinSlotsStoreSlot {
                outer_op,
                inner_op,
                slot,
                value,
                lhs_slot,
                rhs_slot,
            } => Some(RawRegisterLoopOp::BinF64ConstLeftBinSlotsStoreSlot {
                outer_op,
                inner_op,
                slot,
                value,
                lhs_slot,
                rhs_slot,
            }),
            RegisterInstr::NegF64Slot { slot } => Some(RawRegisterLoopOp::NegF64Slot { slot }),
            RegisterInstr::AddConstI64Slot { slot, delta } => {
                Some(RawRegisterLoopOp::AddConstI64Slot { slot, delta })
            }
            _ => None,
        }
    }
}

struct RegionLowering<'a> {
    code: &'a [Instr],
    start: usize,
    end: usize,
    instructions: Vec<RegisterInstr>,
    /// Operand-stack depth entering the instruction currently being lowered.
    /// `None` marks unreachable code (after `Return` / unconditional `Jump`
    /// with no seeded branch target).
    depth: Option<usize>,
    max_depth: usize,
    /// stack ip (region-relative) → first lowered register pc.
    pc_map: Vec<Option<usize>>,
    /// Seeded operand depth at branch targets (region-relative).
    depth_at: Vec<Option<usize>>,
    /// (register pc, absolute stack target ip) fixups patched after the scan.
    pending_jumps: Vec<(usize, usize)>,
    slot_count: usize,
}

fn lower_region(
    code: &[Instr],
    start: usize,
    end: usize,
    slot_count: usize,
    slot_names: Rc<Vec<String>>,
    name: String,
) -> Result<RegisterProgram, String> {
    if start > end || end > code.len() {
        return Err(format!(
            "register VM prototype: invalid code region {start}..{end} (code len {})",
            code.len()
        ));
    }
    let len = end - start;
    let mut lowering = RegionLowering {
        code,
        start,
        end,
        instructions: Vec::with_capacity(len),
        depth: Some(0),
        max_depth: 0,
        pc_map: vec![None; len],
        depth_at: vec![None; len],
        pending_jumps: Vec::new(),
        slot_count,
    };

    for ip in start..end {
        lowering.lower_at(ip)?;
    }

    if lowering.depth.is_some() {
        return Err(
            "register VM prototype: code region falls through without a return".to_string(),
        );
    }

    // Patch branch targets from stack ips to register pcs.
    let pending = std::mem::take(&mut lowering.pending_jumps);
    for (pc, target) in pending {
        let reg_target = lowering.resolve_target(target)?;
        match &mut lowering.instructions[pc] {
            RegisterInstr::Jump { target }
            | RegisterInstr::JumpIfFalse { target, .. }
            | RegisterInstr::BranchCmpI64 { target, .. }
            | RegisterInstr::BranchCmpF64 { target, .. }
            | RegisterInstr::BranchGtI64Slots { target, .. }
            | RegisterInstr::BranchCmpI64Slots { target, .. }
            | RegisterInstr::AddConstI64SlotBranchLe { target, .. } => *target = reg_target,
            other => {
                return Err(format!(
                    "register VM prototype: jump fixup hit non-branch instruction {other:?}"
                ))
            }
        }
    }

    Ok(RegisterProgram {
        instructions: lowering.instructions,
        loop_blocks: Vec::new(),
        frame_registers: lowering.max_depth,
        slot_count,
        slot_names,
        name,
    })
}

impl RegionLowering<'_> {
    fn resolve_target(&self, target: usize) -> Result<usize, String> {
        if target < self.start || target >= self.end {
            return Err(format!(
                "register VM prototype: jump target {target} outside code region {}..{}",
                self.start, self.end
            ));
        }
        self.pc_map[target - self.start].ok_or_else(|| {
            format!("register VM prototype: jump target {target} was not lowered (unreachable?)")
        })
    }

    /// Seed the operand depth expected at a branch target; consistent merges
    /// are required, mismatches are explicit translation errors.
    fn seed(&mut self, target: usize, depth: usize) -> Result<(), String> {
        if target < self.start || target >= self.end {
            return Err(format!(
                "register VM prototype: jump target {target} outside code region {}..{}",
                self.start, self.end
            ));
        }
        let entry = &mut self.depth_at[target - self.start];
        match entry {
            Some(existing) if *existing != depth => Err(format!(
                "register VM prototype: inconsistent operand depth at jump target {target} \
                 ({existing} vs {depth})"
            )),
            _ => {
                *entry = Some(depth);
                Ok(())
            }
        }
    }

    fn push(&mut self) -> Result<usize, String> {
        let Some(depth) = self.depth.as_mut() else {
            return Err("register VM prototype: push in unreachable code".to_string());
        };
        let dst = *depth;
        *depth += 1;
        if *depth > self.max_depth {
            self.max_depth = *depth;
        }
        Ok(dst)
    }

    fn pop(&mut self, context: &str) -> Result<usize, String> {
        let Some(depth) = self.depth.as_mut() else {
            return Err(format!(
                "register VM prototype: pop in unreachable code at {context}"
            ));
        };
        if *depth == 0 {
            return Err(format!(
                "register VM prototype: operand underflow at {context}"
            ));
        }
        *depth -= 1;
        Ok(*depth)
    }

    fn peek(&self, context: &str) -> Result<usize, String> {
        match self.depth {
            Some(depth) if depth > 0 => Ok(depth - 1),
            Some(_) => Err(format!(
                "register VM prototype: operand underflow at {context}"
            )),
            None => Err(format!(
                "register VM prototype: peek in unreachable code at {context}"
            )),
        }
    }

    fn ensure_slot(&self, slot: usize, context: &str) -> Result<(), String> {
        if slot < self.slot_count {
            Ok(())
        } else {
            Err(format!(
                "register VM prototype: {context} slot {slot} out of bounds for {} slots",
                self.slot_count
            ))
        }
    }

    fn emit(&mut self, instr: RegisterInstr) {
        self.instructions.push(instr);
    }

    fn emit_branch(&mut self, instr: RegisterInstr, stack_target: usize) -> Result<(), String> {
        let depth = self
            .depth
            .ok_or_else(|| "register VM prototype: branch in unreachable code".to_string())?;
        self.seed(stack_target, depth)?;
        self.pending_jumps
            .push((self.instructions.len(), stack_target));
        self.instructions.push(instr);
        Ok(())
    }

    fn lower_at(&mut self, ip: usize) -> Result<(), String> {
        let rel = ip - self.start;
        // Merge any seeded branch-target depth with the fall-through depth.
        if let Some(seed) = self.depth_at[rel] {
            match self.depth {
                Some(depth) if depth != seed => {
                    return Err(format!(
                        "register VM prototype: inconsistent operand depth at ip {ip} \
                         ({depth} vs seeded {seed})"
                    ));
                }
                _ => self.depth = Some(seed),
            }
        } else if let Some(depth) = self.depth {
            // Record the fall-through depth so later backward jumps can be
            // consistency-checked.
            self.depth_at[rel] = Some(depth);
        }

        if self.depth.is_none() {
            // Unreachable instruction (e.g. the dead `Jump` the compiler
            // emits after a `return` inside `if`). Not lowered, not mapped:
            // a later jump into it is an explicit error.
            return Ok(());
        }

        self.pc_map[rel] = Some(self.instructions.len());
        self.lower_instr(ip)
    }

    fn lower_instr(&mut self, ip: usize) -> Result<(), String> {
        let instr = &self.code[ip];
        match instr {
            // ===== constants =====
            Instr::PushI64(value) => {
                let dst = self.push()?;
                self.emit(RegisterInstr::ConstI64 { dst, value: *value });
            }
            Instr::PushF64(value) => {
                let dst = self.push()?;
                self.emit(RegisterInstr::ConstF64 { dst, value: *value });
            }
            Instr::PushBool(value) => {
                let dst = self.push()?;
                self.emit(RegisterInstr::ConstBool { dst, value: *value });
            }
            Instr::PushNothing => {
                let dst = self.push()?;
                self.emit(RegisterInstr::ConstNothing { dst });
            }

            // ===== slot loads/stores =====
            Instr::LoadSlotI64(slot) => {
                self.ensure_slot(*slot, "LoadSlotI64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotI64 { dst, slot: *slot });
            }
            Instr::StoreSlotI64(slot) => {
                self.ensure_slot(*slot, "StoreSlotI64")?;
                let src = self.pop("StoreSlotI64")?;
                self.emit(RegisterInstr::StoreSlotI64 { slot: *slot, src });
            }
            Instr::LoadSlotF64(slot) => {
                self.ensure_slot(*slot, "LoadSlotF64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotF64 { dst, slot: *slot });
            }
            Instr::StoreSlotF64(slot) => {
                self.ensure_slot(*slot, "StoreSlotF64")?;
                let src = self.pop("StoreSlotF64")?;
                self.emit(RegisterInstr::StoreSlotF64 { slot: *slot, src });
            }
            Instr::LoadSlotI64ToF64(slot) => {
                self.ensure_slot(*slot, "LoadSlotI64ToF64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotI64ToF64 { dst, slot: *slot });
            }
            Instr::LoadSlot(slot) => {
                self.ensure_slot(*slot, "LoadSlot")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotAny { dst, slot: *slot });
            }
            Instr::StoreSlot(slot) => {
                self.ensure_slot(*slot, "StoreSlot")?;
                let src = self.pop("StoreSlot")?;
                self.emit(RegisterInstr::StoreSlotAny { slot: *slot, src });
            }

            // ===== I64 arithmetic =====
            Instr::AddI64 | Instr::SubI64 | Instr::MulI64 | Instr::ModI64 => {
                let op = match instr {
                    Instr::AddI64 => I64BinOp::Add,
                    Instr::SubI64 => I64BinOp::Sub,
                    Instr::MulI64 => I64BinOp::Mul,
                    _ => I64BinOp::Rem,
                };
                let rhs = self.pop("I64 binop rhs")?;
                let lhs = self.pop("I64 binop lhs")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::BinI64 { op, dst, lhs, rhs });
            }
            Instr::IncI64 => {
                let src = self.pop("IncI64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::AddConstI64 { dst, src, value: 1 });
            }
            Instr::NegI64 => {
                let src = self.pop("NegI64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::NegI64 { dst, src });
            }
            Instr::AddConstI64Slot(slot, delta) => {
                self.ensure_slot(*slot, "AddConstI64Slot")?;
                self.emit(RegisterInstr::AddConstI64Slot {
                    slot: *slot,
                    delta: *delta,
                });
            }
            Instr::LoadAddConstI64Slot(slot, delta) => {
                // Unfused `LoadSlotI64; PushI64(delta); AddI64`.
                self.ensure_slot(*slot, "LoadAddConstI64Slot")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotI64 { dst, slot: *slot });
                self.emit(RegisterInstr::AddConstI64 {
                    dst,
                    src: dst,
                    value: *delta,
                });
            }

            // ===== F64 arithmetic =====
            Instr::AddF64 | Instr::SubF64 | Instr::MulF64 | Instr::DivF64 | Instr::PowF64 => {
                let op = match instr {
                    Instr::AddF64 => F64BinOp::Add,
                    Instr::SubF64 => F64BinOp::Sub,
                    Instr::MulF64 => F64BinOp::Mul,
                    Instr::DivF64 => F64BinOp::Div,
                    _ => F64BinOp::Pow,
                };
                let rhs = self.pop("F64 binop rhs")?;
                let lhs = self.pop("F64 binop lhs")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::BinF64 { op, dst, lhs, rhs });
            }
            Instr::NegF64 => {
                let src = self.pop("NegF64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::NegF64 { dst, src });
            }
            Instr::LoadAddF64Slot(slot)
            | Instr::LoadSubF64Slot(slot)
            | Instr::LoadMulF64Slot(slot)
            | Instr::LoadDivF64Slot(slot) => {
                let op = match instr {
                    Instr::LoadAddF64Slot(_) => F64BinOp::Add,
                    Instr::LoadSubF64Slot(_) => F64BinOp::Sub,
                    Instr::LoadMulF64Slot(_) => F64BinOp::Mul,
                    _ => F64BinOp::Div,
                };
                self.ensure_slot(*slot, "LoadOpF64Slot")?;
                let src = self.pop("LoadOpF64Slot")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::BinF64Slot {
                    op,
                    dst,
                    src,
                    slot: *slot,
                });
            }
            // Issue #9126: fused `slot[dst] = slot[lhs] + slot[rhs]` (with the
            // I64→F64 converting rhs form). Translated as the unfused
            // load/load/add/store register sequence — the register VM already
            // pays no stack-dispatch cost, so parity matters more than fusion.
            Instr::AddF64Slots(dst_slot, lhs_slot, rhs_slot)
            | Instr::AddF64I64Slots(dst_slot, lhs_slot, rhs_slot) => {
                let name = if matches!(instr, Instr::AddF64Slots(..)) {
                    "AddF64Slots"
                } else {
                    "AddF64I64Slots"
                };
                self.ensure_slot(*lhs_slot, name)?;
                self.ensure_slot(*rhs_slot, name)?;
                self.ensure_slot(*dst_slot, name)?;
                let lhs = self.push()?;
                self.emit(RegisterInstr::LoadSlotF64 {
                    dst: lhs,
                    slot: *lhs_slot,
                });
                let rhs = self.push()?;
                if matches!(instr, Instr::AddF64Slots(..)) {
                    self.emit(RegisterInstr::LoadSlotF64 {
                        dst: rhs,
                        slot: *rhs_slot,
                    });
                } else {
                    self.emit(RegisterInstr::LoadSlotI64ToF64 {
                        dst: rhs,
                        slot: *rhs_slot,
                    });
                }
                let rhs = self.pop(name)?;
                let lhs = self.pop(name)?;
                let sum = self.push()?;
                self.emit(RegisterInstr::BinF64 {
                    op: F64BinOp::Add,
                    dst: sum,
                    lhs,
                    rhs,
                });
                let src = self.pop(name)?;
                self.emit(RegisterInstr::StoreSlotF64 {
                    slot: *dst_slot,
                    src,
                });
            }
            Instr::LoadSquareF64Slot(slot) => {
                // Unfused `LoadSlotF64; DupF64; MulF64`.
                self.ensure_slot(*slot, "LoadSquareF64Slot")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotF64 { dst, slot: *slot });
                self.emit(RegisterInstr::BinF64 {
                    op: F64BinOp::Mul,
                    dst,
                    lhs: dst,
                    rhs: dst,
                });
            }

            // ===== comparisons =====
            Instr::LtI64
            | Instr::LeI64
            | Instr::GtI64
            | Instr::GeI64
            | Instr::EqI64
            | Instr::NeI64 => {
                let op = match instr {
                    Instr::LtI64 => CmpOp::Lt,
                    Instr::LeI64 => CmpOp::Le,
                    Instr::GtI64 => CmpOp::Gt,
                    Instr::GeI64 => CmpOp::Ge,
                    Instr::EqI64 => CmpOp::Eq,
                    _ => CmpOp::Ne,
                };
                let rhs = self.pop("I64 compare rhs")?;
                let lhs = self.pop("I64 compare lhs")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::CmpI64 { op, dst, lhs, rhs });
            }
            Instr::LtF64
            | Instr::LeF64
            | Instr::GtF64
            | Instr::GeF64
            | Instr::EqF64
            | Instr::NeF64 => {
                let op = match instr {
                    Instr::LtF64 => CmpOp::Lt,
                    Instr::LeF64 => CmpOp::Le,
                    Instr::GtF64 => CmpOp::Gt,
                    Instr::GeF64 => CmpOp::Ge,
                    Instr::EqF64 => CmpOp::Eq,
                    _ => CmpOp::Ne,
                };
                let rhs = self.pop("F64 compare rhs")?;
                let lhs = self.pop("F64 compare lhs")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::CmpF64 { op, dst, lhs, rhs });
            }

            // ===== conversions / stack shuffles =====
            Instr::ToF64 => {
                let src = self.pop("ToF64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::I64ToF64 { dst, src });
            }
            Instr::ToI64 => {
                let src = self.pop("ToI64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::F64ToI64 { dst, src });
            }
            Instr::BoolToI64 => {
                let src = self.pop("BoolToI64")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::BoolToI64 { dst, src });
            }
            Instr::I64ToBool => {
                let src = self.pop("I64ToBool")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::I64ToBool { dst, src });
            }
            Instr::NotBool => {
                let src = self.pop("NotBool")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::NotBool { dst, src });
            }
            Instr::Dup | Instr::DupI64 | Instr::DupF64 => {
                let src = self.peek("Dup")?;
                let dst = self.push()?;
                self.emit(RegisterInstr::Move { dst, src });
            }
            Instr::Pop => {
                // Pure translation-time effect: the dead value simply stays in
                // its (now unused) register.
                self.pop("Pop")?;
            }

            // ===== control flow =====
            Instr::Jump(target) => {
                self.emit_branch(RegisterInstr::Jump { target: usize::MAX }, *target)?;
                self.depth = None;
            }
            Instr::JumpIfZero(target) => {
                let src = self.pop("JumpIfZero")?;
                self.emit_branch(
                    RegisterInstr::JumpIfFalse {
                        src,
                        target: usize::MAX,
                    },
                    *target,
                )?;
            }
            Instr::JumpIfEqI64(target)
            | Instr::JumpIfNeI64(target)
            | Instr::JumpIfLtI64(target)
            | Instr::JumpIfGtI64(target)
            | Instr::JumpIfLeI64(target)
            | Instr::JumpIfGeI64(target) => {
                let op = match instr {
                    Instr::JumpIfEqI64(_) => CmpOp::Eq,
                    Instr::JumpIfNeI64(_) => CmpOp::Ne,
                    Instr::JumpIfLtI64(_) => CmpOp::Lt,
                    Instr::JumpIfGtI64(_) => CmpOp::Gt,
                    Instr::JumpIfLeI64(_) => CmpOp::Le,
                    _ => CmpOp::Ge,
                };
                let rhs = self.pop("I64 branch rhs")?;
                let lhs = self.pop("I64 branch lhs")?;
                self.emit_branch(
                    RegisterInstr::BranchCmpI64 {
                        op,
                        lhs,
                        rhs,
                        target: usize::MAX,
                    },
                    *target,
                )?;
            }
            Instr::JumpIfEqF64(target)
            | Instr::JumpIfNeF64(target)
            | Instr::JumpIfNotLtF64(target)
            | Instr::JumpIfNotGtF64(target)
            | Instr::JumpIfNotLeF64(target)
            | Instr::JumpIfNotGeF64(target) => {
                let op = match instr {
                    Instr::JumpIfEqF64(_) => F64BranchOp::Eq,
                    Instr::JumpIfNeF64(_) => F64BranchOp::Ne,
                    Instr::JumpIfNotLtF64(_) => F64BranchOp::NotLt,
                    Instr::JumpIfNotGtF64(_) => F64BranchOp::NotGt,
                    Instr::JumpIfNotLeF64(_) => F64BranchOp::NotLe,
                    _ => F64BranchOp::NotGe,
                };
                let rhs = self.pop("F64 branch rhs")?;
                let lhs = self.pop("F64 branch lhs")?;
                self.emit_branch(
                    RegisterInstr::BranchCmpF64 {
                        op,
                        lhs,
                        rhs,
                        target: usize::MAX,
                    },
                    *target,
                )?;
            }
            Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, target) => {
                self.ensure_slot(*lhs_slot, "JumpIfGtI64Slots")?;
                self.ensure_slot(*rhs_slot, "JumpIfGtI64Slots")?;
                self.emit_branch(
                    RegisterInstr::BranchGtI64Slots {
                        lhs_slot: *lhs_slot,
                        rhs_slot: *rhs_slot,
                        target: usize::MAX,
                    },
                    *target,
                )?;
            }
            Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, target) => {
                self.ensure_slot(*slot, "AddConstI64SlotAndJumpIfLe")?;
                self.ensure_slot(*stop_slot, "AddConstI64SlotAndJumpIfLe")?;
                self.emit_branch(
                    RegisterInstr::AddConstI64SlotBranchLe {
                        slot: *slot,
                        delta: *delta,
                        stop_slot: *stop_slot,
                        target: usize::MAX,
                    },
                    *target,
                )?;
            }
            // Fused slot-vs-constant compare-and-branch (Issue #10105). Decompose
            // into the primitive register ops it stands for — load the I64 slot,
            // materialize the constant, branch — so register-eligible functions
            // with a constant loop guard keep their register translation instead
            // of falling back to the stack VM.
            Instr::JumpIfCmpI64SlotConst(slot, konst, cmp, target) => {
                self.ensure_slot(*slot, "JumpIfCmpI64SlotConst")?;
                let lhs_dst = self.push()?;
                self.emit(RegisterInstr::LoadSlotI64 {
                    dst: lhs_dst,
                    slot: *slot,
                });
                let rhs_dst = self.push()?;
                self.emit(RegisterInstr::ConstI64 {
                    dst: rhs_dst,
                    value: *konst,
                });
                let op = match cmp {
                    I64Cmp::Lt => CmpOp::Lt,
                    I64Cmp::Gt => CmpOp::Gt,
                    I64Cmp::Le => CmpOp::Le,
                    I64Cmp::Ge => CmpOp::Ge,
                    I64Cmp::Eq => CmpOp::Eq,
                    I64Cmp::Ne => CmpOp::Ne,
                };
                let rhs = self.pop("JumpIfCmpI64SlotConst rhs")?;
                let lhs = self.pop("JumpIfCmpI64SlotConst lhs")?;
                self.emit_branch(
                    RegisterInstr::BranchCmpI64 {
                        op,
                        lhs,
                        rhs,
                        target: usize::MAX,
                    },
                    *target,
                )?;
            }

            // ===== calls (stack VM trampoline) =====
            Instr::Call(func_index, arg_count)
            | Instr::CallInbounds(func_index, arg_count)
            | Instr::CallResolved(func_index, arg_count) => {
                let inbounds = matches!(instr, Instr::CallInbounds(..));
                let mut args_start = None;
                for _ in 0..*arg_count {
                    args_start = Some(self.pop("call argument")?);
                }
                let dst = self.push()?;
                self.emit(RegisterInstr::CallStack {
                    func_index: *func_index,
                    args_start: args_start.unwrap_or(dst),
                    arg_count: *arg_count,
                    dst,
                    inbounds,
                });
            }
            Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => {
                // Unfused `LoadSlotI64(slot)...; Call{Resolved,Inbounds}`.
                let inbounds = matches!(instr, Instr::CallInboundsI64Slots(..));
                let mut args_start = None;
                for slot in &operands.slots {
                    self.ensure_slot(*slot, "CallI64Slots")?;
                    let dst = self.push()?;
                    args_start.get_or_insert(dst);
                    self.emit(RegisterInstr::LoadSlotI64 { dst, slot: *slot });
                }
                for _ in 0..operands.slots.len() {
                    self.pop("CallI64Slots args")?;
                }
                let dst = self.push()?;
                self.emit(RegisterInstr::CallStack {
                    func_index: operands.func_index,
                    args_start: args_start.unwrap_or(dst),
                    arg_count: operands.slots.len(),
                    dst,
                    inbounds,
                });
            }
            Instr::CallIntrinsic(intrinsic) => {
                let arity = intrinsic.arity();
                let mut args_start = None;
                for _ in 0..arity {
                    args_start = Some(self.pop("intrinsic argument")?);
                }
                let dst = self.push()?;
                self.emit(RegisterInstr::CallIntrinsic {
                    intrinsic: *intrinsic,
                    args_start: args_start.unwrap_or(dst),
                    arg_count: arity,
                    dst,
                });
            }

            // ===== returns =====
            Instr::ReturnI64 => {
                let src = self.pop("ReturnI64")?;
                self.emit(RegisterInstr::Return {
                    kind: ReturnKind::I64,
                    src,
                });
                self.depth = None;
            }
            Instr::ReturnF64 => {
                let src = self.pop("ReturnF64")?;
                self.emit(RegisterInstr::Return {
                    kind: ReturnKind::F64,
                    src,
                });
                self.depth = None;
            }
            Instr::ReturnAny => {
                let src = self.pop("ReturnAny")?;
                self.emit(RegisterInstr::Return {
                    kind: ReturnKind::Any,
                    src,
                });
                self.depth = None;
            }
            Instr::ReturnNothing => {
                self.emit(RegisterInstr::Return {
                    kind: ReturnKind::Nothing,
                    src: 0,
                });
                self.depth = None;
            }

            other => {
                return Err(format!(
                    "register VM prototype cannot lower stack instruction: {other:?}"
                ))
            }
        }
        Ok(())
    }
}

// ===================== Interpretation =====================

/// Outcome of a register program run: the returned value plus the dynamic
/// dispatch count (for the #8559 measurement matrix).
#[derive(Debug)]
pub struct RegisterRunOutcome {
    pub value: Value,
    pub dispatch_count: usize,
    pub register_call_count: usize,
}

struct ActiveRegisterFrame {
    program: Rc<RegisterProgram>,
    slots: Vec<Option<Value>>,
    registers: Vec<Option<Value>>,
    pc: usize,
    return_dst: Option<usize>,
}

impl ActiveRegisterFrame {
    fn new(
        program: Rc<RegisterProgram>,
        slots: Vec<Option<Value>>,
        return_dst: Option<usize>,
    ) -> Self {
        let registers = vec![None; program.frame_registers()];
        Self {
            program,
            slots,
            registers,
            pc: 0,
            return_dst,
        }
    }
}

/// How often the interpreter polls the cooperative cancellation flag.
const CANCEL_CHECK_INTERVAL: usize = 1 << 12;
/// Native register frames are heap-backed, but runaway recursion must still
/// surface as Julia's catchable StackOverflowError instead of growing until OOM
/// (Issue #10054). Keep this in sync with `Vm::MAX_CALL_DEPTH`.
const MAX_REGISTER_CALL_DEPTH: usize = 10_000;

/// Execute a lowered register program against pre-bound local slots.
///
/// Translatable calls push explicit register frames; other calls and intrinsics
/// trampoline into `host` (the stack VM). Errors are structured [`VmError`]s so
/// the gate can `raise` them through the stack VM's normal handler machinery.
pub fn execute_register_program(
    program: &RegisterProgram,
    slots: &mut [Option<Value>],
    host: &mut dyn RegisterVmHost,
) -> Result<RegisterRunOutcome, VmError> {
    let mut frames = vec![ActiveRegisterFrame::new(
        Rc::new(program.clone()),
        slots.to_vec(),
        None,
    )];
    let mut dispatch_count = 0usize;
    let mut register_call_count = 0usize;

    macro_rules! current_program {
        () => {
            frames
                .last()
                .ok_or_else(|| VmError::InternalError("register VM: no active frame".to_string()))?
                .program
                .as_ref()
        };
    }

    macro_rules! current_slots {
        () => {
            &frames
                .last()
                .ok_or_else(|| VmError::InternalError("register VM: no active frame".to_string()))?
                .slots
        };
    }

    macro_rules! current_slots_mut {
        () => {
            &mut frames
                .last_mut()
                .ok_or_else(|| VmError::InternalError("register VM: no active frame".to_string()))?
                .slots
        };
    }

    macro_rules! reg {
        ($idx:expr) => {
            frames
                .last()
                .ok_or_else(|| VmError::InternalError("register VM: no active frame".to_string()))?
                .registers
                .get($idx)
                .and_then(Option::as_ref)
                .ok_or_else(|| {
                    VmError::InternalError(format!("register VM: register {} is undefined", $idx))
                })
        };
    }

    macro_rules! set_reg {
        ($idx:expr, $value:expr) => {{
            let slot = frames
                .last_mut()
                .ok_or_else(|| VmError::InternalError("register VM: no active frame".to_string()))?
                .registers
                .get_mut($idx)
                .ok_or_else(|| {
                    VmError::InternalError(format!("register VM: register {} out of bounds", $idx))
                })?;
            *slot = Some($value);
        }};
    }

    while !frames.is_empty() {
        let instr = {
            let frame = frames.last_mut().ok_or_else(|| {
                VmError::InternalError("register VM: no active frame".to_string())
            })?;
            let Some(instr) = frame.program.instructions().get(frame.pc).copied() else {
                return Err(VmError::InternalError(format!(
                    "register VM prototype reached end of program without return: {}",
                    frame.program.name()
                )));
            };
            frame.pc += 1;
            instr
        };
        dispatch_count += 1;
        if dispatch_count.is_multiple_of(CANCEL_CHECK_INTERVAL) && crate::cancel::is_requested() {
            return Err(VmError::Cancelled);
        }
        match instr {
            RegisterInstr::ConstI64 { dst, value } => set_reg!(dst, Value::I64(value)),
            RegisterInstr::ConstF64 { dst, value } => set_reg!(dst, Value::F64(value)),
            RegisterInstr::ConstBool { dst, value } => set_reg!(dst, Value::Bool(value)),
            RegisterInstr::ConstNothing { dst } => set_reg!(dst, Value::Nothing),

            RegisterInstr::LoadSlotI64 { dst, slot } => {
                let value =
                    load_slot_numeric(current_program!(), current_slots!(), slot, "LoadSlotI64")?;
                set_reg!(dst, value);
            }
            RegisterInstr::StoreSlotI64 { slot, src } => {
                let value = value_as_i64(reg!(src)?)?;
                store_slot(current_slots_mut!(), slot, Value::I64(value))?;
            }
            RegisterInstr::LoadSlotF64 { dst, slot } => {
                let value = load_slot_f64(current_program!(), current_slots!(), slot)?;
                set_reg!(dst, value);
            }
            RegisterInstr::StoreSlotF64 { slot, src } => {
                let value = value_as_f64(reg!(src)?, host)?;
                store_slot(current_slots_mut!(), slot, Value::F64(value))?;
            }
            RegisterInstr::LoadSlotI64ToF64 { dst, slot } => {
                let value =
                    load_slot_numeric(current_program!(), current_slots!(), slot, "LoadSlotI64")?;
                let converted = numeric_value_to_f64(&value).ok_or_else(|| {
                    VmError::InternalError(format!(
                        "LoadSlotI64ToF64: expected numeric, got {value:?}"
                    ))
                })?;
                set_reg!(dst, Value::F64(converted));
            }
            RegisterInstr::LoadSlotAny { dst, slot } => {
                let value = match current_slots!().get(slot) {
                    Some(Some(value)) => value.clone(),
                    Some(None) => {
                        return Err(VmError::UndefVarError(current_program!().slot_name(slot)));
                    }
                    None => {
                        return Err(VmError::InternalError(format!(
                            "LoadSlot: slot out of bounds: {slot}"
                        )));
                    }
                };
                set_reg!(dst, value);
            }
            RegisterInstr::StoreSlotAny { slot, src } => {
                let value = reg!(src)?.clone();
                let value = host.normalize_for_slot_storage(value);
                store_slot(current_slots_mut!(), slot, value)?;
            }

            RegisterInstr::BinI64 { op, dst, lhs, rhs } => {
                let b = value_as_i64(reg!(rhs)?)?;
                if matches!(op, I64BinOp::Rem) && b == 0 {
                    return Err(VmError::DivisionByZero);
                }
                let a = value_as_i64(reg!(lhs)?)?;
                let result = match op {
                    I64BinOp::Add => a.wrapping_add(b),
                    I64BinOp::Sub => a.wrapping_sub(b),
                    I64BinOp::Mul => a.wrapping_mul(b),
                    // wrapping_rem: rem(typemin(Int64), -1) == 0 in Julia; a plain
                    // `%` panics on the i64::MIN % -1 overflow (Issue #9429).
                    I64BinOp::Rem => a.wrapping_rem(b),
                };
                set_reg!(dst, Value::I64(result));
            }
            RegisterInstr::NegI64 { dst, src } => {
                let a = value_as_i64(reg!(src)?)?;
                set_reg!(dst, Value::I64(a.wrapping_neg()));
            }
            RegisterInstr::AddConstI64 { dst, src, value } => {
                let a = value_as_i64(reg!(src)?)?;
                set_reg!(dst, Value::I64(a.wrapping_add(value)));
            }
            RegisterInstr::AddConstI64Slot { slot, delta } => {
                match current_slots_mut!().get_mut(slot) {
                    Some(Some(Value::I64(value))) => {
                        *value = value.wrapping_add(delta);
                    }
                    Some(Some(_)) => {
                        return Err(VmError::InternalError(
                            "AddConstI64Slot: expected I64".to_string(),
                        ));
                    }
                    Some(None) => {
                        return Err(VmError::UndefVarError(current_program!().slot_name(slot)));
                    }
                    None => {
                        return Err(VmError::InternalError(format!(
                            "AddConstI64Slot: slot out of bounds: {slot}"
                        )));
                    }
                }
            }
            RegisterInstr::BinF64 { op, dst, lhs, rhs } => {
                let b = value_as_f64(reg!(rhs)?, host)?;
                let a = value_as_f64(reg!(lhs)?, host)?;
                set_reg!(dst, Value::F64(eval_f64_binop(op, a, b)));
            }
            RegisterInstr::BinF64Slot { op, dst, src, slot } => {
                let b = slot_f64_for_op(current_program!(), current_slots!(), slot)?;
                let a = value_as_f64(reg!(src)?, host)?;
                set_reg!(dst, Value::F64(eval_f64_binop(op, a, b)));
            }
            RegisterInstr::BinF64Slots {
                op,
                dst,
                lhs_slot,
                rhs_slot,
            } => {
                let a = slot_f64_for_op(current_program!(), current_slots!(), lhs_slot)?;
                let b = slot_f64_for_op(current_program!(), current_slots!(), rhs_slot)?;
                set_reg!(dst, Value::F64(eval_f64_binop(op, a, b)));
            }
            RegisterInstr::BinF64Const {
                op,
                dst,
                src,
                value,
            } => {
                let a = value_as_f64(reg!(src)?, host)?;
                set_reg!(dst, Value::F64(eval_f64_binop(op, a, value)));
            }
            RegisterInstr::BinF64ConstLeft {
                op,
                dst,
                value,
                src,
            } => {
                let b = value_as_f64(reg!(src)?, host)?;
                set_reg!(dst, Value::F64(eval_f64_binop(op, value, b)));
            }
            RegisterInstr::BinF64SlotConst {
                op,
                dst,
                slot,
                value,
                const_on_left,
            } => {
                let slot_value = slot_f64_for_op(current_program!(), current_slots!(), slot)?;
                let (a, b) = if const_on_left {
                    (value, slot_value)
                } else {
                    (slot_value, value)
                };
                set_reg!(dst, Value::F64(eval_f64_binop(op, a, b)));
            }
            RegisterInstr::BinF64StoreSlot { op, slot, lhs, rhs } => {
                let b = value_as_f64(reg!(rhs)?, host)?;
                let a = value_as_f64(reg!(lhs)?, host)?;
                store_slot(
                    current_slots_mut!(),
                    slot,
                    Value::F64(eval_f64_binop(op, a, b)),
                )?;
            }
            RegisterInstr::BinF64ConstLeftStoreSlot {
                op,
                slot,
                value,
                rhs,
            } => {
                let b = value_as_f64(reg!(rhs)?, host)?;
                store_slot(
                    current_slots_mut!(),
                    slot,
                    Value::F64(eval_f64_binop(op, value, b)),
                )?;
            }
            RegisterInstr::BinF64ConstLeftBinSlotsStoreSlot {
                outer_op,
                inner_op,
                slot,
                value,
                lhs_slot,
                rhs_slot,
            } => {
                let lhs = slot_f64_for_op(current_program!(), current_slots!(), lhs_slot)?;
                let rhs = slot_f64_for_op(current_program!(), current_slots!(), rhs_slot)?;
                let inner = eval_f64_binop(inner_op, lhs, rhs);
                store_slot(
                    current_slots_mut!(),
                    slot,
                    Value::F64(eval_f64_binop(outer_op, value, inner)),
                )?;
            }
            RegisterInstr::NegF64 { dst, src } => {
                let a = value_as_f64(reg!(src)?, host)?;
                set_reg!(dst, Value::F64(-a));
            }
            RegisterInstr::NegF64Slot { slot } => {
                let value = slot_f64_for_op(current_program!(), current_slots!(), slot)?;
                store_slot(current_slots_mut!(), slot, Value::F64(-value))?;
            }

            RegisterInstr::CmpI64 { op, dst, lhs, rhs } => {
                let b = value_as_i64(reg!(rhs)?)?;
                let a = value_as_i64(reg!(lhs)?)?;
                set_reg!(dst, Value::Bool(op.eval_i64(a, b)));
            }
            RegisterInstr::CmpF64 { op, dst, lhs, rhs } => {
                let b = value_as_f64(reg!(rhs)?, host)?;
                let a = value_as_f64(reg!(lhs)?, host)?;
                set_reg!(dst, Value::Bool(op.eval_f64(a, b)));
            }

            RegisterInstr::I64ToF64 { dst, src } => {
                let value = reg!(src)?;
                let converted = numeric_value_to_f64(value).ok_or_else(|| {
                    VmError::TypeError(format!("ToF64: expected numeric, got {value:?}"))
                })?;
                set_reg!(dst, Value::F64(converted));
            }
            RegisterInstr::F64ToI64 { dst, src } => {
                let value = reg!(src)?;
                let converted = match value {
                    Value::F64(v) => *v as i64,
                    Value::F32(v) => *v as i64,
                    Value::F16(v) => v.to_f64() as i64,
                    other => value_as_i64(other)?,
                };
                set_reg!(dst, Value::I64(converted));
            }
            RegisterInstr::BoolToI64 { dst, src } => {
                let value = match reg!(src)? {
                    Value::Bool(b) => Value::I64(if *b { 1 } else { 0 }),
                    Value::I64(v) => Value::I64(*v),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "BoolToI64: expected Bool or I64, got {other:?}"
                        )));
                    }
                };
                set_reg!(dst, value);
            }
            RegisterInstr::I64ToBool { dst, src } => {
                let value = match reg!(src)? {
                    Value::I64(v) => Value::Bool(*v != 0),
                    Value::Bool(b) => Value::Bool(*b),
                    other => {
                        return Err(VmError::TypeError(format!(
                            "I64ToBool: expected I64 or Bool, got {other:?}"
                        )));
                    }
                };
                set_reg!(dst, value);
            }
            RegisterInstr::NotBool { dst, src } => {
                let value = match reg!(src)? {
                    Value::Bool(b) => Value::Bool(!*b),
                    Value::Missing => Value::Missing,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "NotBool: expected Bool, got {other:?}"
                        )));
                    }
                };
                set_reg!(dst, value);
            }
            RegisterInstr::Move { dst, src } => {
                let value = reg!(src)?.clone();
                set_reg!(dst, value);
            }

            RegisterInstr::Jump { target } => {
                frames
                    .last_mut()
                    .ok_or_else(|| {
                        VmError::InternalError("register VM: no active frame".to_string())
                    })?
                    .pc = target;
            }
            RegisterInstr::JumpIfFalse { src, target } => {
                let value = reg!(src)?;
                let cond = match value {
                    Value::Bool(b) => *b,
                    other => {
                        return Err(VmError::TypeError(format!(
                            "non-boolean ({}) used in boolean context",
                            host.bool_context_type_name(other)
                        )));
                    }
                };
                if !cond {
                    frames
                        .last_mut()
                        .ok_or_else(|| {
                            VmError::InternalError("register VM: no active frame".to_string())
                        })?
                        .pc = target;
                }
            }
            RegisterInstr::BranchCmpI64 {
                op,
                lhs,
                rhs,
                target,
            } => {
                let b = value_as_i64(reg!(rhs)?)?;
                let a = value_as_i64(reg!(lhs)?)?;
                if op.eval_i64(a, b) {
                    frames
                        .last_mut()
                        .ok_or_else(|| {
                            VmError::InternalError("register VM: no active frame".to_string())
                        })?
                        .pc = target;
                }
            }
            RegisterInstr::BranchCmpF64 {
                op,
                lhs,
                rhs,
                target,
            } => {
                let b = value_as_f64(reg!(rhs)?, host)?;
                let a = value_as_f64(reg!(lhs)?, host)?;
                if op.should_jump(a, b) {
                    frames
                        .last_mut()
                        .ok_or_else(|| {
                            VmError::InternalError("register VM: no active frame".to_string())
                        })?
                        .pc = target;
                }
            }
            RegisterInstr::BranchGtI64Slots {
                lhs_slot,
                rhs_slot,
                target,
            } => {
                let lhs = slot_i64_for_jump(current_program!(), current_slots!(), lhs_slot)?;
                let rhs = slot_i64_for_jump(current_program!(), current_slots!(), rhs_slot)?;
                if lhs > rhs {
                    frames
                        .last_mut()
                        .ok_or_else(|| {
                            VmError::InternalError("register VM: no active frame".to_string())
                        })?
                        .pc = target;
                }
            }
            RegisterInstr::BranchCmpI64Slots {
                op,
                lhs_slot,
                rhs_slot,
                target,
            } => {
                let lhs = slot_i64_for_jump(current_program!(), current_slots!(), lhs_slot)?;
                let rhs = slot_i64_for_jump(current_program!(), current_slots!(), rhs_slot)?;
                if op.eval_i64(lhs, rhs) {
                    frames
                        .last_mut()
                        .ok_or_else(|| {
                            VmError::InternalError("register VM: no active frame".to_string())
                        })?
                        .pc = target;
                }
            }
            RegisterInstr::AddConstI64SlotBranchLe {
                slot,
                delta,
                stop_slot,
                target,
            } => {
                let updated = match current_slots_mut!().get_mut(slot) {
                    Some(Some(Value::I64(value))) => {
                        *value = value.wrapping_add(delta);
                        *value
                    }
                    Some(Some(_)) => {
                        return Err(VmError::InternalError(
                            "AddConstI64SlotAndJumpIfLe: expected I64".to_string(),
                        ));
                    }
                    Some(None) => {
                        return Err(VmError::UndefVarError(current_program!().slot_name(slot)));
                    }
                    None => {
                        return Err(VmError::InternalError(format!(
                            "AddConstI64SlotAndJumpIfLe: slot out of bounds: {slot}"
                        )));
                    }
                };
                let stop = slot_i64_for_jump(current_program!(), current_slots!(), stop_slot)?;
                if updated <= stop {
                    frames
                        .last_mut()
                        .ok_or_else(|| {
                            VmError::InternalError("register VM: no active frame".to_string())
                        })?
                        .pc = target;
                }
            }
            RegisterInstr::LoopI64Slots { block } => {
                let loop_block = {
                    let frame = frames.last().ok_or_else(|| {
                        VmError::InternalError("register VM: no active frame".to_string())
                    })?;
                    frame
                        .program
                        .loop_blocks
                        .get(block)
                        .cloned()
                        .ok_or_else(|| {
                            VmError::InternalError(format!(
                                "register VM: loop block {block} out of bounds"
                            ))
                        })?
                };
                let frame = frames.last_mut().ok_or_else(|| {
                    VmError::InternalError("register VM: no active frame".to_string())
                })?;
                execute_register_loop_block(&loop_block, frame)?;
                frame.pc = loop_block.exit_pc;
            }

            RegisterInstr::CallStack {
                func_index,
                args_start,
                arg_count,
                dst,
                inbounds,
            } => {
                let args = {
                    let frame = frames.last_mut().ok_or_else(|| {
                        VmError::InternalError("register VM: no active frame".to_string())
                    })?;
                    take_args(&mut frame.registers, args_start, arg_count)?
                };
                if let Some(call_frame) =
                    host.prepare_register_call_frame(func_index, &args, inbounds)?
                {
                    if frames.len() >= MAX_REGISTER_CALL_DEPTH {
                        return Err(VmError::StackOverflow);
                    }
                    frames.push(ActiveRegisterFrame::new(
                        call_frame.program,
                        call_frame.slots,
                        Some(dst),
                    ));
                } else {
                    let value = host.call_function(func_index, args, inbounds)?;
                    set_reg!(dst, value);
                }
            }
            RegisterInstr::CallIntrinsic {
                intrinsic,
                args_start,
                arg_count,
                dst,
            } => {
                let args = {
                    let frame = frames.last_mut().ok_or_else(|| {
                        VmError::InternalError("register VM: no active frame".to_string())
                    })?;
                    take_args(&mut frame.registers, args_start, arg_count)?
                };
                let value = host.call_intrinsic(intrinsic, args)?;
                set_reg!(dst, value);
            }

            RegisterInstr::Return { kind, src } => {
                let value = match kind {
                    ReturnKind::Nothing => Value::Nothing,
                    ReturnKind::Any => reg!(src)?.clone(),
                    ReturnKind::I64 => {
                        let value = reg!(src)?;
                        if is_integer_family(value) {
                            value.clone()
                        } else {
                            return Err(VmError::InternalError(format!(
                                "ReturnI64: expected integer, got {value:?}"
                            )));
                        }
                    }
                    ReturnKind::F64 => Value::F64(value_as_f64(reg!(src)?, host)?),
                };
                let completed = frames.pop().ok_or_else(|| {
                    VmError::InternalError("register VM: return without active frame".to_string())
                })?;
                register_call_count += 1;
                if let Some(dst) = completed.return_dst {
                    let caller = frames.last_mut().ok_or_else(|| {
                        VmError::InternalError(
                            "register VM: callee returned without caller frame".to_string(),
                        )
                    })?;
                    let slot = caller.registers.get_mut(dst).ok_or_else(|| {
                        VmError::InternalError(format!(
                            "register VM: return destination register {dst} out of bounds"
                        ))
                    })?;
                    *slot = Some(value);
                } else {
                    slots.clone_from_slice(&completed.slots);
                    return Ok(RegisterRunOutcome {
                        value,
                        dispatch_count,
                        register_call_count,
                    });
                }
            }
        }
    }

    Err(VmError::InternalError(
        "register VM prototype reached end of program without return".to_string(),
    ))
}

fn execute_register_loop_block(
    block: &RegisterLoopBlock,
    frame: &mut ActiveRegisterFrame,
) -> Result<(), VmError> {
    let mut f64_slots = Vec::with_capacity(block.f64_slots.len());
    for (idx, slot) in block.f64_slots.iter().copied().enumerate() {
        if block.f64_live_in[idx] {
            f64_slots.push(slot_f64_for_op(frame.program.as_ref(), &frame.slots, slot)?);
        } else {
            f64_slots.push(0.0);
        }
    }
    let mut i64_slots = Vec::with_capacity(block.i64_slots.len());
    for slot in &block.i64_slots {
        i64_slots.push(slot_i64_for_jump(
            frame.program.as_ref(),
            &frame.slots,
            *slot,
        )?);
    }
    let mut f64_registers = vec![0.0; frame.registers.len()];
    let mut iterations = 0usize;
    loop {
        let lhs = i64_slots[block.lhs_i64_index];
        let rhs = i64_slots[block.rhs_i64_index];
        if block.exit_op.eval_i64(lhs, rhs) {
            break;
        }

        for op in &block.ops {
            execute_register_loop_op(op, &mut f64_slots, &mut i64_slots, &mut f64_registers);
        }

        iterations = iterations.wrapping_add(1);
        if iterations.is_multiple_of(CANCEL_CHECK_INTERVAL) && crate::cancel::is_requested() {
            return Err(VmError::Cancelled);
        }
    }
    for (idx, slot) in block.f64_slots.iter().copied().enumerate() {
        store_slot(&mut frame.slots, slot, Value::F64(f64_slots[idx]))?;
    }
    for (idx, slot) in block.i64_slots.iter().copied().enumerate() {
        store_slot(&mut frame.slots, slot, Value::I64(i64_slots[idx]))?;
    }
    Ok(())
}

fn execute_register_loop_op(
    op: &RegisterLoopOp,
    f64_slots: &mut [f64],
    i64_slots: &mut [i64],
    f64_registers: &mut [f64],
) {
    match op {
        RegisterLoopOp::LoadSlotF64 { dst, source } => {
            f64_registers[*dst] = loop_f64_source_value(*source, f64_slots, i64_slots);
        }
        RegisterLoopOp::StoreSlotF64 { slot_index, src } => {
            f64_slots[*slot_index] = f64_registers[*src];
        }
        RegisterLoopOp::BinF64 { op, dst, lhs, rhs } => {
            let b = f64_registers[*rhs];
            let a = f64_registers[*lhs];
            f64_registers[*dst] = eval_f64_binop(*op, a, b);
        }
        RegisterLoopOp::BinF64Const {
            op,
            dst,
            src,
            value,
        } => {
            let a = f64_registers[*src];
            f64_registers[*dst] = eval_f64_binop(*op, a, *value);
        }
        RegisterLoopOp::BinF64Slots { op, dst, lhs, rhs } => {
            let a = loop_f64_source_value(*lhs, f64_slots, i64_slots);
            let b = loop_f64_source_value(*rhs, f64_slots, i64_slots);
            f64_registers[*dst] = eval_f64_binop(*op, a, b);
        }
        RegisterLoopOp::BinF64SlotConst {
            op,
            dst,
            source,
            value,
            const_on_left,
        } => {
            let slot_value = loop_f64_source_value(*source, f64_slots, i64_slots);
            let (a, b) = if *const_on_left {
                (*value, slot_value)
            } else {
                (slot_value, *value)
            };
            f64_registers[*dst] = eval_f64_binop(*op, a, b);
        }
        RegisterLoopOp::BinF64StoreSlot {
            op,
            slot_index,
            lhs,
            rhs,
        } => {
            let b = f64_registers[*rhs];
            let a = f64_registers[*lhs];
            f64_slots[*slot_index] = eval_f64_binop(*op, a, b);
        }
        RegisterLoopOp::BinF64ConstLeftStoreSlot {
            op,
            slot_index,
            value,
            rhs,
        } => {
            let b = f64_registers[*rhs];
            f64_slots[*slot_index] = eval_f64_binop(*op, *value, b);
        }
        RegisterLoopOp::BinF64ConstLeftBinSlotsStoreSlot {
            outer_op,
            inner_op,
            slot_index,
            value,
            lhs,
            rhs,
        } => {
            let lhs = loop_f64_source_value(*lhs, f64_slots, i64_slots);
            let rhs = loop_f64_source_value(*rhs, f64_slots, i64_slots);
            let inner = eval_f64_binop(*inner_op, lhs, rhs);
            f64_slots[*slot_index] = eval_f64_binop(*outer_op, *value, inner);
        }
        RegisterLoopOp::NegF64Slot { slot_index, source } => {
            let value = loop_f64_source_value(*source, f64_slots, i64_slots);
            f64_slots[*slot_index] = -value;
        }
        RegisterLoopOp::AddConstI64Slot { slot_index, delta } => {
            i64_slots[*slot_index] = i64_slots[*slot_index].wrapping_add(*delta);
        }
        RegisterLoopOp::CopyF64Slot { slot_index, source } => {
            f64_slots[*slot_index] = loop_f64_source_value(*source, f64_slots, i64_slots);
        }
        RegisterLoopOp::BinF64SourcesStoreSlot {
            op,
            slot_index,
            lhs,
            rhs,
        } => {
            let lhs = loop_f64_source_value(*lhs, f64_slots, i64_slots);
            let rhs = loop_f64_source_value(*rhs, f64_slots, i64_slots);
            f64_slots[*slot_index] = eval_f64_binop(*op, lhs, rhs);
        }
        RegisterLoopOp::BinF64SourceSlotConstStoreSlot {
            outer_op,
            inner_op,
            slot_index,
            lhs,
            rhs,
            value,
            const_on_left,
        } => {
            let lhs = loop_f64_source_value(*lhs, f64_slots, i64_slots);
            let rhs = loop_f64_source_value(*rhs, f64_slots, i64_slots);
            let inner = if *const_on_left {
                eval_f64_binop(*inner_op, *value, rhs)
            } else {
                eval_f64_binop(*inner_op, rhs, *value)
            };
            f64_slots[*slot_index] = eval_f64_binop(*outer_op, lhs, inner);
        }
        RegisterLoopOp::BinF64SlotsSlotConstStoreSlot {
            outer_op,
            slots_op,
            slot_const_op,
            slot_index,
            lhs,
            rhs,
            slot_const_source,
            value,
            const_on_left,
        } => {
            let lhs = loop_f64_source_value(*lhs, f64_slots, i64_slots);
            let rhs = loop_f64_source_value(*rhs, f64_slots, i64_slots);
            let left = eval_f64_binop(*slots_op, lhs, rhs);
            let slot_const = loop_f64_source_value(*slot_const_source, f64_slots, i64_slots);
            let right = if *const_on_left {
                eval_f64_binop(*slot_const_op, *value, slot_const)
            } else {
                eval_f64_binop(*slot_const_op, slot_const, *value)
            };
            f64_slots[*slot_index] = eval_f64_binop(*outer_op, left, right);
        }
        RegisterLoopOp::BinF64SourceSlotConstSourceStoreSlot {
            outer_op,
            middle_op,
            inner_op,
            slot_index,
            lhs,
            inner_source,
            value,
            inner_const_on_left,
            tail,
        } => {
            let lhs = loop_f64_source_value(*lhs, f64_slots, i64_slots);
            let inner_source = loop_f64_source_value(*inner_source, f64_slots, i64_slots);
            let inner = if *inner_const_on_left {
                eval_f64_binop(*inner_op, *value, inner_source)
            } else {
                eval_f64_binop(*inner_op, inner_source, *value)
            };
            let middle = eval_f64_binop(*middle_op, lhs, inner);
            let tail = loop_f64_source_value(*tail, f64_slots, i64_slots);
            f64_slots[*slot_index] = eval_f64_binop(*outer_op, middle, tail);
        }
    }
}

fn loop_f64_source_value(source: LoopF64Source, f64_slots: &[f64], i64_slots: &[i64]) -> f64 {
    match source {
        LoopF64Source::F64Slot(idx) => f64_slots[idx],
        LoopF64Source::I64Slot(idx) => i64_slots[idx] as f64,
    }
}

fn eval_f64_binop(op: F64BinOp, a: f64, b: f64) -> f64 {
    match op {
        F64BinOp::Add => a + b,
        F64BinOp::Sub => a - b,
        F64BinOp::Mul => a * b,
        F64BinOp::Div => a / b,
        F64BinOp::Pow => crate::vm::intrinsics_exec::pow_f64(a, b),
    }
}

fn take_args(
    registers: &mut [Option<Value>],
    args_start: usize,
    arg_count: usize,
) -> Result<Vec<Value>, VmError> {
    let mut args = Vec::with_capacity(arg_count);
    for idx in args_start..args_start + arg_count {
        let value = registers
            .get_mut(idx)
            .and_then(Option::take)
            .ok_or_else(|| {
                VmError::InternalError(format!("register VM: call argument register {idx} unset"))
            })?;
        args.push(value);
    }
    Ok(args)
}

fn store_slot(slots: &mut [Option<Value>], slot: usize, value: Value) -> Result<(), VmError> {
    let entry = slots.get_mut(slot).ok_or_else(|| {
        VmError::InternalError(format!("register VM: slot out of bounds: {slot}"))
    })?;
    *entry = Some(value);
    Ok(())
}

/// Stack `pop_i64` parity: exact `I64`, `Bool`, and narrow/wide integer
/// values widen to `i64`; anything else is the same `TypeError`.
fn value_as_i64(value: &Value) -> Result<i64, VmError> {
    match value {
        Value::I64(v) => Ok(*v),
        Value::Bool(v) => Ok(if *v { 1 } else { 0 }),
        Value::I32(v) => Ok(*v as i64),
        Value::I16(v) => Ok(*v as i64),
        Value::I8(v) => Ok(*v as i64),
        Value::I128(v) => Ok(*v as i64),
        Value::U8(v) => Ok(*v as i64),
        Value::U16(v) => Ok(*v as i64),
        Value::U32(v) => Ok(*v as i64),
        Value::U64(v) => Ok(*v as i64),
        Value::U128(v) => Ok(*v as i64),
        other => Err(VmError::TypeError(format!(
            "expected I64, got {:?}",
            crate::vm::util::value_type_name(other)
        ))),
    }
}

/// Pure arms of the stack VM's `pop_f64_or_i64` conversion.
fn numeric_value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::F64(v) => Some(*v),
        Value::F32(v) => Some(*v as f64),
        Value::F16(v) => Some(v.to_f64()),
        Value::I64(v) => Some(*v as f64),
        Value::I128(v) => Some(*v as f64),
        Value::I32(v) => Some(*v as f64),
        Value::I16(v) => Some(*v as f64),
        Value::I8(v) => Some(*v as f64),
        Value::U64(v) => Some(*v as f64),
        Value::U128(v) => Some(*v as f64),
        Value::U32(v) => Some(*v as f64),
        Value::U16(v) => Some(*v as f64),
        Value::U8(v) => Some(*v as f64),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Stack `pop_f64_or_i64` parity: pure numeric conversions inline, anything
/// else (BigInt, Rational/Irrational structs) through the host.
fn value_as_f64(value: &Value, host: &mut dyn RegisterVmHost) -> Result<f64, VmError> {
    match numeric_value_to_f64(value) {
        Some(v) => Ok(v),
        None => host.value_to_f64_slow(value),
    }
}

fn is_integer_family(value: &Value) -> bool {
    matches!(
        value,
        Value::I64(_)
            | Value::Bool(_)
            | Value::I32(_)
            | Value::I16(_)
            | Value::I8(_)
            | Value::I128(_)
            | Value::U8(_)
            | Value::U16(_)
            | Value::U32(_)
            | Value::U64(_)
            | Value::U128(_)
            | Value::BigInt(_)
    )
}

fn is_numeric_family(value: &Value) -> bool {
    is_integer_family(value) || matches!(value, Value::F16(_) | Value::F32(_) | Value::F64(_))
}

/// Stack `LoadSlotI64` parity: numeric slot values load unwidened.
fn load_slot_numeric(
    program: &RegisterProgram,
    slots: &[Option<Value>],
    slot: usize,
    instr_name: &str,
) -> Result<Value, VmError> {
    match slots.get(slot) {
        Some(Some(value)) if is_numeric_family(value) && !matches!(value, Value::BigInt(_)) => {
            Ok(value.clone())
        }
        Some(Some(value)) => Err(VmError::InternalError(format!(
            "{instr_name}: expected numeric in {}, got {value:?}",
            program.slot_name(slot)
        ))),
        Some(None) => Err(VmError::UndefVarError(program.slot_name(slot))),
        None => Err(VmError::InternalError(format!(
            "{instr_name}: slot out of bounds: {slot}"
        ))),
    }
}

/// Stack `LoadSlotF64` parity: `F64` direct, `F16`/`F32` pass through,
/// integer family widens to `F64`.
fn load_slot_f64(
    program: &RegisterProgram,
    slots: &[Option<Value>],
    slot: usize,
) -> Result<Value, VmError> {
    match slots.get(slot) {
        Some(Some(Value::F64(v))) => Ok(Value::F64(*v)),
        Some(Some(value @ (Value::F16(_) | Value::F32(_)))) => Ok(value.clone()),
        Some(Some(value)) if is_integer_family(value) && !matches!(value, Value::BigInt(_)) => {
            numeric_value_to_f64(value).map(Value::F64).ok_or_else(|| {
                VmError::InternalError("LoadSlotF64: expected F64-compatible value".to_string())
            })
        }
        Some(Some(_)) => Err(VmError::InternalError(
            "LoadSlotF64: expected F64-compatible value".to_string(),
        )),
        Some(None) => Err(VmError::UndefVarError(program.slot_name(slot))),
        None => Err(VmError::InternalError(format!(
            "LoadSlotF64: slot out of bounds: {slot}"
        ))),
    }
}

/// Stack `slot_f64_for_op` parity for the fused F64 slot operand.
fn slot_f64_for_op(
    program: &RegisterProgram,
    slots: &[Option<Value>],
    slot: usize,
) -> Result<f64, VmError> {
    match slots.get(slot) {
        Some(Some(value)) => numeric_value_to_f64(value).ok_or_else(|| {
            VmError::TypeError(format!(
                "expected F64-compatible value in {}, got {value:?}",
                program.slot_name(slot)
            ))
        }),
        Some(None) => Err(VmError::UndefVarError(program.slot_name(slot))),
        None => Err(VmError::InternalError(format!(
            "slot out of bounds: {slot}"
        ))),
    }
}

/// Stack `load_i64_slot_for_jump` parity: integer-family slot values widen,
/// floats are a `TypeError`, unset slots raise `UndefVarError`.
fn slot_i64_for_jump(
    program: &RegisterProgram,
    slots: &[Option<Value>],
    slot: usize,
) -> Result<i64, VmError> {
    match slots.get(slot) {
        Some(Some(value)) if is_integer_family(value) && !matches!(value, Value::BigInt(_)) => {
            value_as_i64(value)
        }
        Some(Some(value @ (Value::F16(_) | Value::F32(_) | Value::F64(_)))) => {
            Err(VmError::TypeError(format!(
                "expected I64, got {:?}",
                crate::vm::util::value_type_name(value)
            )))
        }
        Some(Some(value)) => Err(VmError::InternalError(format!(
            "expected numeric in {}, got {value:?}",
            program.slot_name(slot)
        ))),
        Some(None) => Err(VmError::UndefVarError(program.slot_name(slot))),
        None => Err(VmError::InternalError(format!(
            "slot out of bounds: {slot}"
        ))),
    }
}

// ===================== Standalone runner =====================

pub struct RegisterVm<R: RngLike> {
    program: RegisterProgram,
    slots: Vec<Option<Value>>,
    dispatch_count: usize,
    _rng: R,
}

impl<R: RngLike> RegisterVm<R> {
    pub fn new(program: RegisterProgram, rng: R) -> Self {
        Self {
            slots: vec![None; program.slot_count],
            program,
            dispatch_count: 0,
            _rng: rng,
        }
    }

    pub fn dispatch_count(&self) -> usize {
        self.dispatch_count
    }

    /// Run without a stack VM host: calls/intrinsics error explicitly.
    pub fn run(&mut self) -> Result<Value, VmError> {
        self.run_with_host(&mut NoStackHost)
    }

    pub fn run_with_host(&mut self, host: &mut dyn RegisterVmHost) -> Result<Value, VmError> {
        let outcome = execute_register_program(&self.program, &mut self.slots, host)?;
        self.dispatch_count += outcome.dispatch_count;
        Ok(outcome.value)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use subset_julia_vm_bytecode::ValueType;

    fn assert_i64(value: &Value, expected: i64) {
        match value {
            Value::I64(v) => assert_eq!(*v, expected),
            other => panic!("expected I64({expected}), got {other:?}"),
        }
    }

    fn assert_f64(value: &Value, expected: f64) {
        match value {
            Value::F64(v) => assert_eq!(*v, expected),
            other => panic!("expected F64({expected}), got {other:?}"),
        }
    }

    fn func_info(code_len: usize, slot_count: usize, slot_names: Vec<&str>) -> FunctionInfo {
        FunctionInfo {
            name: "test_fn".to_string(),
            params: Vec::new(),
            kwparams: Vec::new(),
            entry: 0,
            return_type: ValueType::Any,
            return_julia_type: None,
            is_base_extension: false,
            is_generated: false,
            is_lowering_helper: false,
            definition_order: 0,
            min_world: 0,
            type_params: Vec::new(),
            param_julia_types: Vec::new(),
            code_start: 0,
            code_end: code_len,
            slot_names: slot_names.into_iter().map(str::to_string).collect(),
            slot_types: Vec::new(),
            local_slot_count: slot_count,
            param_slots: Vec::new(),
            vararg_param_index: None,
            vararg_fixed_count: None,
            inlining_meta: 0,
            constprop_meta: 0,
            nospecialize_meta: 0,
            propagate_inbounds_meta: false,
            nospecializeinfer_meta: false,
            purity_meta: 0,
            direct_return_type_param: None,
            def_line: 0,
            suppress_short_name_alias: false,
            shared_plan: None,
        }
    }

    /// Loop fixture: `s = 0; i = 0; while i < 5 { s += i; i += 1 }; return s`.
    #[test]
    fn lowers_and_runs_counted_i64_loop() {
        let code = vec![
            Instr::PushI64(0),
            Instr::StoreSlotI64(0), // s
            Instr::PushI64(0),
            Instr::StoreSlotI64(1), // i
            // header (ip 4)
            Instr::LoadSlotI64(1),
            Instr::PushI64(5),
            Instr::JumpIfGeI64(13),
            // body
            Instr::LoadSlotI64(0),
            Instr::LoadSlotI64(1),
            Instr::AddI64,
            Instr::StoreSlotI64(0),
            Instr::AddConstI64Slot(1, 1),
            Instr::Jump(4),
            // exit (ip 13)
            Instr::LoadSlotI64(0),
            Instr::ReturnI64,
        ];
        let func = func_info(code.len(), 2, vec!["s", "i"]);
        let program = RegisterProgram::from_stack_function(&code, &func)
            .expect("lower counted loop to registers");
        let mut vm = RegisterVm::new(program, crate::rng::StableRng::new(0));
        let result = vm.run().expect("run counted loop");
        assert_i64(&result, 10);
        assert!(vm.dispatch_count() > code.len(), "loop must re-dispatch");
    }

    /// Branch merge fixture: `if a > b { x = a } else { x = b }; return x`
    /// exercises depth-consistent merges from both arms.
    #[test]
    fn lowers_branch_merge_with_consistent_depths() {
        let code = vec![
            Instr::LoadSlotI64(0),
            Instr::PushI64(3),
            Instr::JumpIfLeI64(6),
            Instr::LoadSlotI64(0),
            Instr::ReturnI64,
            Instr::Jump(6), // dead code after return (mirrors compiler output)
            Instr::PushI64(-1),
            Instr::ReturnI64,
        ];
        let func = func_info(code.len(), 1, vec!["a"]);
        let program =
            RegisterProgram::from_stack_function(&code, &func).expect("lower branch merge");
        let mut vm = RegisterVm::new(program.clone(), crate::rng::StableRng::new(0));
        vm.slots[0] = Some(Value::I64(9));
        assert_i64(&vm.run().expect("run branch"), 9);

        let mut vm = RegisterVm::new(program, crate::rng::StableRng::new(0));
        vm.slots[0] = Some(Value::I64(2));
        assert_i64(&vm.run().expect("run branch"), -1);
    }

    #[test]
    fn f64_arithmetic_and_branches_run() {
        // acc = 0.0; while acc < 2.0 { acc = acc + 0.5 }; return acc
        let code = vec![
            Instr::PushF64(0.0),
            Instr::StoreSlotF64(0),
            // header (ip 2)
            Instr::LoadSlotF64(0),
            Instr::PushF64(2.0),
            Instr::JumpIfNotLtF64(10),
            // body
            Instr::LoadSlotF64(0),
            Instr::PushF64(0.5),
            Instr::AddF64,
            Instr::StoreSlotF64(0),
            Instr::Jump(2),
            // exit (ip 10)
            Instr::LoadSlotF64(0),
            Instr::ReturnF64,
        ];
        let func = func_info(code.len(), 1, vec!["acc"]);
        let program = RegisterProgram::from_stack_function(&code, &func).expect("lower f64 loop");
        let mut vm = RegisterVm::new(program, crate::rng::StableRng::new(0));
        assert_f64(&vm.run().expect("run f64 loop"), 2.0);
    }

    #[test]
    fn untranslatable_instruction_is_named_in_error() {
        let code = vec![Instr::PushStr("boom".to_string()), Instr::ReturnAny];
        let func = func_info(code.len(), 0, vec![]);
        let err = RegisterProgram::from_stack_function(&code, &func)
            .expect_err("PushStr must not translate");
        assert!(
            err.contains("PushStr"),
            "error must name the instruction: {err}"
        );
    }

    #[test]
    fn calls_error_without_stack_host() {
        let code = vec![
            Instr::PushI64(1),
            Instr::CallResolved(7, 1),
            Instr::ReturnAny,
        ];
        let func = func_info(code.len(), 0, vec![]);
        let program = RegisterProgram::from_stack_function(&code, &func).expect("lower call");
        let mut vm = RegisterVm::new(program, crate::rng::StableRng::new(0));
        let err = vm.run().expect_err("call without host must fail");
        assert!(matches!(err, VmError::InternalError(_)));
    }

    /// Calls trampoline through the host and land results in the destination
    /// register (mock host stands in for the stack VM).
    #[test]
    fn calls_trampoline_through_host() {
        struct DoublingHost {
            calls: usize,
        }
        impl RegisterVmHost for DoublingHost {
            fn call_function(
                &mut self,
                _func_index: usize,
                args: Vec<Value>,
                _inbounds: bool,
            ) -> Result<Value, VmError> {
                self.calls += 1;
                match args.as_slice() {
                    [Value::I64(v)] => Ok(Value::I64(v * 2)),
                    other => Err(VmError::InternalError(format!("unexpected args {other:?}"))),
                }
            }
            fn call_intrinsic(
                &mut self,
                _intrinsic: Intrinsic,
                _args: Vec<Value>,
            ) -> Result<Value, VmError> {
                Err(VmError::InternalError("no intrinsics".to_string()))
            }
        }

        // return f(20) + f(1)
        let code = vec![
            Instr::PushI64(20),
            Instr::CallResolved(3, 1),
            Instr::PushI64(1),
            Instr::CallResolved(3, 1),
            Instr::AddI64,
            Instr::ReturnI64,
        ];
        let func = func_info(code.len(), 0, vec![]);
        let program = RegisterProgram::from_stack_function(&code, &func).expect("lower calls");
        let mut vm = RegisterVm::new(program, crate::rng::StableRng::new(0));
        let mut host = DoublingHost { calls: 0 };
        assert_i64(&vm.run_with_host(&mut host).expect("run"), 42);
        assert_eq!(host.calls, 2);
    }

    #[test]
    fn lowers_shared_plan_identity_without_stack_instr_issue_9089() {
        use crate::compile::ssa_ir::{
            build_function,
            plan::{plan_function, NumericConvertGate},
        };
        use crate::compile::test_helpers::{var_expr, zero_span};
        use crate::ir::core::{Block, Function, Stmt, TypedParam};

        let span = zero_span();
        let func = Function {
            name: "register_shared_plan_identity_9089".to_string(),
            params: vec![TypedParam::untyped("x".to_string(), span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(var_expr("x")),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };
        let ssa = build_function(&func).expect("build SSA");
        let plan = plan_function(&ssa, span, NumericConvertGate::default()).expect("plan SSA");

        let program = RegisterProgram::from_shared_plan(
            &plan,
            1,
            Rc::new(vec!["x".to_string()]),
            "register_shared_plan_identity_9089".to_string(),
        )
        .expect("lower shared plan");
        assert!(
            !program
                .instructions()
                .iter()
                .any(|instr| matches!(instr, RegisterInstr::CallStack { .. })),
            "shared-plan lowering must not introduce a stack-bytecode trampoline"
        );

        let mut slots = vec![Some(Value::I64(41))];
        let outcome = execute_register_program(&program, &mut slots, &mut NoStackHost)
            .expect("run shared-plan identity");
        assert_i64(&outcome.value, 41);
    }

    #[test]
    fn lowers_shared_plan_call_with_function_index_issue_9089() {
        use crate::compile::ssa_ir::{
            build_function,
            plan::{plan_function, NumericConvertGate},
        };
        use crate::compile::test_helpers::{call_expr, var_expr, zero_span};
        use crate::ir::core::{Block, Function, Stmt, TypedParam};
        use std::collections::HashMap;

        struct EchoHost;
        impl RegisterVmHost for EchoHost {
            fn call_function(
                &mut self,
                func_index: usize,
                args: Vec<Value>,
                _inbounds: bool,
            ) -> Result<Value, VmError> {
                assert_eq!(func_index, 7);
                match args.as_slice() {
                    [Value::I64(v)] => Ok(Value::I64(*v + 1)),
                    other => Err(VmError::InternalError(format!("unexpected args {other:?}"))),
                }
            }

            fn call_intrinsic(
                &mut self,
                _intrinsic: Intrinsic,
                _args: Vec<Value>,
            ) -> Result<Value, VmError> {
                Err(VmError::InternalError("no intrinsics".to_string()))
            }
        }

        let span = zero_span();
        let func = Function {
            name: "register_shared_plan_call_9089".to_string(),
            params: vec![TypedParam::untyped("x".to_string(), span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(call_expr("callee9089", vec![var_expr("x")])),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
            new_struct_name: None,
        };
        let ssa = build_function(&func).expect("build SSA");
        let plan = plan_function(&ssa, span, NumericConvertGate::default()).expect("plan SSA");
        let function_indices = HashMap::from([("callee9089".to_string(), vec![7usize])]);

        let program = RegisterProgram::from_shared_plan_with_functions(
            &plan,
            1,
            Rc::new(vec!["x".to_string()]),
            "register_shared_plan_call_9089".to_string(),
            &function_indices,
        )
        .expect("lower shared-plan call");
        assert!(
            program
                .instructions()
                .iter()
                .any(|instr| matches!(instr, RegisterInstr::CallStack { func_index: 7, .. })),
            "shared-plan call must resolve to a register CallStack trampoline"
        );

        let mut slots = vec![Some(Value::I64(41))];
        let outcome = execute_register_program(&program, &mut slots, &mut EchoHost)
            .expect("run shared-plan call");
        assert_i64(&outcome.value, 42);
    }

    #[test]
    fn jump_into_unreachable_code_is_rejected() {
        let code = vec![
            Instr::Jump(3),
            Instr::PushI64(1), // unreachable, but targeted from ip 4
            Instr::ReturnI64,
            Instr::PushI64(0),
            Instr::JumpIfEqI64(1), // needs 2 operands AND targets unmapped ip
            Instr::ReturnNothing,
        ];
        let func = func_info(code.len(), 0, vec![]);
        assert!(RegisterProgram::from_stack_function(&code, &func).is_err());
    }

    #[test]
    fn loop_fusion_postpass_collapses_slot_update_ops_issue_9906() {
        let ops = vec![
            RegisterLoopOp::LoadSlotF64 {
                dst: 1,
                source: LoopF64Source::F64Slot(0),
            },
            RegisterLoopOp::BinF64SlotConst {
                op: F64BinOp::Mul,
                dst: 2,
                source: LoopF64Source::F64Slot(1),
                value: 0.5,
                const_on_left: true,
            },
            RegisterLoopOp::BinF64StoreSlot {
                op: F64BinOp::Add,
                slot_index: 2,
                lhs: 1,
                rhs: 2,
            },
        ];
        let fused = SharedPlanLowering::fuse_loop_ops(ops);
        assert!(
            matches!(
                fused.as_slice(),
                [RegisterLoopOp::BinF64SourceSlotConstStoreSlot { .. }]
            ),
            "slot update should fuse to one loop op: {fused:?}"
        );

        let mut f64_slots = vec![2.0, 4.0, 0.0];
        let mut i64_slots = Vec::new();
        let mut f64_registers = vec![0.0; 3];
        execute_register_loop_op(
            &fused[0],
            &mut f64_slots,
            &mut i64_slots,
            &mut f64_registers,
        );
        assert_eq!(f64_slots[2], 4.0);
    }
}
