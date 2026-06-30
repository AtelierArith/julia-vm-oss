//! Runtime state for higher-order-function, broadcast, and generator execution.
//!
//! These structs were factored out of `vm/frame.rs` (Issue #6828) so that the
//! frame module holds only call-frame/local-slot machinery. The HOF executor
//! (`vm/hof_exec/`) and the iterator/return runtime own this control-flow state.

use crate::vm::value::{ArrayRef, IORef, Value};

/// Kind of higher-order function operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Variant coverage intentionally includes staged op kinds used by dispatch tables.
#[allow(dead_code)]
pub(crate) enum HofOpKind {
    Broadcast,           // Original broadcast: apply f to each element
    FilterMap,           // Filtered generator: predicate first, map kept values
    BroadcastTupleSplat, // Generator(f, I1, I2, ...): apply f to each tuple element as args
    Broadcast2,          // broadcast(f, A, B): apply f to each element pair with shape broadcasting
    Broadcast2InPlace,   // broadcast!(f, dest, A, B): in-place version of Broadcast2
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
    /// F64 array data (legacy fast path). Unconstructed since the reducer HOF VM
    /// instructions were removed (Issue #6733) — broadcast now uses the `Values`
    /// path — but retained as broadcast infrastructure for a future f64 fast path.
    #[allow(dead_code)]
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
    /// F64 results (legacy fast path). Unconstructed since the reducer HOF VM
    /// instructions were removed (Issue #6733); retained as broadcast
    /// infrastructure (the per-variant arms below are still exercised by the
    /// `Values` path's siblings).
    #[allow(dead_code)]
    F64(Vec<f64>),
    /// Value results (for struct arrays and mixed types)
    Values(Vec<Value>),
}

impl BroadcastResults {
    #[allow(dead_code)] // legacy f64 fast-path constructor; see BroadcastResults::F64 (#6733)
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
    pub runtime_callable: Option<Value>,
    /// Input data - can be f64 array or values array
    pub input: BroadcastInput,
    pub input_shape: Vec<usize>,
    /// Second input for broadcast(f, A, B) - None for single-array HOF
    pub input2: Option<BroadcastInput>,
    pub input2_shape: Option<Vec<usize>>,
    /// Result shape after broadcasting (for Broadcast2 mode)
    pub result_shape: Option<Vec<usize>>,
    /// Destination array for in-place operations (broadcast!)
    pub dest_array: Option<ArrayRef>,
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
    /// Whether the completed value-mode array result should be returned as the
    /// public `Array{T,N}` wrapper instead of the legacy native carrier.
    pub wrap_array_result: bool,
    /// For mapreduce: the reduce function index (separate from the map func_index)
    pub reduce_func_index: Option<usize>,
}

pub(crate) enum RuntimeCallableResult {
    Immediate(Value),
    StartedFrame,
    Raised,
}

/// State for composed function call execution: (f ∘ g)(x) = f(g(x))
/// When calling a composed function, we first call the inner function,
/// then when it returns, we call the outer function with the result.
pub(crate) struct ComposedCallState {
    /// Stack of pending outer functions to call (in order: first to pop is next to call)
    /// For (a ∘ b ∘ c)(x), this will be [a, b] and c is called first
    pub pending_outers: Vec<Value>,
    /// Return IP after the entire composed call completes
    pub return_ip: usize,
    /// Frame depth when composed call started - used to detect return
    pub call_frame_depth: usize,
}

/// Continuation kind for one lazy `iterate(::Generator)` function-callable step.
pub(crate) enum GeneratorIterateKind {
    Map,
    FilterPredicate {
        map_func_index: usize,
        predicate_func_index: usize,
        iter: Value,
        input_value: Value,
    },
    FilterMap,
}

/// State for one lazy `iterate(::Generator)` function-callable step.
/// The VM starts normal function frames for `g.f(value)` and uses this state
/// to wrap the returned mapped value with the saved next iterator state.
pub(crate) struct GeneratorIterateState {
    pub next_state: Value,
    pub return_ip: usize,
    pub call_frame_depth: usize,
    pub kind: GeneratorIterateKind,
}

/// State for sprint function call execution.
/// sprint(f, args...) calls f(io, args...) and returns the IOBuffer content as a string.
pub(crate) struct SprintState {
    /// The IOBuffer being written to (with interior mutability)
    pub io: IORef,
    /// Return IP after sprint completes
    pub return_ip: usize,
    /// Frame depth when sprint call started - used to detect when f returns
    pub call_frame_depth: usize,
}
