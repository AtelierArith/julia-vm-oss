//! Generator value types.
//!
//! Split out of `container.rs` by value kind (Issue #6835).

use super::ArrayElementType;
use super::Value;
use crate::types::JuliaType;

/// Generator value: lazy iterator that applies a function to each element.
///
/// Julia's Generator is defined as:
/// ```julia
/// struct Generator{I, F}
///     f::F
///     iter::I
/// end
/// ```
///
/// When iterated, it yields `f(x)` for each `x` in `iter`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum GeneratorCallable {
    FunctionIndex(usize),
    FilteredFunctionIndex {
        map_func_index: usize,
        predicate_func_index: usize,
    },
    TupleSplatFunctionIndex(usize),
    TypeObject(JuliaType),
    TupleSplatTypeObject(JuliaType),
    RuntimeValue(Box<Value>),
    TupleSplatRuntimeValue(Box<Value>),
    Eager,
}

#[derive(Debug, Clone)]
pub struct GeneratorValue {
    /// Callable applied to each element.
    pub callable: GeneratorCallable,
    /// The underlying iterator (Array, Range, Tuple, etc.)
    pub iter: Box<Value>,
    /// Upstream Julia's `collect(itr::Generator)` uses `@default_eltype`
    /// before observing a first value. This stores that inferred result
    /// element type for empty generator collection.
    pub result_element_type: Option<ArrayElementType>,
}

impl GeneratorValue {
    pub fn new(func_index: usize, iter: Value) -> Self {
        Self::with_result_element_type(GeneratorCallable::FunctionIndex(func_index), iter, None)
    }

    pub fn with_result_element_type(
        callable: GeneratorCallable,
        iter: Value,
        result_element_type: Option<ArrayElementType>,
    ) -> Self {
        Self {
            callable,
            iter: Box::new(iter),
            result_element_type,
        }
    }

    pub fn eager(iter: Value) -> Self {
        Self::with_result_element_type(GeneratorCallable::Eager, iter, None)
    }
}
