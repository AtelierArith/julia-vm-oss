//! Type definitions for the VM.
//!
//! This module contains struct definitions used by the VM:
//! - `FunctionInfo`: Information about a compiled function
//! - `KwParamInfo`: Keyword parameter info
//! - `StructDefInfo`: Struct type definition
//! - `AbstractTypeDefInfo`: Abstract type definition
//! - `ShowMethodEntry`: Entry for Base.show method
//! - `SpecializationKey`, `SpecializedCode`, `SpecializableFunction`: Lazy AoT support
//! - `RuntimeCompileContext`: Context for runtime specialization
//! - `CompiledProgram`: A compiled program ready for execution

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::instr::Instr;
use super::value::{Value, ValueType};

/// Function information for the VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<(String, ValueType)>,
    /// Keyword parameters with their default values
    pub kwparams: Vec<KwParamInfo>,
    pub entry: usize,
    pub return_type: ValueType,
    /// Type parameters from where clause (for type binding support)
    pub type_params: Vec<crate::types::TypeParam>,
    /// Original JuliaType for each parameter (preserves parametric patterns like Complex{T})
    pub param_julia_types: Vec<crate::types::JuliaType>,
    /// Code boundary: start instruction index (inclusive)
    pub code_start: usize,
    /// Code boundary: end instruction index (exclusive)
    pub code_end: usize,
    /// Local slot names (index -> variable name)
    pub slot_names: Vec<String>,
    /// Total number of local slots
    pub local_slot_count: usize,
    /// Slot indices for positional parameters (aligned with params)
    pub param_slots: Vec<usize>,
    /// Index of varargs parameter (if any). Varargs collects remaining args into a Tuple.
    /// For `function f(a, b, args...)`, vararg_param_index would be Some(2).
    pub vararg_param_index: Option<usize>,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    pub vararg_fixed_count: Option<usize>,
}

/// Keyword parameter info for VM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KwParamInfo {
    pub name: String,
    pub default: Value,
    pub ty: ValueType,
    pub slot: usize,
    /// True if this kwarg is required (no default value)
    pub required: bool,
    /// True if this is a varargs kwparam (kwargs...) that collects remaining kwargs
    pub is_varargs: bool,
}

/// Struct type definition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructDefInfo {
    pub name: String,
    pub is_mutable: bool,
    pub fields: Vec<(String, ValueType)>, // (field_name, field_type)
    /// Parent abstract type name (for `struct Dog <: Animal`)
    pub parent_type: Option<String>,
}

impl StructDefInfo {
    /// Check if this struct is isbits (immutable with all primitive fields)
    /// isbits types can be stored inline in arrays (AoS layout)
    pub fn is_isbits(&self) -> bool {
        !self.is_mutable
            && self.fields.iter().all(|(_, field_type)| {
                matches!(
                    field_type,
                    ValueType::F32
                        | ValueType::F64
                        | ValueType::I8
                        | ValueType::I16
                        | ValueType::I32
                        | ValueType::I64
                        | ValueType::U8
                        | ValueType::U16
                        | ValueType::U32
                        | ValueType::U64
                        | ValueType::Bool
                        | ValueType::Char
                )
            })
    }
}

/// Abstract type definition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbstractTypeDefInfo {
    pub name: String,
    /// Parent abstract type name (for `abstract type Mammal <: Animal`)
    pub parent: Option<String>,
    /// Type parameters for parametric abstract types (Issue #2523)
    /// e.g., [T] for `abstract type Container{T} end`
    pub type_params: Vec<String>,
}

/// Entry for a registered Base.show method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowMethodEntry {
    /// The struct type name this show method handles
    pub type_name: String,
    /// Function index in the functions table
    pub func_index: usize,
}

// === Lazy AoT Compilation Support ===

/// Key for specialization cache lookup
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SpecializationKey {
    pub func_index: usize,
    pub arg_types: Vec<ValueType>,
}

/// Specialized function code
#[derive(Debug, Clone)]
pub struct SpecializedCode {
    /// Entry point in the code vector
    pub entry: usize,
    /// Inferred return type for this specialization
    pub return_type: ValueType,
    /// Length of the specialized bytecode
    pub code_len: usize,
}

/// A function that can be specialized at runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecializableFunction {
    /// The Core IR for this function (retained for specialization)
    pub ir: crate::ir::core::Function,
    /// Function name (for error messages)
    pub name: String,
    /// Fallback function index (generic version)
    pub fallback_index: usize,
}

/// Runtime compile context for specialization
#[derive(Debug, Clone)]
pub struct RuntimeCompileContext {
    pub struct_table: HashMap<String, crate::compile::StructInfo>,
    pub struct_defs: Vec<StructDefInfo>,
    pub parametric_structs: HashMap<String, crate::compile::ParametricStructDef>,
}

/// A compiled Julia program ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    pub code: Vec<Instr>,
    pub functions: Vec<FunctionInfo>,
    pub struct_defs: Vec<StructDefInfo>,
    pub abstract_types: Vec<AbstractTypeDefInfo>,
    /// Registry of Base.show(io::IO, x::T) methods by type name
    pub show_methods: Vec<ShowMethodEntry>,
    pub entry: usize,
    /// Functions that can be specialized at runtime (Lazy AoT)
    pub specializable_functions: Vec<SpecializableFunction>,
    /// Runtime compile context for specialization (not serialized)
    #[serde(skip)]
    pub compile_context: Option<RuntimeCompileContext>,
    /// Number of base functions (for REPL to track across evaluations)
    pub base_function_count: usize,
    /// Global slot names (index -> variable name) for module/main scope
    pub global_slot_names: Vec<String>,
    /// Total number of global slots
    pub global_slot_count: usize,
}
