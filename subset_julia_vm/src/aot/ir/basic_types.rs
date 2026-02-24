//! Low-level SSA IR types for AoT compilation.
//!
//! Contains basic block, instruction, terminator, variable reference,
//! constant value, and IR function/module types.

use super::super::types::JuliaType;
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
    LoadConst { dest: VarRef, value: ConstValue },
    /// Copy a value
    Copy { dest: VarRef, src: VarRef },
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
    /// Function call
    Call {
        dest: Option<VarRef>,
        func: String,
        args: Vec<VarRef>,
    },
    /// Array/collection access
    GetIndex {
        dest: VarRef,
        array: VarRef,
        index: VarRef,
    },
    /// Array/collection mutation
    SetIndex {
        array: VarRef,
        index: VarRef,
        value: VarRef,
    },
    /// Field access
    GetField {
        dest: VarRef,
        object: VarRef,
        field: String,
    },
    /// Field mutation
    SetField {
        object: VarRef,
        field: String,
        value: VarRef,
    },
    /// Type assertion/check
    TypeAssert {
        dest: VarRef,
        src: VarRef,
        ty: JuliaType,
    },
    /// Phi node for SSA form
    Phi {
        dest: VarRef,
        incoming: Vec<(String, VarRef)>,
    },
}

/// Block terminator instruction
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Return from function
    Return(Option<VarRef>),
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
    pub ty: JuliaType,
}

impl VarRef {
    /// Create a new variable reference
    pub fn new(name: String, ty: JuliaType) -> Self {
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

/// Constant value
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Int64(i64),
    Int32(i32),
    Float64(f64),
    Float32(f32),
    Bool(bool),
    Char(char),
    String(String),
    Nothing,
}

impl ConstValue {
    /// Get the type of this constant
    pub fn get_type(&self) -> JuliaType {
        match self {
            ConstValue::Int64(_) => JuliaType::Int64,
            ConstValue::Int32(_) => JuliaType::Int32,
            ConstValue::Float64(_) => JuliaType::Float64,
            ConstValue::Float32(_) => JuliaType::Float32,
            ConstValue::Bool(_) => JuliaType::Bool,
            ConstValue::Char(_) => JuliaType::Char,
            ConstValue::String(_) => JuliaType::String,
            ConstValue::Nothing => JuliaType::Nothing,
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
    pub params: Vec<(String, JuliaType)>,
    /// Return type
    pub return_type: JuliaType,
    /// Basic blocks
    pub blocks: Vec<BasicBlock>,
    /// Entry block label
    pub entry: String,
}

impl IrFunction {
    /// Create a new IR function
    pub fn new(name: String, params: Vec<(String, JuliaType)>, return_type: JuliaType) -> Self {
        let entry = "entry".to_string();
        Self {
            name,
            params,
            return_type,
            blocks: vec![BasicBlock::new(entry.clone())],
            entry,
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
}

impl IrModule {
    /// Create a new IR module
    pub fn new(name: String) -> Self {
        Self {
            name,
            functions: Vec::new(),
        }
    }

    /// Add a function to the module
    pub fn add_function(&mut self, func: IrFunction) {
        self.functions.push(func);
    }
}
