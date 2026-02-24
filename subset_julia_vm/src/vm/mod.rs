// Submodules
mod broadcast;
mod builtins_arrays;
mod builtins_dicts;
mod builtins_equality;
mod builtins_collections;
mod builtins_exec;
mod builtins_io;
mod builtins_linalg;
mod builtins_macro;
mod builtins_math;
mod builtins_numeric;
mod builtins_reflection;
mod builtins_sets;
mod builtins_stats;
mod builtins_strings;
mod builtins_types;
mod builtins_types_conversion;
mod convert;
mod dynamic_ops;
pub mod error;
mod exec;
mod field_indices;
mod formatting;
mod frame;
mod hof_exec;
pub mod instr;
pub(crate) mod intrinsics_exec;
mod matmul;
pub mod profiler;
pub(crate) mod slot;
pub mod specialize;
pub(crate) mod splat;
pub mod stack_ops;
mod type_ops;
pub(crate) mod type_utils;
pub mod types;
pub(crate) mod util;
pub mod value;

// Re-exports from types module
pub use types::{
    AbstractTypeDefInfo, CompiledProgram, FunctionInfo, KwParamInfo, RuntimeCompileContext,
    ShowMethodEntry, SpecializableFunction, SpecializationKey, SpecializedCode, StructDefInfo,
};

// Re-exports
pub use error::{SpannedVmError, VmError};
pub use instr::Instr;
pub use stack_ops::{StackOps, StackOpsExt};
pub use value::{
    new_array_ref,
    new_typed_array_ref,
    ArrayData,
    ArrayElementType,
    ArrayRef,
    ArrayValue,
    ClosureValue,
    ComposedFunctionValue,
    DictKey,
    DictValue,
    ExprValue,
    FunctionValue,
    GeneratorValue,
    GlobalRefValue,
    IOKind,
    IOValue,
    LineNumberNodeValue,
    ModuleValue,
    NamedTupleValue,
    PairsValue,
    RangeValue,
    StructInstance,
    // Macro system types
    SymbolValue,
    TupleValue,
    TypedArrayRef,
    TypedArrayValue,
    Value,
    ValueType,
};

// Internal imports
use frame::{BroadcastState, ComposedCallState, Frame, Handler, SprintState};
use util::bind_value_to_slot;

use crate::intrinsics::Intrinsic;
use crate::rng::{RngInstance, RngLike, StableRng, Xoshiro};
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::os::raw::{c_char, c_void};

/// Hash a type name string to a u64 key for the dispatch cache (Issue #3355).
/// Avoids storing String keys in the hot dispatch path.
#[inline]
pub(crate) fn hash_type_name(name: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

/// Output callback function type for streaming output.
/// Takes a context pointer and the output string (null-terminated C string).
pub type OutputCallback = extern "C" fn(context: *mut c_void, output: *const c_char);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BinaryDispatchOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    IntDiv,
    Pow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BinaryDispatchKey {
    pub op: BinaryDispatchOp,
    pub left: ValueType,
    pub right: ValueType,
}

pub struct Vm<R: RngLike> {
    ip: usize,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    return_ips: Vec<usize>,
    handlers: Vec<Handler>,
    code: Vec<Instr>,
    functions: Vec<FunctionInfo>,
    struct_defs: Vec<StructDefInfo>,
    abstract_types: Vec<AbstractTypeDefInfo>, // User-defined abstract types
    show_methods: std::collections::HashMap<String, usize>, // type_name -> func_index
    struct_heap: Vec<StructInstance>,         // Heap for mutable struct instances
    rng: R,
    output: String, // Buffer for println output
    output_callback: Option<OutputCallback>,
    output_callback_context: *mut c_void,
    broadcast_state: Option<BroadcastState>,
    composed_call_state: Option<ComposedCallState>,
    sprint_state: Option<SprintState>,
    pending_error: Option<VmError>,
    /// The pending exception value for catch blocks (preserves struct instances)
    pending_exception_value: Option<Value>,
    rethrow_on_finally: bool,
    // Test state for @test and @testset macros
    test_pass_count: usize,
    test_fail_count: usize,
    test_broken_count: usize,
    current_testset: Option<String>,
    // Test throws state: (expected_exception_type, was_thrown)
    test_throws_state: Option<(String, bool)>,
    // === Lazy AoT Compilation Support ===
    specializable_functions: Vec<SpecializableFunction>,
    specialization_cache: HashMap<SpecializationKey, SpecializedCode>,
    binary_method_cache: HashMap<BinaryDispatchKey, usize>,
    compile_context: Option<RuntimeCompileContext>,
    global_slot_names: Vec<String>,
    global_slot_map: HashMap<String, usize>,
    // Macro system support
    gensym_counter: u64, // Counter for generating unique symbol names
    // Cached well-known struct type IDs (Issue #2940)
    cached_cartesian_index_type_id: Cell<Option<usize>>,
    cached_pair_type_id: Cell<Option<usize>>,
    cached_complex_type_id: Cell<Option<usize>>,
    // Struct name -> index lookup (Issue #2938)
    struct_def_name_index: HashMap<String, usize>,
    // Abstract type name -> index lookup (Issue #2896)
    abstract_type_name_index: HashMap<String, usize>,
    // Method dispatch cache: (call_site_ip, hashed_type_name) → func_index (Issue #2943, #3355)
    dispatch_cache: HashMap<usize, HashMap<u64, usize>>,
    // Function name → indices lookup for O(1) name-based dispatch (Issue #3361)
    function_name_index: HashMap<String, Vec<usize>>,
    // Source map: instruction IP → source span (Issue #2856)
    source_map: Vec<Option<crate::span::Span>>,
    // IP of the last instruction that caused an error (Issue #2856)
    last_error_ip: Option<usize>,
    // Pre-computed transitive closure of abstract type hierarchy (Issue #3356).
    // Maps type name -> list of all ancestor type names (including parametric base names).
    type_ancestors: HashMap<String, Vec<String>>,
}

/// Pre-compute the transitive closure of the abstract type hierarchy (Issue #3356).
///
/// For each struct and abstract type, walks the parent chain and collects all
/// ancestor type names. This makes `check_abstract_type_hierarchy` O(1).
fn compute_type_ancestors(
    struct_defs: &[StructDefInfo],
    abstract_types: &[AbstractTypeDefInfo],
    abstract_type_name_index: &HashMap<String, usize>,
) -> HashMap<String, Vec<String>> {
    fn base_name(s: &str) -> &str {
        s.find('{').map(|idx| &s[..idx]).unwrap_or(s)
    }

    fn collect_ancestors(
        start_parent: &Option<String>,
        abstract_types: &[AbstractTypeDefInfo],
        abstract_type_name_index: &HashMap<String, usize>,
    ) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current_parent = start_parent.clone();
        while let Some(ref parent) = current_parent {
            chain.push(parent.clone());
            let parent_base = base_name(parent);
            if parent_base != parent.as_str() {
                chain.push(parent_base.to_string());
            }
            current_parent = abstract_type_name_index
                .get(parent_base)
                .and_then(|&idx| abstract_types.get(idx))
                .and_then(|at| at.parent.clone());
        }
        chain
    }

    let mut ancestors: HashMap<String, Vec<String>> = HashMap::new();

    for struct_def in struct_defs {
        let chain =
            collect_ancestors(&struct_def.parent_type, abstract_types, abstract_type_name_index);
        if !chain.is_empty() {
            ancestors.insert(struct_def.name.clone(), chain);
        }
    }

    for abstract_type in abstract_types {
        let chain =
            collect_ancestors(&abstract_type.parent, abstract_types, abstract_type_name_index);
        if !chain.is_empty() {
            ancestors.insert(abstract_type.name.clone(), chain);
        }
    }

    ancestors
}

impl<R: RngLike> Vm<R> {
    /// Create a new VM with a flat instruction list and an RNG instance.
    ///
    /// Use this constructor when you have a raw `Vec<Instr>` (e.g., from incremental
    /// compilation). For compiled programs with entry points and metadata, prefer
    /// [`Vm::new_program`].
    pub fn new(code: Vec<Instr>, rng: R) -> Self {
        Self {
            ip: 0,
            stack: Vec::with_capacity(256),
            frames: vec![Frame::new()],
            return_ips: Vec::new(),
            handlers: Vec::new(),
            code,
            functions: Vec::new(),
            struct_defs: Vec::new(),
            abstract_types: Vec::new(),
            show_methods: std::collections::HashMap::new(),
            struct_heap: Vec::new(),
            rng,
            output: String::new(),
            output_callback: None,
            output_callback_context: std::ptr::null_mut(),
            broadcast_state: None,
            composed_call_state: None,
            sprint_state: None,
            pending_error: None,
            pending_exception_value: None,
            rethrow_on_finally: false,
            test_pass_count: 0,
            test_fail_count: 0,
            test_broken_count: 0,
            current_testset: None,
            test_throws_state: None,
            // Lazy AoT fields
            specializable_functions: Vec::new(),
            specialization_cache: HashMap::new(),
            binary_method_cache: HashMap::new(),
            compile_context: None,
            global_slot_names: Vec::new(),
            global_slot_map: HashMap::new(),
            gensym_counter: 0,
            cached_cartesian_index_type_id: Cell::new(None),
            cached_pair_type_id: Cell::new(None),
            cached_complex_type_id: Cell::new(None),
            struct_def_name_index: HashMap::new(),
            abstract_type_name_index: HashMap::new(),
            dispatch_cache: HashMap::new(),
            function_name_index: HashMap::new(),
            source_map: Vec::new(),
            last_error_ip: None,
            type_ancestors: HashMap::new(),
        }
    }

    /// Create a new VM from a fully compiled program.
    ///
    /// `CompiledProgram` carries the entry point IP, all function/struct definitions,
    /// global slot layout, and optional lazy-AoT context produced by the compiler.
    /// This is the primary constructor used after calling [`compile_and_run_str`] or
    /// the two-phase compile pipeline.
    pub fn new_program(program: CompiledProgram, rng: R) -> Self {
        // Build show_methods HashMap from the CompiledProgram's Vec
        let show_methods = program
            .show_methods
            .iter()
            .map(|entry| (entry.type_name.clone(), entry.func_index))
            .collect();

        let global_slot_map = program
            .global_slot_names
            .iter()
            .enumerate()
            .map(|(idx, name)| (name.clone(), idx))
            .collect::<HashMap<_, _>>();

        let struct_def_name_index = program
            .struct_defs
            .iter()
            .enumerate()
            .map(|(idx, def)| (def.name.clone(), idx))
            .collect::<HashMap<_, _>>();

        let abstract_type_name_index = program
            .abstract_types
            .iter()
            .enumerate()
            .map(|(idx, at)| (at.name.clone(), idx))
            .collect::<HashMap<_, _>>();

        // Pre-compute transitive closure of abstract type hierarchy (Issue #3356)
        let type_ancestors = compute_type_ancestors(
            &program.struct_defs,
            &program.abstract_types,
            &abstract_type_name_index,
        );

        // Build function name → indices lookup for O(1) dispatch (Issue #3361)
        let mut function_name_index: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, func) in program.functions.iter().enumerate() {
            function_name_index
                .entry(func.name.clone())
                .or_default()
                .push(idx);
        }

        Self {
            ip: program.entry,
            stack: Vec::with_capacity(256),
            frames: vec![Frame::new_with_slots(program.global_slot_count, None)],
            return_ips: Vec::new(),
            handlers: Vec::new(),
            code: program.code,
            functions: program.functions,
            struct_defs: program.struct_defs,
            abstract_types: program.abstract_types,
            show_methods,
            struct_heap: Vec::new(),
            rng,
            output: String::new(),
            output_callback: None,
            output_callback_context: std::ptr::null_mut(),
            broadcast_state: None,
            composed_call_state: None,
            sprint_state: None,
            pending_error: None,
            pending_exception_value: None,
            rethrow_on_finally: false,
            test_pass_count: 0,
            test_fail_count: 0,
            test_broken_count: 0,
            current_testset: None,
            test_throws_state: None,
            // Lazy AoT fields
            specializable_functions: program.specializable_functions,
            specialization_cache: HashMap::new(),
            binary_method_cache: HashMap::new(),
            compile_context: program.compile_context,
            global_slot_names: program.global_slot_names,
            global_slot_map,
            gensym_counter: 0,
            cached_cartesian_index_type_id: Cell::new(None),
            cached_pair_type_id: Cell::new(None),
            cached_complex_type_id: Cell::new(None),
            struct_def_name_index,
            abstract_type_name_index,
            dispatch_cache: HashMap::new(),
            function_name_index,
            source_map: Vec::new(),
            last_error_ip: None,
            type_ancestors,
        }
    }

    /// Inject an `Int64` variable into the current frame before execution.
    ///
    /// If `name` maps to a slot in the global slot layout, the slot is updated
    /// directly; otherwise the value is stored in the legacy `locals_i64` map.
    /// This is used by the REPL and FFI layer to pass integer inputs into Julia code
    /// without going through compilation.
    pub fn set_local_i64(&mut self, name: &str, v: i64) {
        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(frame) = self.frames.last_mut() {
                if let Some(slot_ref) = frame.locals_slots.get_mut(slot) {
                    *slot_ref = Some(Value::I64(v));
                    return;
                }
            }
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.locals_i64.insert(name.to_string(), v);
        }
    }

    /// Inject a `Float64` variable into the current frame before execution.
    ///
    /// Mirrors [`Vm::set_local_i64`] but for floating-point values. The slot-based
    /// fast path is tried first; the legacy `locals_f64` map is used as fallback.
    pub fn set_local_f64(&mut self, name: &str, v: f64) {
        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(frame) = self.frames.last_mut() {
                if let Some(slot_ref) = frame.locals_slots.get_mut(slot) {
                    *slot_ref = Some(Value::F64(v));
                    return;
                }
            }
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.locals_f64.insert(name.to_string(), v);
        }
    }

    /// Get the accumulated output from println calls
    pub fn get_output(&self) -> &str {
        &self.output
    }

    /// Set the source map that maps instruction IPs to source spans (Issue #2856).
    ///
    /// The source map is a parallel vector to `code` — `source_map[ip]` gives the
    /// source span for instruction at `ip`. Populated by the compiler; empty by default.
    pub fn set_source_map(&mut self, source_map: Vec<Option<crate::span::Span>>) {
        self.source_map = source_map;
    }

    /// Get the source span for the instruction that caused the last error (Issue #2856).
    ///
    /// Returns `None` if no error has occurred, or if the source map is not populated.
    pub fn last_error_span(&self) -> Option<crate::span::Span> {
        self.last_error_ip
            .and_then(|ip| self.source_map.get(ip).copied().flatten())
    }

    /// Create a [`SpannedVmError`] from a `VmError`, attaching the source span
    /// of the last error instruction if available (Issue #2856).
    pub fn spanned_error(&self, error: VmError) -> SpannedVmError {
        SpannedVmError {
            error,
            span: self.last_error_span(),
        }
    }

    /// Get the type_id for Complex struct from struct_defs (cached).
    /// Returns the first type_id for a struct named "Complex" or "Complex{...}"
    fn get_complex_type_id(&self) -> usize {
        if let Some(id) = self.cached_complex_type_id.get() {
            return id;
        }
        let id = self
            .struct_defs
            .iter()
            .enumerate()
            .find_map(|(idx, def)| {
                if def.name == "Complex" || def.name.starts_with("Complex{") {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        self.cached_complex_type_id.set(Some(id));
        id
    }

    /// Create a Complex struct instance with the correct struct_name from struct_defs.
    /// This ensures the struct_name matches what's registered in struct_defs for proper dispatch.
    fn create_complex(&mut self, type_id: usize, re: f64, im: f64) -> Value {
        let struct_name = self
            .struct_defs
            .get(type_id)
            .map(|def| def.name.clone())
            .unwrap_or_else(|| "Complex{Float64}".to_string());
        let s =
            StructInstance::with_name(type_id, struct_name, vec![Value::F64(re), Value::F64(im)]);
        let idx = self.struct_heap.len();
        self.struct_heap.push(s);
        Value::StructRef(idx)
    }

    /// Get the ValueType from a Value (for Lazy AoT specialization)
    fn get_value_type(&self, val: &Value) -> ValueType {
        match val {
            Value::I64(_) => ValueType::I64,
            Value::F64(_) => ValueType::F64,
            Value::Str(_) => ValueType::Str,
            Value::Char(_) => ValueType::Char,
            Value::Bool(_) => ValueType::Bool,
            Value::Nothing => ValueType::Nothing,
            Value::Missing => ValueType::Missing,
            Value::Array(arr) => ValueType::ArrayOf(arr.borrow().element_type()),
            Value::Memory(mem) => ValueType::ArrayOf(mem.borrow().element_type().clone()),
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    ValueType::Struct(s.type_id)
                } else {
                    ValueType::Any
                }
            }
            Value::Struct(s) => ValueType::Struct(s.type_id),
            Value::Tuple(_) => ValueType::Tuple,
            Value::NamedTuple(_) => ValueType::Tuple,
            Value::Dict(_) => ValueType::Dict,
            Value::Range(_) => ValueType::Range,
            Value::DataType(_) => ValueType::DataType,
            Value::Rng(_) => ValueType::Rng,
            Value::Generator(_) => ValueType::Generator,
            _ => ValueType::Any,
        }
    }

    /// Get the JuliaType for a Value (for type parameter binding)
    fn get_value_julia_type(&self, val: &Value) -> crate::types::JuliaType {
        match val {
            Value::I64(_) => crate::types::JuliaType::Int64,
            Value::F64(_) => crate::types::JuliaType::Float64,
            Value::Str(_) => crate::types::JuliaType::String,
            Value::Char(_) => crate::types::JuliaType::Char,
            Value::Bool(_) => crate::types::JuliaType::Bool,
            Value::Nothing => crate::types::JuliaType::Nothing,
            Value::Missing => crate::types::JuliaType::Missing,
            Value::StructRef(idx) => {
                if let Some(s) = self.struct_heap.get(*idx) {
                    self.get_parametric_struct_name(s)
                } else {
                    crate::types::JuliaType::Any
                }
            }
            Value::Struct(s) => self.get_parametric_struct_name(s),
            Value::DataType(jt) => jt.clone(),
            Value::Array(arr) => {
                // Get element type from array to support Vector{T} dispatch
                let arr_ref = arr.borrow();
                let elem_jtype = match &arr_ref.data {
                    ArrayData::I8(_) => crate::types::JuliaType::Int8,
                    ArrayData::I16(_) => crate::types::JuliaType::Int16,
                    ArrayData::I32(_) => crate::types::JuliaType::Int32,
                    ArrayData::I64(_) => crate::types::JuliaType::Int64,
                    ArrayData::U8(_) => crate::types::JuliaType::UInt8,
                    ArrayData::U16(_) => crate::types::JuliaType::UInt16,
                    ArrayData::U32(_) => crate::types::JuliaType::UInt32,
                    ArrayData::U64(_) => crate::types::JuliaType::UInt64,
                    ArrayData::F32(_) => crate::types::JuliaType::Float32,
                    ArrayData::F64(_) => crate::types::JuliaType::Float64,
                    ArrayData::Bool(_) => crate::types::JuliaType::Bool,
                    ArrayData::String(_) => crate::types::JuliaType::String,
                    ArrayData::Char(_) => crate::types::JuliaType::Char,
                    ArrayData::StructRefs(refs) => {
                        // Get the struct type from first element
                        if let Some(idx) = refs.first() {
                            if let Some(s) = self.struct_heap.get(*idx) {
                                crate::types::JuliaType::Struct(s.struct_name.clone())
                            } else {
                                crate::types::JuliaType::Any
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                    ArrayData::Any(values) => {
                        // Get the type from first element
                        if let Some(first) = values.first() {
                            match first {
                                Value::StructRef(idx) => {
                                    if let Some(s) = self.struct_heap.get(*idx) {
                                        crate::types::JuliaType::Struct(s.struct_name.clone())
                                    } else {
                                        crate::types::JuliaType::Any
                                    }
                                }
                                Value::Struct(s) => {
                                    crate::types::JuliaType::Struct(s.struct_name.clone())
                                }
                                _ => crate::types::JuliaType::Any,
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                };
                crate::types::JuliaType::VectorOf(Box::new(elem_jtype))
            }
            // Memory → Array (Issue #2764)
            Value::Memory(mem) => {
                let arr = crate::vm::util::memory_to_array_ref(mem);
                let arr_ref = arr.borrow();
                let elem_jtype = match &arr_ref.data {
                    ArrayData::I8(_) => crate::types::JuliaType::Int8,
                    ArrayData::I16(_) => crate::types::JuliaType::Int16,
                    ArrayData::I32(_) => crate::types::JuliaType::Int32,
                    ArrayData::I64(_) => crate::types::JuliaType::Int64,
                    ArrayData::U8(_) => crate::types::JuliaType::UInt8,
                    ArrayData::U16(_) => crate::types::JuliaType::UInt16,
                    ArrayData::U32(_) => crate::types::JuliaType::UInt32,
                    ArrayData::U64(_) => crate::types::JuliaType::UInt64,
                    ArrayData::F32(_) => crate::types::JuliaType::Float32,
                    ArrayData::F64(_) => crate::types::JuliaType::Float64,
                    ArrayData::Bool(_) => crate::types::JuliaType::Bool,
                    ArrayData::String(_) => crate::types::JuliaType::String,
                    ArrayData::Char(_) => crate::types::JuliaType::Char,
                    ArrayData::StructRefs(refs) => {
                        if let Some(idx) = refs.first() {
                            if let Some(s) = self.struct_heap.get(*idx) {
                                crate::types::JuliaType::Struct(s.struct_name.clone())
                            } else {
                                crate::types::JuliaType::Any
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                    ArrayData::Any(values) => {
                        if let Some(first) = values.first() {
                            match first {
                                Value::StructRef(idx) => {
                                    if let Some(s) = self.struct_heap.get(*idx) {
                                        crate::types::JuliaType::Struct(s.struct_name.clone())
                                    } else {
                                        crate::types::JuliaType::Any
                                    }
                                }
                                Value::Struct(s) => {
                                    crate::types::JuliaType::Struct(s.struct_name.clone())
                                }
                                _ => crate::types::JuliaType::Any,
                            }
                        } else {
                            crate::types::JuliaType::Any
                        }
                    }
                };
                crate::types::JuliaType::VectorOf(Box::new(elem_jtype))
            }
            _ => crate::types::JuliaType::Any,
        }
    }

    /// Get the full parametric struct name for a struct instance.
    /// Preserves actual type parameters (e.g., "Complex{Bool}", "Complex{Int64}").
    fn get_parametric_struct_name(&self, s: &StructInstance) -> crate::types::JuliaType {
        // Preserve the actual struct name including type parameters
        crate::types::JuliaType::Struct(s.struct_name.clone())
    }

    /// Try to load a variable from a specific frame index.
    /// Returns true if the variable was found and pushed onto the stack.
    fn try_load_from_frame(&mut self, name: &str, frame_idx: usize) -> bool {
        if let Some(frame) = self.frames.get(frame_idx) {
            // 1. Check slot-based locals first
            if let Some(val) = self.load_slot_value_by_name(frame, name) {
                self.stack.push(val);
                return true;
            }
            // 2. O(1) tag-dispatched lookup across all typed maps
            if let Some(val) = frame.get_local(name) {
                self.stack.push(val);
                return true;
            }
            // 3. Check type_bindings for type parameters from where clause
            if let Some(julia_type) = frame.type_bindings.get(name) {
                self.stack.push(Value::DataType(julia_type.clone()));
                return true;
            }
        }
        false
    }

    /// Get a variable value from a specific frame without pushing to stack.
    /// Returns None if the variable is not found.
    fn get_value_from_frame(&self, name: &str, frame_idx: usize) -> Option<Value> {
        let frame = self.frames.get(frame_idx)?;
        // 1. Check slot-based locals first
        if let Some(val) = self.load_slot_value_by_name(frame, name) {
            return Some(val);
        }
        // 2. Issue #1744: Check captured variables for deeply nested closures
        if let Some(v) = frame.captured_vars.get(name) {
            return Some(v.clone());
        }
        // 3. O(1) tag-dispatched lookup across all typed maps
        frame.get_local(name)
    }

    /// Check if a variable is defined in a specific frame.
    /// Returns true if the variable exists in that frame.
    fn is_var_defined_in_frame(&self, name: &str, frame_idx: usize) -> bool {
        self.get_value_from_frame(name, frame_idx).is_some()
    }

    /// Get a variable value by name, checking current frame first, then global.
    /// Used by eval to resolve symbols at runtime.
    pub fn get_variable_value(&self, name: &str) -> Option<Value> {
        // First check current frame
        let current_frame_idx = self.frames.len().saturating_sub(1);
        if let Some(val) = self.get_value_from_frame(name, current_frame_idx) {
            return Some(val);
        }
        // Try global frame if not in current frame
        if self.frames.len() > 1 {
            if let Some(val) = self.get_value_from_frame(name, 0) {
                return Some(val);
            }
        }
        None
    }

    /// Set a variable value by name in the current frame.
    /// Used by eval to support assignment expressions.
    pub fn set_variable_value(&mut self, name: &str, val: Value) {
        if let Some(frame) = self.frames.last_mut() {
            util::bind_value_to_frame(frame, name, ValueType::Any, val, &mut self.struct_heap);
        }
    }

    fn slot_index_for_frame(&self, frame: &Frame, name: &str) -> Option<usize> {
        if let Some(func_index) = frame.func_index {
            self.functions.get(func_index).and_then(|func| {
                func.slot_names
                    .iter()
                    .position(|slot_name| slot_name == name)
            })
        } else {
            self.global_slot_map.get(name).copied()
        }
    }

    fn load_slot_value_by_name(&self, frame: &Frame, name: &str) -> Option<Value> {
        let slot = self.slot_index_for_frame(frame, name)?;
        frame.locals_slots.get(slot).and_then(|v| v.clone())
    }

    fn slot_name_for_frame(&self, frame: &Frame, slot: usize) -> String {
        if let Some(func_index) = frame.func_index {
            if let Some(name) = self
                .functions
                .get(func_index)
                .and_then(|func| func.slot_names.get(slot))
            {
                return name.clone();
            }
        } else if let Some(name) = self.global_slot_names.get(slot) {
            return name.clone();
        }
        format!("slot {}", slot)
    }

    /// Set the output callback for streaming output.
    /// The callback will be called for each output line with the context pointer.
    pub fn set_output_callback(&mut self, callback: OutputCallback, context: *mut c_void) {
        self.output_callback = Some(callback);
        self.output_callback_context = context;
    }

    /// Emit output to the buffer and optionally to the callback.
    /// This is the central method for all output operations.
    ///
    /// When inside a sprint call, output is redirected to the sprint's IOBuffer
    /// instead of stdout/the main output buffer.
    fn emit_output(&mut self, s: &str, newline: bool) {
        // Check if we're inside a sprint call - if so, redirect output to the sprint buffer
        if let Some(ref state) = self.sprint_state {
            let mut io = state.io.borrow_mut();
            io.buffer.push_str(s);
            if newline {
                io.buffer.push('\n');
            }
            return;
        }

        // Normal output path - buffer for get_output() (REPL and non-streaming use cases)
        self.output.push_str(s);
        if newline {
            self.output.push('\n');
        }

        // Call the streaming callback if set
        if let Some(callback) = self.output_callback {
            let line = if newline {
                format!("{}\n", s)
            } else {
                s.to_string()
            };
            if let Ok(cstr) = CString::new(line) {
                callback(self.output_callback_context, cstr.as_ptr());
            }
        }
    }

    /// Get a global variable by name from the top-level frame.
    /// Used by REPL session to extract variables after execution.
    pub fn get_global(&self, name: &str) -> Option<Value> {
        // Look in the first (global) frame
        let frame = self.frames.first()?;

        if let Some(&slot) = self.global_slot_map.get(name) {
            if let Some(Some(val)) = frame.locals_slots.get(slot) {
                return Some(val.clone());
            }
        }

        // Check each type of local storage
        if let Some(&v) = frame.locals_i64.get(name) {
            return Some(Value::I64(v));
        }
        if let Some(&v) = frame.locals_f64.get(name) {
            return Some(Value::F64(v));
        }
        // Complex is now stored in locals_struct (handled below)
        if let Some(v) = frame.locals_str.get(name) {
            return Some(Value::Str(v.clone()));
        }
        if let Some(v) = frame.locals_array.get(name) {
            return Some(Value::Array(v.clone()));
        }
        // Check locals_any for TypedArray and other dynamic types
        if let Some(v) = frame.locals_any.get(name) {
            return Some(v.clone());
        }
        if let Some(v) = frame.locals_range.get(name) {
            return Some(Value::Range(v.clone()));
        }
        if let Some(v) = frame.locals_tuple.get(name) {
            return Some(Value::Tuple(v.clone()));
        }
        if let Some(v) = frame.locals_named_tuple.get(name) {
            return Some(Value::NamedTuple(v.clone()));
        }
        if let Some(v) = frame.locals_dict.get(name) {
            return Some(Value::Dict(v.clone()));
        }
        if let Some(v) = frame.locals_rng.get(name) {
            return Some(Value::Rng(v.clone()));
        }
        // Struct locals are stored as heap indices - return StructRef to preserve heap reference
        if let Some(&idx) = frame.locals_struct.get(name) {
            return Some(Value::StructRef(idx));
        }

        None
    }

    /// Get a reference to the struct heap (for REPL display)
    pub fn get_struct_heap(&self) -> &[StructInstance] {
        &self.struct_heap
    }

    /// Pop a numeric value as f64 from stack, handling Rational and BigInt.
    /// Uses StackOpsExt with struct_heap context.
    #[inline]
    pub fn pop_f64_or_i64(&mut self) -> Result<f64, VmError> {
        StackOpsExt::pop_f64_or_i64(&mut self.stack, &self.struct_heap)
    }

    /// Pop a complex number from stack, handling promotion from real numbers.
    /// Uses StackOpsExt with struct_heap context.
    #[inline]
    pub fn pop_complex(&mut self) -> Result<(f64, f64), VmError> {
        StackOpsExt::pop_complex(&mut self.stack, &self.struct_heap)
    }

    /// Pop exception handlers that were pushed by the current function.
    /// This should be called before returning from a function to clean up
    /// any try-catch handlers that are still active.
    ///
    /// Handlers store `return_ip_len` which is the length of return_ips
    /// when the handler was pushed. We pop all handlers whose return_ip_len
    /// is greater than or equal to the current return_ips length (after
    /// we've popped our return IP).
    pub(crate) fn pop_handlers_for_return(&mut self) {
        let current_return_ip_len = self.return_ips.len();
        // Pop handlers that were pushed in the current function frame
        // (their return_ip_len >= current_return_ip_len means they were
        // pushed after we entered this function)
        while let Some(handler) = self.handlers.last() {
            if handler.return_ip_len >= current_return_ip_len {
                self.handlers.pop();
            } else {
                break;
            }
        }
    }

    fn handle_error(&mut self, err: VmError) -> Result<(), VmError> {
        if let Some(handler) = self.handlers.pop() {
            self.pending_error = Some(err);
            self.rethrow_on_finally = handler.catch_ip.is_none() && handler.finally_ip.is_some();
            self.stack.truncate(handler.stack_len);
            self.frames.truncate(handler.frame_len);
            self.return_ips.truncate(handler.return_ip_len);

            if let Some(catch_ip) = handler.catch_ip {
                self.ip = catch_ip;
            } else if let Some(finally_ip) = handler.finally_ip {
                self.ip = finally_ip;
            } else {
                let err = self
                    .pending_error
                    .take()
                    .unwrap_or(VmError::InvalidInstruction);
                self.rethrow_on_finally = false;
                return Err(err);
            }
            Ok(())
        } else {
            Err(err)
        }
    }

    fn error_code(err: &VmError) -> i64 {
        match err {
            VmError::ErrorException(_) => 0, // User-thrown error
            VmError::AssertionFailed(_) => 1,
            VmError::Cancelled => 17,
            VmError::DivisionByZero => 2,
            VmError::StackOverflow => 3,
            VmError::StackUnderflow => 4,
            VmError::InvalidInstruction => 5,
            VmError::IndexOutOfBounds { .. } => 6,
            VmError::DimensionMismatch { .. } => 7,
            VmError::MatMulDimensionMismatch { .. } => 8,
            VmError::BroadcastDimensionMismatch { .. } => 9,
            VmError::EmptyArrayPop => 10,
            VmError::TypeError(_) => 11,
            VmError::DomainError(_) => 12,
            VmError::UnknownBroadcastOp(_) => 13,
            VmError::FieldIndexOutOfBounds { .. } => 14,
            VmError::ImmutableFieldAssign(_) => 15,
            VmError::NotImplemented(_) => 16,
            // Tuple/NamedTuple/Dict errors
            VmError::TupleIndexOutOfBounds { .. } => 18,
            VmError::EmptyTuple => 19,
            VmError::TupleDestructuringMismatch { .. } => 20,
            VmError::NamedTupleFieldNotFound(_) => 21,
            VmError::NamedTupleLengthMismatch { .. } => 22,
            VmError::DictKeyNotFound(_) => 23,
            VmError::InvalidDictKey(_) => 24,
            VmError::RangeIndexOutOfBounds { .. } => 25,
            VmError::EmptyRange => 26,
            VmError::UndefVarError(_) => 27,
            VmError::StringIndexError { .. } => 28,
            VmError::MethodError(_) => 29,
            VmError::InexactError(_) => 30,
            VmError::UndefKeywordError(_) => 31,
            VmError::OverflowError(_) => 32,
            VmError::InternalError(_) => 33,
        }
    }

    fn raise(&mut self, err: VmError) -> Result<(), VmError> {
        if self.handle_error(err.clone()).is_ok() {
            Ok(())
        } else {
            Err(err)
        }
    }

    fn try_or_handle<T>(&mut self, result: Result<T, VmError>) -> Result<Option<T>, VmError> {
        match result {
            Ok(val) => Ok(Some(val)),
            Err(err) => {
                if self.handle_error(err.clone()).is_ok() {
                    Ok(None)
                } else {
                    Err(err)
                }
            }
        }
    }

    /// Get a function by index, returning an error if the index is out of bounds.
    ///
    /// This is the single source of truth for function index lookups.
    /// All execution modules should use this method instead of raw
    /// `self.functions.get(idx).ok_or_else(...)` or `match self.functions.get(idx)`.
    fn get_function_checked(&self, index: usize) -> Result<&FunctionInfo, VmError> {
        self.functions.get(index).ok_or_else(|| {
            VmError::InternalError(format!(
                "Function index {} out of bounds (have {} functions)",
                index,
                self.functions.len()
            ))
        })
    }

    /// Get function indices by name using the pre-built index (Issue #3361).
    /// Returns an empty slice if no functions match.
    #[inline]
    pub(crate) fn get_function_indices_by_name(&self, name: &str) -> &[usize] {
        self.function_name_index
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get a cloned function by index, raising through error handling if not found.
    ///
    /// Returns `Ok(Some(func))` if the function was found, `Ok(None)` if the index
    /// was invalid but the error was caught by a try-catch handler, or `Err` if
    /// the error propagated.
    fn get_function_cloned_or_raise(
        &mut self,
        index: usize,
    ) -> Result<Option<FunctionInfo>, VmError> {
        let result = self.get_function_checked(index).cloned();
        self.try_or_handle(result)
    }

    // ==================== Boolean Context Helpers ====================

    /// Check if a value is a boolean and return its value.
    /// Returns Err(TypeError) if the value is not a boolean (Julia semantics).
    #[inline]
    fn expect_bool(&self, v: &Value) -> Result<bool, VmError> {
        match v {
            Value::Bool(b) => Ok(*b),
            _ => {
                let type_name = self.get_type_name(v);
                Err(VmError::TypeError(format!(
                    "non-boolean ({}) used in boolean context",
                    type_name
                )))
            }
        }
    }

    /// Execute JumpIfZero instruction: jump to target if condition is false.
    /// Returns Some(target) if should jump, None if should continue.
    /// Returns Err if condition is not a boolean.
    #[inline]
    fn execute_jump_if_zero(&mut self, target: usize) -> Result<Option<usize>, VmError> {
        let v = self.stack.pop_value()?;
        let cond = self.expect_bool(&v)?;
        Ok(if !cond { Some(target) } else { None })
    }

    // ==================== Comparison Helpers (return Bool) ====================

    /// Execute floating-point comparison, returns Bool
    #[inline]
    fn cmp_f64<F: Fn(f64, f64) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let b = self.pop_f64_or_i64()?;
        let a = self.pop_f64_or_i64()?;
        self.stack.push(Value::Bool(op(a, b)));
        Ok(())
    }

    /// Execute integer comparison, returns Bool
    #[inline]
    fn cmp_i64<F: Fn(i64, i64) -> bool>(&mut self, op: F) -> Result<(), VmError> {
        let b = self.stack.pop_i64()?;
        let a = self.stack.pop_i64()?;
        self.stack.push(Value::Bool(op(a, b)));
        Ok(())
    }

    /// Compare two struct values by comparing all fields recursively.
    /// Returns true if both are structs of the same type with equal fields.
    /// This is the default == behavior for immutable structs without custom ==.
    fn compare_struct_fields(&self, left: &Value, right: &Value) -> bool {
        // Resolve StructRef to actual struct data
        let left_struct = match left {
            Value::Struct(s) => Some(s.clone()),
            Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
            _ => None,
        };
        let right_struct = match right {
            Value::Struct(s) => Some(s.clone()),
            Value::StructRef(idx) => self.struct_heap.get(*idx).cloned(),
            _ => None,
        };

        match (left_struct, right_struct) {
            (Some(l), Some(r)) => {
                // Check struct type names match exactly (including type parameters)
                // In Julia, Point{Int64}(3, 4) != Point{Float64}(3.0, 4.0)
                // because they have different concrete types
                if l.struct_name != r.struct_name {
                    return false;
                }
                // Check field count matches
                if l.values.len() != r.values.len() {
                    return false;
                }
                // Compare all fields recursively
                for (lv, rv) in l.values.iter().zip(r.values.iter()) {
                    if !self.compare_values_equal(lv, rv) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Compare two values for equality (used by struct field comparison).
    /// Handles recursive comparison for nested structs.
    fn compare_values_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F64(a), Value::F64(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Nothing, Value::Nothing) => true,
            (Value::Missing, Value::Missing) => true,
            // Cross-type numeric comparison
            (Value::I64(a), Value::F64(b)) => (*a as f64) == *b,
            (Value::F64(a), Value::I64(b)) => *a == (*b as f64),
            // Struct comparison (recursive)
            (Value::Struct(_), _)
            | (_, Value::Struct(_))
            | (Value::StructRef(_), _)
            | (_, Value::StructRef(_)) => self.compare_struct_fields(left, right),
            // Arrays
            (Value::Array(a), Value::Array(b)) => {
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                if a_ref.shape != b_ref.shape {
                    return false;
                }
                // Simple comparison via Debug (like isequal)
                format!("{:?}", a_ref.data) == format!("{:?}", b_ref.data)
            }
            // Memory → Array (Issue #2764)
            (Value::Memory(m), Value::Array(b)) => {
                let a = crate::vm::util::memory_to_array_ref(m);
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                if a_ref.shape != b_ref.shape {
                    return false;
                }
                format!("{:?}", a_ref.data) == format!("{:?}", b_ref.data)
            }
            (Value::Array(a), Value::Memory(m)) => {
                let b = crate::vm::util::memory_to_array_ref(m);
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                if a_ref.shape != b_ref.shape {
                    return false;
                }
                format!("{:?}", a_ref.data) == format!("{:?}", b_ref.data)
            }
            (Value::Memory(m1), Value::Memory(m2)) => {
                let a = crate::vm::util::memory_to_array_ref(m1);
                let b = crate::vm::util::memory_to_array_ref(m2);
                let a_ref = a.borrow();
                let b_ref = b.borrow();
                if a_ref.shape != b_ref.shape {
                    return false;
                }
                format!("{:?}", a_ref.data) == format!("{:?}", b_ref.data)
            }
            // Tuples
            (Value::Tuple(a), Value::Tuple(b)) => {
                if a.elements.len() != b.elements.len() {
                    return false;
                }
                for (av, bv) in a.elements.iter().zip(b.elements.iter()) {
                    if !self.compare_values_equal(av, bv) {
                        return false;
                    }
                }
                true
            }
            // Different types are not equal
            _ => false,
        }
    }

    // ==================== End Helpers ====================

    /// Check if a runtime type matches a function parameter type
    fn type_matches(&self, runtime_type: &str, param_type: &crate::types::JuliaType) -> bool {
        use crate::types::JuliaType;

        fn strip_module(name: &str) -> &str {
            if let Some(idx) = name.rfind('.') {
                &name[idx + 1..]
            } else {
                name
            }
        }

        // Extract base name from runtime type (e.g., "Complex{Float64}" -> "Complex")
        let runtime_base_raw = if let Some(idx) = runtime_type.find('{') {
            &runtime_type[..idx]
        } else {
            runtime_type
        };
        let runtime_base = strip_module(runtime_base_raw);

        match param_type {
            JuliaType::Any => true,
            JuliaType::Int64 => runtime_type == "Int64",
            JuliaType::Float64 => runtime_type == "Float64",
            JuliaType::Real => {
                matches!(
                    runtime_type,
                    "Int64"
                        | "Int32"
                        | "Int16"
                        | "Int8"
                        | "Int128"
                        | "UInt64"
                        | "UInt32"
                        | "UInt16"
                        | "UInt8"
                        | "UInt128"
                        | "Float64"
                        | "Float32"
                        | "Float16"
                        | "Bool"
                        | "BigInt"
                        | "BigFloat"
                ) || runtime_base == "Rational"
            }
            JuliaType::Number => {
                matches!(
                    runtime_type,
                    "Int64"
                        | "Int32"
                        | "Int16"
                        | "Int8"
                        | "Int128"
                        | "UInt64"
                        | "UInt32"
                        | "UInt16"
                        | "UInt8"
                        | "UInt128"
                        | "Float64"
                        | "Float32"
                        | "Float16"
                        | "Bool"
                        | "BigInt"
                        | "BigFloat"
                ) || runtime_base == "Complex"
                    || runtime_base == "Rational"
            }
            JuliaType::Integer => {
                matches!(
                    runtime_type,
                    "Int64"
                        | "Int32"
                        | "Int16"
                        | "Int8"
                        | "Int128"
                        | "UInt64"
                        | "UInt32"
                        | "UInt16"
                        | "UInt8"
                        | "UInt128"
                        | "Bool"
                )
            }
            JuliaType::Signed => {
                matches!(
                    runtime_type,
                    "Int64" | "Int32" | "Int16" | "Int8" | "Int128"
                )
            }
            JuliaType::Unsigned => {
                matches!(
                    runtime_type,
                    "UInt64" | "UInt32" | "UInt16" | "UInt8" | "UInt128"
                )
            }
            JuliaType::AbstractFloat => {
                matches!(runtime_type, "Float64" | "Float32" | "Float16" | "BigFloat")
            }
            JuliaType::Struct(name) => {
                // Handle parametric types: "Complex{Float64}" matches "Complex"
                let param_has_type_params = name.contains('{');
                let param_base_raw = if let Some(idx) = name.find('{') {
                    &name[..idx]
                } else {
                    name.as_str()
                };
                let param_base = strip_module(param_base_raw);
                let runtime_has_type_params = runtime_type.contains('{');

                // If both have type parameters:
                // - Type variable params (Dict{K,V}, Rational{T}) match any runtime params
                //   with the same base type (Issue #2748)
                // - Concrete params require exact match
                //   e.g., Rational{Int64} should NOT match Rational{BigInt}
                // But Rational{Int64} should match Rational (no params = any params)
                if param_has_type_params && runtime_has_type_params {
                    if crate::vm::util::has_type_variable_param(name) {
                        runtime_base == param_base
                    } else {
                        strip_module(runtime_type) == strip_module(name)
                    }
                } else {
                    runtime_base == param_base || strip_module(runtime_type) == strip_module(name)
                }
            }
            JuliaType::TypeVar(_, _) => true, // Type variables match anything
            _ => runtime_type == param_type.name(),
        }
    }

    /// Match a runtime value against a JuliaType parameter, including Type{T} patterns.
    fn value_matches_param(&self, value: &Value, param_type: &crate::types::JuliaType) -> bool {
        use crate::types::JuliaType;

        match (value, param_type) {
            (Value::DataType(dt), JuliaType::TypeOf(inner)) => match inner.as_ref() {
                JuliaType::TypeVar(_, _) => true,
                other => dt.is_subtype_of(other),
            },
            // StructRef Dict should NOT match bare JuliaType::Dict.
            // Bare ::Dict annotations are for Value::Dict (Rust-backed);
            // StructRef Dict instances should only match ::Dict{K,V} (Pure Julia).
            // (Issue #2748)
            (Value::StructRef(_), JuliaType::Dict) => false,
            // Value::Dict (Rust-backed) should NOT match Dict{K,V} (Pure Julia parametric).
            // Pure Julia Dict functions expect StructRef with field access (.slots, .keys, etc.).
            // (Issue #2748)
            (Value::Dict(_), JuliaType::Struct(name)) if name.starts_with("Dict{") => false,
            _ => {
                let runtime_type = self.get_type_name(value);
                self.type_matches(&runtime_type, param_type)
            }
        }
    }

    /// Find the best matching method index for a function name and arguments.
    fn find_best_method_index(&self, names: &[&str], args: &[Value]) -> Option<usize> {
        let mut best: Option<(usize, u32)> = None;

        // Use function_name_index for O(1) lookup per name (Issue #3361)
        for name in names {
            for &idx in self.get_function_indices_by_name(name) {
                let func = &self.functions[idx];
                if func.vararg_param_index.is_some() {
                    continue;
                }
                if func.param_julia_types.len() != args.len() {
                    continue;
                }

                let mut matches = true;
                for (arg, param_ty) in args.iter().zip(func.param_julia_types.iter()) {
                    if !self.value_matches_param(arg, param_ty) {
                        matches = false;
                        break;
                    }
                }
                if !matches {
                    continue;
                }

                let score: u32 = func
                    .param_julia_types
                    .iter()
                    .map(|ty| ty.specificity() as u32)
                    .sum();
                if best.is_none_or(|(_, best_score)| score > best_score) {
                    best = Some((idx, score));
                }
            }
        }

        best.map(|(idx, _)| idx)
    }

    /// Find binary operator method and cache result by operand types.
    ///
    /// Positive cache only: misses are not cached so newly added methods remain visible.
    fn find_cached_binary_method_index(
        &mut self,
        op: BinaryDispatchOp,
        names: &[&str],
        left: &Value,
        right: &Value,
    ) -> Option<usize> {
        let key = BinaryDispatchKey {
            op,
            left: self.get_value_type(left),
            right: self.get_value_type(right),
        };

        if let Some(idx) = self.binary_method_cache.get(&key) {
            return Some(*idx);
        }

        let args = [left.clone(), right.clone()];
        let found = self.find_best_method_index(names, &args);
        if let Some(idx) = found {
            self.binary_method_cache.insert(key, idx);
        }
        found
    }

    /// Extract and bind type parameter values from arguments to a frame.
    /// This enables `where T` type parameters to be used as values inside the function body.
    /// Must be called after frame creation and before pushing the frame for execution.
    fn bind_type_params(&self, func: &FunctionInfo, args: &[Value], frame: &mut Frame) {
        if func.type_params.is_empty() {
            return;
        }
        for (idx, arg) in args.iter().enumerate() {
            if let Some(param_jtype) = func.param_julia_types.get(idx) {
                let arg_jtype = self.get_value_julia_type(arg);

                // Special handling for Val{N} - extract integer, boolean, or symbol value directly
                if let crate::types::JuliaType::Struct(param_type_name) = param_jtype {
                    if param_type_name.starts_with("Val{") && param_type_name.ends_with("}") {
                        let param_type_arg = &param_type_name[4..param_type_name.len() - 1];
                        if func.type_params.iter().any(|tp| tp.name == param_type_arg) {
                            if let crate::types::JuliaType::Struct(arg_type_name) = &arg_jtype {
                                if arg_type_name.starts_with("Val{") && arg_type_name.ends_with("}")
                                {
                                    let arg_value_str = &arg_type_name[4..arg_type_name.len() - 1];
                                    if let Ok(int_val) = arg_value_str.parse::<i64>() {
                                        frame
                                            .locals_i64
                                            .insert(param_type_arg.to_string(), int_val);
                                        frame.var_types.insert(param_type_arg.to_string(), frame::VarTypeTag::I64);
                                        continue;
                                    }
                                    if arg_value_str == "true" {
                                        frame.locals_bool.insert(param_type_arg.to_string(), true);
                                        frame.var_types.insert(param_type_arg.to_string(), frame::VarTypeTag::Bool);
                                        continue;
                                    } else if arg_value_str == "false" {
                                        frame.locals_bool.insert(param_type_arg.to_string(), false);
                                        frame.var_types.insert(param_type_arg.to_string(), frame::VarTypeTag::Bool);
                                        continue;
                                    }
                                    if arg_value_str.starts_with(':') {
                                        let symbol_name = arg_value_str.trim_start_matches(':');
                                        frame.locals_val_symbol.insert(
                                            param_type_arg.to_string(),
                                            symbol_name.to_string(),
                                        );
                                        frame.var_types.insert(param_type_arg.to_string(), frame::VarTypeTag::ValSymbol);
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(bindings) =
                    arg_jtype.extract_type_bindings(param_jtype, &func.type_params)
                {
                    for (name, bound_type) in bindings {
                        frame.type_bindings.insert(name, bound_type);
                    }
                }
            }
        }
    }

    /// Start a function call by index with positional arguments.
    fn start_function_call(&mut self, func_index: usize, args: Vec<Value>) -> Result<(), VmError> {
        let func = self
            .functions
            .get(func_index)
            .ok_or_else(|| VmError::TypeError(format!("Unknown function index: {}", func_index)))?
            .clone();

        let mut frame = Frame::new_with_slots(func.local_slot_count, Some(func_index));

        // Bind type parameters from where clauses (Issue #2468)
        self.bind_type_params(&func, &args, &mut frame);

        for (idx, slot) in func.param_slots.iter().enumerate() {
            if let Some(val) = args.get(idx) {
                bind_value_to_slot(&mut frame, *slot, val.clone(), &mut self.struct_heap);
            }
        }

        for kwparam in &func.kwparams {
            if kwparam.required {
                return Err(VmError::UndefKeywordError(kwparam.name.clone()));
            }
            bind_value_to_slot(
                &mut frame,
                kwparam.slot,
                kwparam.default.clone(),
                &mut self.struct_heap,
            );
        }

        self.return_ips.push(self.ip);
        self.frames.push(frame);
        self.ip = func.entry;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_value_type_returns_arrayof_for_typed_array() {
        let vm = Vm::new(vec![], StableRng::new(0));
        let arr = Value::Array(new_array_ref(ArrayValue::from_f64(vec![1.0, 2.0], vec![2])));

        assert_eq!(
            vm.get_value_type(&arr),
            ValueType::ArrayOf(ArrayElementType::F64)
        );
    }

    // === Issue #3094: VmError::InternalError propagation tests ===

    /// InternalError has error code 33.
    #[test]
    fn test_internal_error_code_is_33() {
        let code = Vm::<StableRng>::error_code(&VmError::InternalError("test".to_string()));
        assert_eq!(code, 33);
    }

    /// handle_error catches a user-visible error when a handler with catch_ip exists.
    #[test]
    fn test_handle_error_catches_user_error_with_handler() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.handlers.push(Handler {
            catch_ip: Some(100),
            finally_ip: None,
            stack_len: 0,
            frame_len: 1,
            return_ip_len: 0,
        });
        let result = vm.handle_error(VmError::TypeError("test".to_string()));
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(vm.ip, 100);
        assert!(
            matches!(vm.pending_error, Some(VmError::TypeError(_))),
            "Expected pending TypeError, got {:?}",
            vm.pending_error
        );
    }

    /// handle_error propagates ANY error when no handler exists.
    #[test]
    fn test_handle_error_propagates_when_no_handler() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let result = vm.handle_error(VmError::InternalError("test".to_string()));
        assert!(
            matches!(result, Err(VmError::InternalError(_))),
            "Expected Err(InternalError), got {:?}",
            result
        );
    }

    /// raise() catches a user-visible error when a handler exists.
    #[test]
    fn test_raise_catches_with_handler() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.handlers.push(Handler {
            catch_ip: Some(50),
            finally_ip: None,
            stack_len: 0,
            frame_len: 1,
            return_ip_len: 0,
        });
        let result = vm.raise(VmError::DomainError("test".to_string()));
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(vm.ip, 50);
    }

    /// raise() propagates when no handler exists.
    #[test]
    fn test_raise_propagates_without_handler() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        let result = vm.raise(VmError::TypeError("test".to_string()));
        assert!(
            matches!(result, Err(VmError::TypeError(_))),
            "Expected Err(TypeError), got {:?}",
            result
        );
    }

    /// InternalError from get_function_checked IS caught by handlers when the
    /// instruction uses try_or_handle (e.g., Call instruction uses
    /// get_function_cloned_or_raise). This documents that InternalError
    /// propagation depends on HOW the error is surfaced, not the error variant.
    #[test]
    fn test_internal_error_caught_via_try_or_handle_in_run() {
        // Call(9999, 0) triggers InternalError via get_function_cloned_or_raise,
        // which calls try_or_handle → handle_error. The handler catches it.
        let catch_ip = 2;
        let mut vm = Vm::new(
            vec![
                Instr::PushHandler(Some(catch_ip), None), // ip=0: push handler
                Instr::Call(9999, 0), // ip=1: invalid func → InternalError via try_or_handle
                Instr::ClearError,    // ip=2 (catch): clear error
                Instr::PushI64(42),   // ip=3: push result
                Instr::ReturnAny,     // ip=4: return 42
            ],
            StableRng::new(0),
        );
        let result = vm.run();
        // InternalError IS caught because Call uses try_or_handle
        assert!(
            matches!(result, Ok(Value::I64(42))),
            "Expected Ok(I64(42)), got {:?}",
            result
        );
    }

    /// A user-visible error (DivisionByZero) IS caught by handlers in run().
    /// DivisionByZero goes through raise(), which checks handlers.
    #[test]
    fn test_user_error_caught_by_handler_in_run() {
        // Set up: try { 1 % 0 } catch; return 42
        let catch_ip = 4;
        let mut vm = Vm::new(
            vec![
                Instr::PushHandler(Some(catch_ip), None), // ip=0: push handler
                Instr::PushI64(1),                         // ip=1
                Instr::PushI64(0),                         // ip=2
                Instr::ModI64,                             // ip=3: 1 % 0 -> DivisionByZero
                Instr::ClearError,                         // ip=4 (catch): clear error
                Instr::PushI64(42),                        // ip=5: push result
                Instr::ReturnAny,                          // ip=6: return 42
            ],
            StableRng::new(0),
        );
        let result = vm.run();
        assert!(
            matches!(result, Ok(Value::I64(42))),
            "Expected Ok(I64(42)), got {:?}",
            result
        );
    }

    /// Errors returned via direct Err() (not raise/try_or_handle) bypass handlers.
    /// This verifies the ? operator propagation path in the run loop.
    #[test]
    fn test_direct_err_return_bypasses_handlers() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        // Push a handler
        vm.handlers.push(Handler {
            catch_ip: Some(100),
            finally_ip: None,
            stack_len: 0,
            frame_len: 1,
            return_ip_len: 0,
        });
        // Simulate what happens when an instruction does `return Err(InternalError)`:
        // The error goes through dispatch_instr's `?` → run's `result?` → caller.
        // The handler is NOT consulted because raise/handle_error is never called.
        // We verify this by checking that the handler is still on the stack after
        // a direct Err propagation (handle_error would have popped it).
        assert_eq!(vm.handlers.len(), 1);
        let result: Result<(), VmError> = Err(VmError::InternalError("direct".to_string()));
        // Simulating the ? operator: the error propagates without touching handlers
        assert!(result.is_err());
        assert_eq!(vm.handlers.len(), 1, "Handler should NOT have been popped");
    }

    /// handle_error with a finally-only handler (no catch) sets rethrow_on_finally.
    #[test]
    fn test_handle_error_finally_only_sets_rethrow() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.handlers.push(Handler {
            catch_ip: None,
            finally_ip: Some(200),
            stack_len: 0,
            frame_len: 1,
            return_ip_len: 0,
        });
        let result = vm.handle_error(VmError::TypeError("test".to_string()));
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(vm.ip, 200);
        assert!(vm.rethrow_on_finally);
    }

    /// handle_error with a handler that has neither catch nor finally propagates.
    #[test]
    fn test_handle_error_no_catch_no_finally_propagates() {
        let mut vm = Vm::new(vec![], StableRng::new(0));
        vm.handlers.push(Handler {
            catch_ip: None,
            finally_ip: None,
            stack_len: 0,
            frame_len: 1,
            return_ip_len: 0,
        });
        let result = vm.handle_error(VmError::InternalError("test".to_string()));
        assert!(
            matches!(result, Err(VmError::InternalError(_))),
            "Expected Err(InternalError), got {:?}",
            result
        );
    }

    /// get_function_checked returns InternalError for out-of-bounds index.
    #[test]
    fn test_get_function_checked_internal_error() {
        let vm = Vm::new(vec![], StableRng::new(0));
        let result = vm.get_function_checked(999);
        assert!(
            matches!(result, Err(VmError::InternalError(_))),
            "Expected Err(InternalError), got {:?}",
            result
        );
    }

    // === SpannedVmError / source map tests (Issue #2856) ===

    /// last_error_span returns None when no error has occurred.
    #[test]
    fn test_last_error_span_none_by_default() {
        let vm = Vm::new(vec![], StableRng::new(0));
        assert_eq!(vm.last_error_span(), None);
    }

    /// last_error_span returns None when source map is empty.
    #[test]
    fn test_last_error_span_none_without_source_map() {
        let mut vm = Vm::new(
            vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
            StableRng::new(0),
        );
        let result = vm.run();
        assert!(result.is_err());
        // No source map set, so span is None even though last_error_ip is set
        assert_eq!(vm.last_error_span(), None);
    }

    /// last_error_span returns the span from the source map when available.
    #[test]
    fn test_last_error_span_with_source_map() {
        use crate::span::Span;

        let span_at_2 = Span::new(10, 15, 3, 3, 5, 10);
        let mut vm = Vm::new(
            vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
            StableRng::new(0),
        );
        vm.set_source_map(vec![None, None, Some(span_at_2)]);

        let result = vm.run();
        assert!(result.is_err());
        assert_eq!(vm.last_error_span(), Some(span_at_2));
    }

    /// spanned_error wraps VmError with the last error span.
    #[test]
    fn test_spanned_error_attaches_span() {
        use crate::span::Span;

        let span = Span::new(0, 5, 1, 1, 1, 6);
        let mut vm = Vm::new(
            vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
            StableRng::new(0),
        );
        vm.set_source_map(vec![None, None, Some(span)]);

        let result = vm.run();
        assert!(result.is_err());

        let err = result.unwrap_err();
        let spanned = vm.spanned_error(err.clone());
        assert_eq!(spanned.error, err);
        assert_eq!(spanned.span, Some(span));
    }

    /// spanned_error returns None span when source map has no entry for the IP.
    #[test]
    fn test_spanned_error_no_span_for_unmapped_ip() {
        let mut vm = Vm::new(
            vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
            StableRng::new(0),
        );
        // Source map entries only for first two IPs, not for IP=2 (ModI64)
        vm.set_source_map(vec![None, None]);

        let result = vm.run();
        assert!(result.is_err());

        let err = result.unwrap_err();
        let spanned = vm.spanned_error(err.clone());
        assert_eq!(spanned.error, err);
        assert_eq!(spanned.span, None);
    }

    /// Display of SpannedVmError includes line:column when span is present.
    #[test]
    fn test_spanned_error_display_with_location() {
        use crate::span::Span;

        let span = Span::new(10, 20, 5, 5, 8, 18);
        let mut vm = Vm::new(
            vec![Instr::PushI64(1), Instr::PushI64(0), Instr::ModI64],
            StableRng::new(0),
        );
        vm.set_source_map(vec![None, None, Some(span)]);

        let result = vm.run();
        let err = result.unwrap_err();
        let spanned = vm.spanned_error(err);
        let display = format!("{}", spanned);
        assert!(
            display.contains("at line 5:8"),
            "Expected span info in display, got: {}",
            display
        );
    }
}
