//! Low-level SSA IR types for AoT compilation.
//!
//! Contains basic block, instruction, terminator, variable reference,
//! constant value, and IR function/module types.

use super::super::types::StaticType;
use super::aggregate_types::AggregateLayout;
use super::{AotBuiltinOp, ConstValue};
use std::fmt;

#[derive(Debug, Clone)]
pub struct BasicBlock {
    /// Block label/identifier
    pub label: String,
    /// Instructions in this block
    pub instructions: Vec<Instruction>,
    /// Terminator instruction
    pub terminator: Option<Terminator>,
}

impl BasicBlock {
    /// Create a new basic block
    pub fn new(label: String) -> Self {
        Self {
            label,
            instructions: Vec::new(),
            terminator: None,
        }
    }

    /// Add an instruction to the block
    pub fn push(&mut self, inst: Instruction) {
        self.instructions.push(inst);
    }

    /// Set the terminator
    pub fn set_terminator(&mut self, term: Terminator) {
        self.terminator = Some(term);
    }
}

/// IR instruction
#[derive(Debug, Clone)]
pub enum Instruction {
    /// Load a constant value
    LoadConst {
        dest: VarRef,
        value: ConstValue,
    },
    /// Copy a value
    Copy {
        dest: VarRef,
        src: VarRef,
    },
    /// Binary operation
    BinOp {
        dest: VarRef,
        op: BinOpKind,
        left: VarRef,
        right: VarRef,
    },
    /// Unary operation
    UnaryOp {
        dest: VarRef,
        op: UnaryOpKind,
        operand: VarRef,
    },
    Builtin {
        dest: VarRef,
        op: AotBuiltinOp,
        args: Vec<VarRef>,
    },
    Rand {
        dest: VarRef,
        dims: Vec<VarRef>,
    },
    Randn {
        dest: VarRef,
        dims: Vec<VarRef>,
    },
    /// Function call
    Call {
        dest: Option<VarRef>,
        func: String,
        args: Vec<VarRef>,
    },
    /// Function call with multiple return values.
    CallMulti {
        dests: Vec<VarRef>,
        func: String,
        args: Vec<VarRef>,
    },
    ArrayNew {
        dest: VarRef,
        dims: Vec<VarRef>,
        init: ArrayInit,
    },
    ArraySlice {
        dest: VarRef,
        source: VarRef,
        selectors: Vec<ArraySelector>,
        dims: Vec<VarRef>,
    },
    UnitRangeLength {
        dest: VarRef,
        start: VarRef,
        stop: VarRef,
    },
    ArraySliceAssign {
        array: VarRef,
        selectors: Vec<ArraySelector>,
        value: VarRef,
    },
    /// Stack-allocated isbits struct construction.
    StructNew {
        dest: VarRef,
        layout_id: u32,
        size: u32,
        align: u8,
        fields: Vec<StructFieldInit>,
    },
    /// Array/collection access
    GetIndex {
        dest: VarRef,
        array: VarRef,
        indices: Vec<VarRef>,
    },
    /// Array/collection mutation
    SetIndex {
        array: VarRef,
        indices: Vec<VarRef>,
        value: VarRef,
    },
    /// Field access
    GetField {
        dest: VarRef,
        object: VarRef,
        field: String,
    },
    /// Field access with a precomputed byte offset.
    GetFieldOffset {
        dest: VarRef,
        object: VarRef,
        layout_id: u32,
        offset: i32,
    },
    /// Field mutation
    SetField {
        object: VarRef,
        field: String,
        value: VarRef,
    },
    /// Field mutation with a precomputed byte offset.
    SetFieldOffset {
        object: VarRef,
        offset: i32,
        value: VarRef,
    },
    /// Type assertion/check
    TypeAssert {
        dest: VarRef,
        src: VarRef,
        ty: StaticType,
    },
    /// Phi node for SSA form
    Phi {
        dest: VarRef,
        incoming: Vec<(String, VarRef)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayInit {
    Zero,
    One,
}

#[derive(Debug, Clone)]
pub enum ArraySelector {
    Scalar(VarRef),
    UnitRange { start: VarRef, stop: VarRef },
}

#[derive(Debug, Clone)]
pub struct StructFieldInit {
    pub offset: i32,
    pub value: VarRef,
}

/// Block terminator instruction
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Return from function
    Return(Option<VarRef>),
    /// Return multiple values from a tuple-returning function.
    ReturnMany(Vec<VarRef>),
    /// Unconditional jump
    Jump(String),
    /// Conditional branch
    Branch {
        cond: VarRef,
        then_block: String,
        else_block: String,
    },
    /// Switch on value
    Switch {
        value: VarRef,
        cases: Vec<(ConstValue, String)>,
        default: String,
    },
}

/// Variable reference
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VarRef {
    /// Variable name
    pub name: String,
    /// SSA version (for SSA form)
    pub version: usize,
    /// Type of this variable
    pub ty: StaticType,
}

impl VarRef {
    /// Create a new variable reference
    pub fn new(name: String, ty: StaticType) -> Self {
        Self {
            name,
            version: 0,
            ty,
        }
    }

    /// Create a new version of this variable
    pub fn next_version(&self) -> Self {
        Self {
            name: self.name.clone(),
            version: self.version + 1,
            ty: self.ty.clone(),
        }
    }
}

impl fmt::Display for VarRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.version == 0 {
            write!(f, "%{}", self.name)
        } else {
            write!(f, "%{}.{}", self.name, self.version)
        }
    }
}

/// Binary operation kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpKind {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    // Logical
    And,
    Or,
}

/// Unary operation kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOpKind {
    Neg,
    Not,
    BitNot,
}

/// A complete function in IR form
#[derive(Debug, Clone)]
pub struct IrFunction {
    /// Function name
    pub name: String,
    /// Parameter names and types
    pub params: Vec<(String, StaticType)>,
    /// Return type
    pub return_type: StaticType,
    /// Basic blocks
    pub blocks: Vec<BasicBlock>,
    /// Entry block label
    pub entry: String,
    /// Source line used for native debug information, when available.
    pub debug_line: Option<u32>,
}

impl IrFunction {
    /// Create a new IR function
    pub fn new(name: String, params: Vec<(String, StaticType)>, return_type: StaticType) -> Self {
        let entry = "entry".to_string();
        Self {
            name,
            params,
            return_type,
            blocks: vec![BasicBlock::new(entry.clone())],
            entry,
            debug_line: None,
        }
    }

    /// Get the entry block
    pub fn entry_block(&self) -> Option<&BasicBlock> {
        self.blocks.iter().find(|b| b.label == self.entry)
    }

    /// Get the entry block mutably
    pub fn entry_block_mut(&mut self) -> Option<&mut BasicBlock> {
        self.blocks.iter_mut().find(|b| b.label == self.entry)
    }

    /// Add a new block
    pub fn add_block(&mut self, block: BasicBlock) {
        self.blocks.push(block);
    }
}

/// A complete IR module
#[derive(Debug, Clone)]
pub struct IrModule {
    /// Module name
    pub name: String,
    /// Functions in this module
    pub functions: Vec<IrFunction>,
    pub layouts: Vec<AggregateLayout>,
}

impl IrModule {
    /// Create a new IR module
    pub fn new(name: String) -> Self {
        Self {
            name,
            functions: Vec::new(),
            layouts: Vec::new(),
        }
    }

    /// Add a function to the module
    pub fn add_function(&mut self, func: IrFunction) {
        self.functions.push(func);
    }
}
