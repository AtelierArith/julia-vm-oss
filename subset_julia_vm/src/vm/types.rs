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

use crate::ir::core::Expr;

use super::value::{Value, ValueType};
use super::{instr::Instr, VarTypeTag};

fn default_method_min_world() -> u64 {
    1
}

/// Function information for the VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub params: Vec<(String, ValueType)>,
    /// Keyword parameters with their default values
    pub kwparams: Vec<KwParamInfo>,
    pub entry: usize,
    pub return_type: ValueType,
    /// Original Julia return type when `return_type` would lose precision
    /// (e.g. `Union{Int64,String}` represented as `ValueType::Any`).
    #[serde(default)]
    pub return_julia_type: Option<crate::types::JuliaType>,
    /// True for methods written as `Base.f(...)` / `Base.:op(...)` extensions.
    #[serde(default)]
    pub is_base_extension: bool,
    /// True for methods lowered from `@generated` definitions. The VM uses this
    /// to route compatibility fallback bodies through the staged Expr cache
    /// instead of the direct-call fast path (Issue #5936).
    #[serde(default)]
    pub is_generated: bool,
    /// First runtime world where this method is visible to ordinary dispatch.
    #[serde(default = "default_method_min_world")]
    pub min_world: u64,
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
    /// Statically known local slot storage tags (index -> type tag).
    #[serde(default)]
    pub slot_types: Vec<Option<VarTypeTag>>,
    /// Total number of local slots
    pub local_slot_count: usize,
    /// Slot indices for positional parameters (aligned with params)
    pub param_slots: Vec<usize>,
    /// Index of varargs parameter (if any). Varargs collects remaining args into a Tuple.
    /// For `function f(a, b, args...)`, vararg_param_index would be Some(2).
    pub vararg_param_index: Option<usize>,
    /// For Vararg{T, N}: fixed argument count N. None = any count. (Issue #2525)
    pub vararg_fixed_count: Option<usize>,
    /// Representative inline metadata from `@inline` / `@noinline` /
    /// `@propagate_inbounds` markers retained at the top of the function body.
    /// Mirrors upstream `CodeInfo.inlining`: 0 = default, 1 = inline,
    /// 2 = noinline (Issues #4977, #4980).
    #[serde(default)]
    pub inlining_meta: u8,
    /// Representative constant-propagation metadata from `Base.@constprop`
    /// markers. Mirrors upstream `Method.constprop` / `CodeInfo.constprop`:
    /// 0 = default, 1 = aggressive, 2 = none (Issues #4978, #4981).
    #[serde(default)]
    pub constprop_meta: u8,
    /// Representative `@nospecialize` bitmask retained from a statement-position
    /// `@nospecialize a b` marker. Mirrors upstream `Method.nospecialize`: bit
    /// `i` (0-based, over explicit positional parameters) is set when that
    /// parameter is nospecialized; a trailing `@specialize` clears the mask
    /// (Issue #4984).
    #[serde(default)]
    pub nospecialize_meta: i32,
    /// Representative `Base.@propagate_inbounds` metadata. Mirrors upstream
    /// `CodeInfo.propagate_inbounds` (Issue #4979).
    #[serde(default)]
    pub propagate_inbounds_meta: bool,
    /// Representative `Base.@nospecializeinfer` metadata. Mirrors upstream
    /// `CodeInfo.nospecializeinfer` (Issue #4979).
    #[serde(default)]
    pub nospecializeinfer_meta: bool,
    /// Representative `Base.@assume_effects` purity bitmask. Mirrors upstream
    /// `CodeInfo.purity` (`encode_effects_override` value): 0 = default
    /// (Issue #4983).
    #[serde(default)]
    pub purity_meta: u16,
    /// Name of the `where`-bound type parameter the body directly returns, when
    /// the method is of the shape `g(...) where {..., R, ...} = R` (the body is
    /// nothing but `return R`). Reflection inference uses this to bind `R` from
    /// the concrete call signature and recover a precise return type instead of
    /// widening to `Any` (Issue #4845).
    #[serde(default)]
    pub direct_return_type_param: Option<String>,
    /// 1-based source line of the function definition, taken from the IR
    /// `Function.span.start_line`. Surfaced as `Method.line` by `methods(f)`
    /// reflection so `show(::Method)` can render the ` @ Module file:line`
    /// suffix (Issue #5125). `0` when no source span is available (e.g. builtin
    /// stub `FunctionInfo`s synthesized in the VM).
    #[serde(default)]
    pub def_line: u32,
}

/// Keyword parameter info for VM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KwParamInfo {
    pub name: String,
    pub default: Value,
    /// Original default expression for omitted keyword evaluation.
    #[serde(default)]
    pub default_expr: Option<Expr>,
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
    #[serde(default)]
    pub field_julia_types: Vec<crate::types::JuliaType>,
    /// Parent abstract type name (for `struct Dog <: Animal`)
    pub parent_type: Option<String>,
}

impl StructDefInfo {
    /// Check if this struct is isbits (immutable with all primitive fields)
    /// isbits types can be stored inline in arrays (AoS layout)
    pub fn is_isbits(&self) -> bool {
        self.is_isbits_with_struct_defs(&[])
    }

    pub fn is_isbits_with_struct_defs(&self, struct_defs: &[StructDefInfo]) -> bool {
        if self.is_mutable {
            return false;
        }
        if self.field_julia_types.len() == self.fields.len() {
            return self
                .field_julia_types
                .iter()
                .all(|field_type| julia_type_isbits(field_type, struct_defs));
        }
        self.fields
            .iter()
            .all(|(_, field_type)| value_type_isbits(field_type, struct_defs))
    }

    /// The data size, in bytes, of an instance of this struct - i.e. what
    /// `sizeof(T)` returns. This is computed from the field layout for both
    /// immutable AND mutable structs: upstream `sizeof(::Type)` of a mutable
    /// struct is still its packed/padded data size (e.g. `sizeof` of a mutable
    /// `(Int8, Int64, Int8)` struct is 24, not the pointer width). The pointer
    /// indirection of a mutable value only matters when it is *embedded* as a
    /// field of another struct, which is handled by `value_type_layout` /
    /// `julia_type_layout` returning the pointer width for mutable field types
    /// (Issue #5107, building on #5100).
    pub fn layout_size_bytes(&self, struct_defs: &[StructDefInfo]) -> Option<usize> {
        let field_offsets = self.field_offsets_bytes(struct_defs)?;
        let mut offset = 0usize;
        let mut max_align = 1usize;
        if self.field_julia_types.len() == self.fields.len() {
            for (idx, field_type) in self.field_julia_types.iter().enumerate() {
                let (size, align) = julia_type_layout(field_type, struct_defs)?;
                max_align = max_align.max(align);
                offset = field_offsets.get(idx).copied()?.checked_add(size)?;
            }
            return Some(align_to(offset, max_align));
        }

        for (idx, (_, field_type)) in self.fields.iter().enumerate() {
            let (size, align) = value_type_layout(field_type, struct_defs)?;
            max_align = max_align.max(align);
            offset = field_offsets.get(idx).copied()?.checked_add(size)?;
        }
        Some(align_to(offset, max_align))
    }

    pub fn field_offsets_bytes(&self, struct_defs: &[StructDefInfo]) -> Option<Vec<usize>> {
        let mut offset = 0usize;
        let mut offsets = Vec::with_capacity(self.fields.len());
        if self.field_julia_types.len() == self.fields.len() {
            for field_type in &self.field_julia_types {
                let (size, align) = julia_type_layout(field_type, struct_defs)?;
                offset = align_to(offset, align);
                offsets.push(offset);
                offset = offset.checked_add(size)?;
            }
            return Some(offsets);
        }

        for (_, field_type) in &self.fields {
            let (size, align) = value_type_layout(field_type, struct_defs)?;
            offset = align_to(offset, align);
            offsets.push(offset);
            offset = offset.checked_add(size)?;
        }
        Some(offsets)
    }

    /// The alignment (in bytes) this struct requires when stored inline as a
    /// field of another struct or as an array element.
    ///
    /// Upstream Julia (`jl_compute_field_offsets` in `julia/src/datatype.c`) sets
    /// a struct's alignment to the maximum alignment of its (inline-stored)
    /// fields - NOT to `next_power_of_two(sizeof)`. The two only coincide when
    /// every field is at least as wide as the struct's size, so an odd-sized
    /// struct such as `struct T; a::Int8; b::Int8; c::Int8; end` aligns to 1,
    /// and a 24-byte `struct S; a::Int64; b::Int64; c::Int64; end` aligns to 8.
    /// This is the type's *own* alignment (what `Base.datatype_alignment(T)`
    /// returns), the maximum over its fields' alignments, for both immutable and
    /// mutable structs - upstream a mutable `(Int8,)` struct still reports
    /// alignment 1. A mutable struct is stored by pointer only when *embedded*
    /// as another struct's field; that 8-byte/8-align indirection is applied by
    /// `value_type_layout` / `julia_type_layout`, not here (Issue #5107,
    /// building on #5100).
    pub fn layout_align_bytes(&self, struct_defs: &[StructDefInfo]) -> Option<usize> {
        let mut max_align = 1usize;
        if self.field_julia_types.len() == self.fields.len() {
            for field_type in &self.field_julia_types {
                let (_, align) = julia_type_layout(field_type, struct_defs)?;
                max_align = max_align.max(align);
            }
            return Some(max_align);
        }

        for (_, field_type) in &self.fields {
            let (_, align) = value_type_layout(field_type, struct_defs)?;
            max_align = max_align.max(align);
        }
        Some(max_align)
    }
}

fn align_to(offset: usize, align: usize) -> usize {
    if align <= 1 {
        offset
    } else {
        offset.div_ceil(align) * align
    }
}

fn value_type_isbits(field_type: &ValueType, struct_defs: &[StructDefInfo]) -> bool {
    match field_type {
        ValueType::Bool
        | ValueType::I8
        | ValueType::I16
        | ValueType::I32
        | ValueType::I64
        | ValueType::I128
        | ValueType::U8
        | ValueType::U16
        | ValueType::U32
        | ValueType::U64
        | ValueType::U128
        | ValueType::F16
        | ValueType::F32
        | ValueType::F64
        | ValueType::ComplexF32
        | ValueType::ComplexF64
        | ValueType::Char
        | ValueType::Nothing
        | ValueType::Missing => true,
        ValueType::Struct(type_id) => struct_defs
            .get(*type_id)
            .is_some_and(|def| def.is_isbits_with_struct_defs(struct_defs)),
        _ => false,
    }
}

fn julia_type_isbits(field_type: &crate::types::JuliaType, struct_defs: &[StructDefInfo]) -> bool {
    match field_type.name().as_ref() {
        "Bool" | "Int8" | "Int16" | "Int32" | "Int64" | "Int128" | "UInt8" | "UInt16"
        | "UInt32" | "UInt64" | "UInt128" | "Float16" | "Float32" | "Float64" | "Char"
        | "Nothing" | "Missing" => true,
        name => struct_defs
            .iter()
            .find(|def| def.name == name)
            .is_some_and(|def| def.is_isbits_with_struct_defs(struct_defs)),
    }
}

fn value_type_layout(
    field_type: &ValueType,
    struct_defs: &[StructDefInfo],
) -> Option<(usize, usize)> {
    match field_type {
        ValueType::Bool | ValueType::I8 | ValueType::U8 => Some((1, 1)),
        ValueType::I16 | ValueType::U16 | ValueType::F16 => Some((2, 2)),
        ValueType::I32 | ValueType::U32 | ValueType::F32 | ValueType::Char => Some((4, 4)),
        ValueType::I64 | ValueType::U64 | ValueType::F64 => Some((8, 8)),
        ValueType::ComplexF32 => Some((8, 4)),
        ValueType::ComplexF64 => Some((16, 8)),
        ValueType::I128 | ValueType::U128 => Some((16, 16)),
        ValueType::Nothing | ValueType::Missing => Some((0, 1)),
        ValueType::Struct(type_id) => {
            let def = struct_defs.get(*type_id)?;
            // A mutable struct is boxed and referenced by pointer when stored as
            // a field of another struct, so it contributes the pointer width and
            // alignment (8, 8), not its own data layout (Issue #5107).
            if def.is_mutable {
                return Some((8, 8));
            }
            let size = def.layout_size_bytes(struct_defs)?;
            // A struct's alignment is the max of its fields' alignments, not
            // `next_power_of_two(size)`. See `layout_align_bytes`. (Issue #5100)
            let align = def.layout_align_bytes(struct_defs)?;
            Some((size, align))
        }
        _ => Some((8, 8)),
    }
}

fn julia_type_layout(
    field_type: &crate::types::JuliaType,
    struct_defs: &[StructDefInfo],
) -> Option<(usize, usize)> {
    match field_type.name().as_ref() {
        "Bool" | "Int8" | "UInt8" => Some((1, 1)),
        "Int16" | "UInt16" | "Float16" => Some((2, 2)),
        "Int32" | "UInt32" | "Float32" | "Char" => Some((4, 4)),
        "Int64" | "UInt64" | "Float64" => Some((8, 8)),
        "Int128" | "UInt128" => Some((16, 16)),
        "Nothing" | "Missing" => Some((0, 1)),
        name => {
            let Some(def) = struct_defs.iter().find(|def| def.name == name) else {
                return Some((8, 8));
            };
            // A mutable struct field is stored by pointer (boxed), contributing
            // the pointer width and alignment (8, 8) (Issue #5107).
            if def.is_mutable {
                return Some((8, 8));
            }
            let size = def.layout_size_bytes(struct_defs)?;
            // A struct's alignment is the max of its fields' alignments, not
            // `next_power_of_two(size)`. See `layout_align_bytes`. (Issue #5100)
            let align = def.layout_align_bytes(struct_defs)?;
            Some((size, align))
        }
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

/// User-declared primitive type definition information (`primitive type Name Bits end`).
///
/// Carries the declared bit width and (optional) abstract supertype so the type
/// reflection layer can answer `isprimitivetype` / `isbitstype` / `sizeof` /
/// `supertype` / `<:` for the user type (Issue #5058).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimitiveTypeDefInfo {
    pub name: String,
    /// Parent abstract type name (for `primitive type MyU8 <: Unsigned 8 end`).
    /// `None` defaults to `Any`.
    pub parent: Option<String>,
    /// Declared number of bits (always a positive multiple of 8).
    pub bits: u32,
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

/// Pre-resolved direct-dispatch record for the all-`I64` specialize hot path
/// (Issue #8167).
///
/// `CallSpecializeI64Slots` originally rebuilt a `SpecializationKey {
/// func_index, arg_types: vec![I64; n] }` and hashed that `Vec`-keyed map on
/// *every* call, plus cloned the callee's `param_slots` `Vec` each time. For a
/// tight loop like `calc_pi`'s `mygcd` that is two heap allocations and a
/// `Vec`-key hash per call. Because the `I64Slots` instruction only fires when
/// every argument slot already holds an `I64`, the resolved specialization for a
/// given `(spec_func_index, arity)` is constant for the lifetime of the `Vm`, so
/// it can be resolved once and dispatched directly thereafter — the
/// "`CallResolvedI64Slots`-like direct call" described in #8159 proposal 1.
#[derive(Debug, Clone)]
pub struct I64SpecDispatch {
    /// Entry point of the specialized body in the code vector.
    pub entry: usize,
    /// One-past-the-end of the specialized body (`entry + code_len`).
    pub code_end: usize,
    /// Generic (fallback) function index, used for frame bookkeeping.
    pub fallback_index: usize,
    /// Local slot count of the callee frame.
    pub local_slot_count: usize,
    /// Parameter slot indices, shared (no per-call `Vec` clone).
    pub param_slots: std::rc::Rc<[usize]>,
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
    pub type_aliases: HashMap<String, String>,
    /// User-declared primitive types, so runtime type reflection can answer
    /// isprimitivetype / sizeof / supertype for them (Issue #5058).
    pub primitive_types: Vec<PrimitiveTypeDefInfo>,
    /// True when the program defines a user `getindex` override on a native
    /// array-like receiver (Issue #6657). The runtime function specializer then
    /// refuses to emit its native-indexing fast path for scalar `xs[i]`, so the
    /// generic body's runtime `getindex` dispatch (which can reach the override)
    /// is used instead. False in the common no-override case, leaving the hot
    /// indexing fast path untouched.
    pub disable_array_getindex_specialization: bool,
    /// True when the program defines a user `setindex!` override on a native
    /// array-like receiver (Issue #6806). The `IndexStore` native write fast path
    /// for a MemoryRef-backed `Array{T,N}` wrapper is then refused so the
    /// override is reached via `setindex!` dispatch. False in the common
    /// no-override case, leaving the hot write fast path untouched. Mirrors
    /// `disable_array_getindex_specialization` for the write side.
    pub disable_array_setindex_specialization: bool,
    /// True when the program defines a user `getproperty` override (Issue #8127).
    /// The function specializer then refuses to emit a direct `GetField` for
    /// `obj.field` reads, so the access goes through the interpreter's
    /// `getproperty` dispatch (which reaches the override). False in the common
    /// no-override case, leaving the hot struct-field fast path untouched.
    pub disable_field_access_specialization: bool,
}

/// A compiled Julia program ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledProgram {
    pub code: Vec<Instr>,
    pub functions: Vec<FunctionInfo>,
    pub struct_defs: Vec<StructDefInfo>,
    pub abstract_types: Vec<AbstractTypeDefInfo>,
    /// User-declared primitive types (`primitive type Name Bits end`, Issue #5058)
    #[serde(default)]
    pub primitive_types: Vec<PrimitiveTypeDefInfo>,
    /// Registry of Base.show(io::IO, x::T) methods by type name
    pub show_methods: Vec<ShowMethodEntry>,
    pub entry: usize,
    /// Functions that can be specialized at runtime (Lazy AoT)
    pub specializable_functions: Vec<SpecializableFunction>,
    /// Map from generic fallback function index to `specializable_functions`
    /// index for runtime `CallSpecialize` emission.
    ///
    /// This intentionally excludes reflection-only registrations, which may
    /// live in `specializable_functions` but must not bypass dispatch.
    #[serde(default)]
    pub runtime_specialization_map: Vec<(usize, usize)>,
    /// Runtime compile context for specialization.
    ///
    /// This is reconstructed for serialized Base caches at load time. Keeping it
    /// out of bincode avoids making every nextest process deserialize the full
    /// prelude struct IR just to start a fixture. See Issue #3973.
    #[serde(skip)]
    pub compile_context: Option<RuntimeCompileContext>,
    /// Number of base functions (for REPL to track across evaluations)
    pub base_function_count: usize,
    /// Macro bindings visible per module, keyed by module path
    /// (`"Main"`, `"AbstractAlgebra"`, ...). Each value is the set of macro
    /// names (with the leading `@`) the module owns or sees via `using`.
    /// Backs function-form `isdefined(::Module, Symbol("@name"))`, which
    /// otherwise never consults the macro binding table (Issue #7948).
    #[serde(default)]
    pub macro_bindings: std::collections::HashMap<String, std::collections::HashSet<String>>,
    /// Global slot names (index -> variable name) for module/main scope
    pub global_slot_names: Vec<String>,
    /// Statically known global slot storage tags (index -> type tag).
    #[serde(default)]
    pub global_slot_types: Vec<Option<VarTypeTag>>,
    /// Total number of global slots
    pub global_slot_count: usize,
}
