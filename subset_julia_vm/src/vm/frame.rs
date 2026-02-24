use super::value::{
    ArrayRef, DictValue, GeneratorValue, NamedTupleValue, RangeValue, SymbolValue, TupleValue,
    Value,
};
use crate::rng::RngInstance;
use crate::types::JuliaType;
use half::f16;
use std::collections::{HashMap, HashSet};

/// Tag identifying which typed HashMap a variable is stored in.
/// Enables O(1) lookup/removal instead of cascading through all maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VarTypeTag {
    I64,
    F64,
    F32,
    F16,
    Str,
    Char,
    Array,
    Tuple,
    NamedTuple,
    Dict,
    Struct,
    Range,
    Rng,
    Generator,
    Any,
    NarrowInt,
    Nothing,
    Bool,
    ValSymbol,
}

#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub locals_slots: Vec<Option<Value>>,
    pub locals_i64: HashMap<String, i64>,
    pub locals_f64: HashMap<String, f64>,
    pub locals_f32: HashMap<String, f32>,
    pub locals_f16: HashMap<String, f16>,
    pub locals_array: HashMap<String, ArrayRef>,
    // Complex is now stored in locals_struct (heap allocation)
    pub locals_str: HashMap<String, String>,
    pub locals_char: HashMap<String, char>,
    pub locals_struct: HashMap<String, usize>, // Index into VM's struct_heap (includes Complex)
    pub locals_rng: HashMap<String, RngInstance>,
    pub locals_range: HashMap<String, RangeValue>,
    pub locals_tuple: HashMap<String, TupleValue>,
    pub locals_named_tuple: HashMap<String, NamedTupleValue>,
    pub locals_dict: HashMap<String, Box<DictValue>>,
    pub locals_generator: HashMap<String, GeneratorValue>,
    pub locals_any: HashMap<String, Value>,
    /// Narrow integer types (I8/I16/I32/I128/U8–U128) stored as Value to preserve type info.
    /// Separate from locals_any to avoid mixing with untyped catch-all values.
    pub locals_narrow_int: HashMap<String, Value>,
    pub locals_nothing: HashSet<String>, // Track variables holding Nothing
    /// Type parameter bindings from where clauses (e.g., T -> Float64)
    pub type_bindings: HashMap<String, JuliaType>,
    /// Boolean type parameters from Val{true}/Val{false} patterns
    pub locals_bool: HashMap<String, bool>,
    /// Symbol type parameters from Val{:symbol} patterns
    pub locals_val_symbol: HashMap<String, String>,
    pub func_index: Option<usize>,
    /// Captured variables from closure environment.
    /// When calling a closure, these values are populated from the ClosureValue's captures.
    pub captured_vars: HashMap<String, Value>,
    /// Type tag cache: tracks which typed map each variable is stored in.
    /// Enables O(1) lookup dispatch and O(1) removal in StoreAny.
    pub var_types: HashMap<String, VarTypeTag>,
}

impl Frame {
    pub fn new() -> Self {
        Self::new_with_slots(0, None)
    }

    pub fn new_with_slots(slot_count: usize, func_index: Option<usize>) -> Self {
        Self {
            locals_slots: vec![None; slot_count],
            locals_i64: HashMap::new(),
            locals_f64: HashMap::new(),
            locals_f32: HashMap::new(),
            locals_f16: HashMap::new(),
            locals_array: HashMap::new(),
            // Complex is now stored in locals_struct
            locals_str: HashMap::new(),
            locals_char: HashMap::new(),
            locals_struct: HashMap::new(),
            locals_rng: HashMap::new(),
            locals_range: HashMap::new(),
            locals_tuple: HashMap::new(),
            locals_named_tuple: HashMap::new(),
            locals_dict: HashMap::new(),
            locals_generator: HashMap::new(),
            locals_any: HashMap::new(),
            locals_narrow_int: HashMap::new(),
            locals_nothing: HashSet::new(),
            type_bindings: HashMap::new(),
            locals_bool: HashMap::new(),
            locals_val_symbol: HashMap::new(),
            func_index,
            captured_vars: HashMap::new(),
            var_types: HashMap::new(),
        }
    }

    /// O(1) variable lookup: check tag first, fall back to cascade for untagged vars.
    pub fn get_local(&self, name: &str) -> Option<Value> {
        if let Some(tag) = self.var_types.get(name) {
            self.get_by_tag(name, *tag)
        } else {
            self.get_by_cascade(name)
        }
    }

    /// Direct lookup using the type tag -- O(1) dispatch to the correct map.
    fn get_by_tag(&self, name: &str, tag: VarTypeTag) -> Option<Value> {
        match tag {
            VarTypeTag::I64 => self.locals_i64.get(name).map(|v| Value::I64(*v)),
            VarTypeTag::F64 => self.locals_f64.get(name).map(|v| Value::F64(*v)),
            VarTypeTag::F32 => self.locals_f32.get(name).map(|v| Value::F32(*v)),
            VarTypeTag::F16 => self.locals_f16.get(name).map(|v| Value::F16(*v)),
            VarTypeTag::Str => self.locals_str.get(name).map(|v| Value::Str(v.clone())),
            VarTypeTag::Char => self.locals_char.get(name).map(|v| Value::Char(*v)),
            VarTypeTag::Array => self.locals_array.get(name).map(|v| Value::Array(v.clone())),
            VarTypeTag::Tuple => self.locals_tuple.get(name).map(|v| Value::Tuple(v.clone())),
            VarTypeTag::NamedTuple => self
                .locals_named_tuple
                .get(name)
                .map(|v| Value::NamedTuple(v.clone())),
            VarTypeTag::Dict => self.locals_dict.get(name).map(|v| Value::Dict(v.clone())),
            VarTypeTag::Struct => self.locals_struct.get(name).map(|v| Value::StructRef(*v)),
            VarTypeTag::Range => self.locals_range.get(name).map(|v| Value::Range(v.clone())),
            VarTypeTag::Rng => self.locals_rng.get(name).map(|v| Value::Rng(v.clone())),
            VarTypeTag::Generator => self
                .locals_generator
                .get(name)
                .map(|v| Value::Generator(v.clone())),
            VarTypeTag::Any => self.locals_any.get(name).cloned(),
            VarTypeTag::NarrowInt => self.locals_narrow_int.get(name).cloned(),
            VarTypeTag::Nothing => {
                if self.locals_nothing.contains(name) {
                    Some(Value::Nothing)
                } else {
                    None
                }
            }
            VarTypeTag::Bool => self.locals_bool.get(name).map(|v| Value::Bool(*v)),
            VarTypeTag::ValSymbol => self
                .locals_val_symbol
                .get(name)
                .map(|v| Value::Symbol(SymbolValue::new(v))),
        }
    }

    /// Fallback linear search for variables without a tag (safety net).
    fn get_by_cascade(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.locals_narrow_int.get(name) {
            return Some(v.clone());
        } else if let Some(v) = self.locals_any.get(name) {
            return Some(v.clone());
        } else if let Some(v) = self.locals_array.get(name) {
            return Some(Value::Array(v.clone()));
        } else if let Some(v) = self.locals_i64.get(name) {
            return Some(Value::I64(*v));
        } else if let Some(v) = self.locals_f64.get(name) {
            return Some(Value::F64(*v));
        } else if let Some(v) = self.locals_f32.get(name) {
            return Some(Value::F32(*v));
        } else if let Some(v) = self.locals_f16.get(name) {
            return Some(Value::F16(*v));
        } else if let Some(v) = self.locals_str.get(name) {
            return Some(Value::Str(v.clone()));
        } else if let Some(v) = self.locals_char.get(name) {
            return Some(Value::Char(*v));
        } else if let Some(v) = self.locals_tuple.get(name) {
            return Some(Value::Tuple(v.clone()));
        } else if let Some(v) = self.locals_named_tuple.get(name) {
            return Some(Value::NamedTuple(v.clone()));
        } else if let Some(v) = self.locals_dict.get(name) {
            return Some(Value::Dict(v.clone()));
        } else if let Some(&idx) = self.locals_struct.get(name) {
            return Some(Value::StructRef(idx));
        } else if let Some(v) = self.locals_range.get(name) {
            return Some(Value::Range(v.clone()));
        } else if let Some(v) = self.locals_rng.get(name) {
            return Some(Value::Rng(v.clone()));
        } else if let Some(v) = self.locals_generator.get(name) {
            return Some(Value::Generator(v.clone()));
        } else if self.locals_nothing.contains(name) {
            return Some(Value::Nothing);
        } else if let Some(v) = self.locals_bool.get(name) {
            return Some(Value::Bool(*v));
        }
        self.locals_val_symbol
            .get(name)
            .map(|v| Value::Symbol(SymbolValue::new(v)))
    }

    /// O(1) removal: remove variable from its tagged map, then clear the tag.
    pub fn remove_var(&mut self, name: &str) {
        if let Some(tag) = self.var_types.remove(name) {
            self.remove_by_tag(name, tag);
        } else {
            self.remove_from_all(name);
        }
    }

    /// Targeted removal from a specific typed map.
    fn remove_by_tag(&mut self, name: &str, tag: VarTypeTag) {
        match tag {
            VarTypeTag::I64 => { self.locals_i64.remove(name); }
            VarTypeTag::F64 => { self.locals_f64.remove(name); }
            VarTypeTag::F32 => { self.locals_f32.remove(name); }
            VarTypeTag::F16 => { self.locals_f16.remove(name); }
            VarTypeTag::Str => { self.locals_str.remove(name); }
            VarTypeTag::Char => { self.locals_char.remove(name); }
            VarTypeTag::Array => { self.locals_array.remove(name); }
            VarTypeTag::Tuple => { self.locals_tuple.remove(name); }
            VarTypeTag::NamedTuple => { self.locals_named_tuple.remove(name); }
            VarTypeTag::Dict => { self.locals_dict.remove(name); }
            VarTypeTag::Struct => { self.locals_struct.remove(name); }
            VarTypeTag::Range => { self.locals_range.remove(name); }
            VarTypeTag::Rng => { self.locals_rng.remove(name); }
            VarTypeTag::Generator => { self.locals_generator.remove(name); }
            VarTypeTag::Any => { self.locals_any.remove(name); }
            VarTypeTag::NarrowInt => { self.locals_narrow_int.remove(name); }
            VarTypeTag::Nothing => { self.locals_nothing.remove(name); }
            VarTypeTag::Bool => { self.locals_bool.remove(name); }
            VarTypeTag::ValSymbol => { self.locals_val_symbol.remove(name); }
        }
    }

    /// Fallback: remove from all typed maps (for untagged variables).
    fn remove_from_all(&mut self, name: &str) {
        self.locals_i64.remove(name);
        self.locals_f64.remove(name);
        self.locals_f32.remove(name);
        self.locals_f16.remove(name);
        self.locals_str.remove(name);
        self.locals_char.remove(name);
        self.locals_array.remove(name);
        self.locals_tuple.remove(name);
        self.locals_named_tuple.remove(name);
        self.locals_dict.remove(name);
        self.locals_struct.remove(name);
        self.locals_range.remove(name);
        self.locals_rng.remove(name);
        self.locals_generator.remove(name);
        self.locals_any.remove(name);
        self.locals_narrow_int.remove(name);
        self.locals_nothing.remove(name);
        self.locals_bool.remove(name);
        self.locals_val_symbol.remove(name);
    }

    /// Create a new frame with captured variables from a closure.
    pub fn new_with_captures(
        slot_count: usize,
        func_index: Option<usize>,
        captures: Vec<(String, Value)>,
    ) -> Self {
        let mut frame = Self::new_with_slots(slot_count, func_index);
        for (name, value) in captures {
            frame.captured_vars.insert(name, value);
        }
        frame
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Handler {
    pub catch_ip: Option<usize>,
    pub finally_ip: Option<usize>,
    pub stack_len: usize,
    pub frame_len: usize,
    pub return_ip_len: usize,
}

/// Kind of higher-order function operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Variant coverage intentionally includes staged op kinds used by dispatch tables.
#[allow(dead_code)]
pub(crate) enum HofOpKind {
    Broadcast,         // Original broadcast: apply f to each element
    Broadcast2,        // broadcast(f, A, B): apply f to each element pair with shape broadcasting
    Broadcast2InPlace, // broadcast!(f, dest, A, B): in-place version of Broadcast2
    // Note: Map, Filter, Reduce, Foldr, ForEach removed - now Pure Julia
    MapInPlace,    // map!(f, dest, src): apply f to each element of src, store in dest
    FilterInPlace, // filter!(f, arr): filter elements in-place
    MapReduce,     // mapreduce(f, op, arr): apply f, then reduce with op
    MapFoldr,      // mapfoldr(f, op, arr): apply f, then right-fold with op
    Sum,           // sum(f, arr): apply f to each element and sum the results
    Any,           // any(f, arr): check if f returns true for any element
    All,           // all(f, arr): check if f returns true for all elements
    Count,         // count(f, arr): count elements where f returns true
    FindAll,       // findall(f, arr): return Int64 indices where f returns true
    FindFirst,     // findfirst(f, arr): return first index where f returns true, or nothing
    FindLast,      // findlast(f, arr): return last index where f returns true, or nothing
    Ntuple,        // ntuple(f, n): apply f to 1..n, collect into tuple
    TupleMap,      // map(f, tuple): apply f to each tuple element, return tuple
}

/// Input data for broadcast/HOF operations - supports both f64 and struct arrays
#[derive(Debug, Clone)]
pub(crate) enum BroadcastInput {
    /// F64 array data (legacy path)
    F64(Vec<f64>),
    /// Values from TypedArray (supports any element type including structs)
    Values(Vec<Value>),
}

impl BroadcastInput {
    pub fn get(&self, index: usize) -> Option<Value> {
        match self {
            BroadcastInput::F64(v) => v.get(index).map(|&x| Value::F64(x)),
            BroadcastInput::Values(v) => v.get(index).cloned(),
        }
    }
}

/// Result storage for broadcast/HOF operations
#[derive(Debug, Clone)]
pub(crate) enum BroadcastResults {
    /// F64 results (legacy path)
    F64(Vec<f64>),
    /// Value results (for struct arrays and mixed types)
    Values(Vec<Value>),
}

impl BroadcastResults {
    pub fn new_f64(capacity: usize) -> Self {
        BroadcastResults::F64(Vec::with_capacity(capacity))
    }

    pub fn new_values(capacity: usize) -> Self {
        BroadcastResults::Values(Vec::with_capacity(capacity))
    }

    pub fn is_empty(&self) -> bool {
        match self {
            BroadcastResults::F64(v) => v.is_empty(),
            BroadcastResults::Values(v) => v.is_empty(),
        }
    }

    pub fn clear(&mut self) {
        match self {
            BroadcastResults::F64(v) => v.clear(),
            BroadcastResults::Values(v) => v.clear(),
        }
    }

    pub fn push_f64(&mut self, val: f64) {
        match self {
            BroadcastResults::F64(v) => v.push(val),
            BroadcastResults::Values(v) => v.push(Value::F64(val)),
        }
    }

    pub fn push_i64(&mut self, val: i64) {
        match self {
            BroadcastResults::F64(v) => v.push(val as f64),
            BroadcastResults::Values(v) => v.push(Value::I64(val)),
        }
    }

    pub fn push_value(&mut self, val: Value) {
        match self {
            BroadcastResults::F64(v) => {
                if let Value::F64(f) = val {
                    v.push(f);
                } else if let Value::I64(i) = val {
                    v.push(i as f64);
                }
            }
            BroadcastResults::Values(v) => v.push(val),
        }
    }

    pub fn take_f64(&mut self) -> Vec<f64> {
        match self {
            BroadcastResults::F64(v) => std::mem::take(v),
            BroadcastResults::Values(v) => v
                .drain(..)
                .map(|val| match val {
                    Value::F64(f) => f,
                    Value::I64(i) => i as f64,
                    _ => 0.0,
                })
                .collect(),
        }
    }

    pub fn take_i64(&mut self) -> Vec<i64> {
        match self {
            BroadcastResults::F64(v) => v.drain(..).map(|f| f as i64).collect(),
            BroadcastResults::Values(v) => v
                .drain(..)
                .map(|val| match val {
                    Value::I64(i) => i,
                    Value::F64(f) => f as i64,
                    _ => 0,
                })
                .collect(),
        }
    }

    pub fn take_values(&mut self) -> Vec<Value> {
        match self {
            BroadcastResults::F64(v) => v.drain(..).map(Value::F64).collect(),
            BroadcastResults::Values(v) => std::mem::take(v),
        }
    }
}

/// State for user-defined function broadcast execution
pub(crate) struct BroadcastState {
    pub func_index: usize,
    /// Input data - can be f64 array or values array
    pub input: BroadcastInput,
    pub input_shape: Vec<usize>,
    /// Second input for broadcast(f, A, B) - None for single-array HOF
    pub input2: Option<BroadcastInput>,
    pub input2_shape: Option<Vec<usize>>,
    /// Result shape after broadcasting (for Broadcast2 mode)
    pub result_shape: Option<Vec<usize>>,
    /// Destination array for in-place operations (broadcast!)
    pub dest_array: Option<super::value::ArrayRef>,
    /// Results storage - can be f64 or values
    pub results: BroadcastResults,
    pub current_index: usize,
    pub return_ip_after_broadcast: usize,
    /// Kind of HOF operation
    pub op_kind: HofOpKind,
    /// For reduce: the accumulator value (changed from f64 to Value for flexibility)
    pub accumulator: Option<Value>,
    /// Extra arguments for broadcast (e.g., Ref(x) in f.(arr, Ref(x)))
    pub extra_args: Vec<Value>,
    /// Frame depth when HOF function is called - used to detect when HOF function returns
    /// (vs when nested functions inside the HOF function body return)
    pub hof_frame_depth: usize,
    /// Whether we're using the Value-based path (for struct arrays)
    pub is_value_mode: bool,
    /// For mapreduce: the reduce function index (separate from the map func_index)
    pub reduce_func_index: Option<usize>,
}

/// State for composed function call execution: (f ∘ g)(x) = f(g(x))
/// When calling a composed function, we first call the inner function,
/// then when it returns, we call the outer function with the result.
pub(crate) struct ComposedCallState {
    /// Stack of pending outer functions to call (in order: first to pop is next to call)
    /// For (a ∘ b ∘ c)(x), this will be [a, b] and c is called first
    pub pending_outers: Vec<super::value::Value>,
    /// Return IP after the entire composed call completes
    pub return_ip: usize,
    /// Frame depth when composed call started - used to detect return
    pub call_frame_depth: usize,
}

/// State for sprint function call execution.
/// sprint(f, args...) calls f(io, args...) and returns the IOBuffer content as a string.
pub(crate) struct SprintState {
    /// The IOBuffer being written to (with interior mutability)
    pub io: super::value::IORef,
    /// Return IP after sprint completes
    pub return_ip: usize,
    /// Frame depth when sprint call started - used to detect when f returns
    pub call_frame_depth: usize,
}
