//! Transfer function registry for type inference.
//!
//! This module provides the central registry for transfer functions (tfuncs),
//! which predict the return type of function calls during abstract interpretation.
//!
//! Transfer functions implement the type-level semantics of Julia functions,
//! allowing the inference engine to determine result types without executing code.

use crate::compile::abstract_interp::StructTypeInfo;
use crate::compile::diagnostics::emit_unknown_function;
use crate::compile::lattice::types::LatticeType;
use std::collections::HashMap;

/// Context for transfer functions that need access to type information.
///
/// This context provides access to the struct table and other type information
/// that transfer functions may need to produce more precise type inference.
#[derive(Debug, Default)]
pub struct TFuncContext<'a> {
    /// Struct type information table (struct name -> StructTypeInfo)
    pub struct_table: Option<&'a HashMap<String, StructTypeInfo>>,
}

impl<'a> TFuncContext<'a> {
    /// Creates a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a context with a struct table.
    pub fn with_struct_table(struct_table: &'a HashMap<String, StructTypeInfo>) -> Self {
        Self {
            struct_table: Some(struct_table),
        }
    }
}

/// Type signature for a contextual transfer function.
///
/// A contextual transfer function takes argument types and a context reference,
/// returning the inferred result type. The context provides access to type
/// information like struct definitions.
pub type ContextualTransferFn = fn(&[LatticeType], &TFuncContext) -> LatticeType;

/// Type signature for a transfer function.
///
/// A transfer function takes argument types and returns the inferred result type.
/// These functions encode type-level knowledge about Julia operations.
///
/// # Examples
/// - `+(Int64, Int64)` → `Int64`
/// - `+(Int64, Float64)` → `Float64`
/// - `length(Array{T})` → `Int64`
pub type TransferFn = fn(&[LatticeType]) -> LatticeType;

/// Registry of transfer functions for type inference.
///
/// The registry maps function names to their transfer functions,
/// which predict return types based on argument types.
///
/// # Design
/// - Functions are registered by name (e.g., "+", "length", "getindex")
/// - Each function has a single transfer function that handles all cases
/// - Transfer functions use pattern matching on argument types
/// - Unknown functions or type combinations return `Top` (Any)
///
/// # Example
/// ```
/// use subset_julia_vm::compile::tfuncs::TransferFunctions;
/// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
///
/// let mut registry = TransferFunctions::new();
/// // In actual usage, you would register transfer functions here
/// // registry.register("+", arithmetic::tfunc_add);
///
/// let args = vec![
///     LatticeType::Concrete(ConcreteType::Int64),
///     LatticeType::Concrete(ConcreteType::Int64),
/// ];
/// // For demonstration, this returns Top since no functions are registered
/// let result = registry.infer_return_type("+", &args);
/// assert_eq!(result, LatticeType::Top);
/// ```
#[derive(Debug)]
pub struct TransferFunctions {
    /// Map from function name to transfer function.
    functions: HashMap<String, TransferFn>,
    /// Map from function name to contextual transfer function.
    /// These are used when context (like struct tables) is available.
    contextual_functions: HashMap<String, ContextualTransferFn>,
}

impl TransferFunctions {
    /// Creates a new, empty transfer function registry.
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            contextual_functions: HashMap::new(),
        }
    }

    /// Registers a transfer function for a given function name.
    ///
    /// # Arguments
    /// - `name`: The function name (e.g., "+", "length", "getindex")
    /// - `tfunc`: The transfer function to register
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::tfuncs::TransferFunctions;
    /// use subset_julia_vm::compile::lattice::types::LatticeType;
    ///
    /// let mut registry = TransferFunctions::new();
    /// registry.register("+", |_args| {
    ///     // Transfer function implementation
    ///     LatticeType::Top
    /// });
    /// ```
    pub fn register(&mut self, name: &str, tfunc: TransferFn) {
        self.functions.insert(name.to_string(), tfunc);
    }

    /// Infers the return type of a function call.
    ///
    /// # Arguments
    /// - `function_name`: The name of the function being called
    /// - `arg_types`: The types of the arguments
    ///
    /// # Returns
    /// The inferred return type, or `Top` if the function is unknown
    /// or the type cannot be determined.
    ///
    /// # Example
    /// ```
    /// use subset_julia_vm::compile::tfuncs::TransferFunctions;
    /// use subset_julia_vm::compile::lattice::types::{ConcreteType, LatticeType};
    ///
    /// let registry = TransferFunctions::new();
    /// let args = vec![
    ///     LatticeType::Concrete(ConcreteType::Int64),
    ///     LatticeType::Concrete(ConcreteType::Float64),
    /// ];
    /// let result = registry.infer_return_type("+", &args);
    /// assert_eq!(result, LatticeType::Top); // Returns Top for unknown function
    /// ```
    pub fn infer_return_type(&self, function_name: &str, arg_types: &[LatticeType]) -> LatticeType {
        if let Some(tfunc) = self.functions.get(function_name) {
            tfunc(arg_types)
        } else {
            // Unknown function: conservatively return Top (Any)
            // Emit diagnostic if enabled
            emit_unknown_function(function_name);
            LatticeType::Top
        }
    }

    /// Registers a contextual transfer function for a given function name.
    ///
    /// Contextual transfer functions have access to type information like
    /// struct definitions, enabling more precise type inference.
    ///
    /// # Arguments
    /// - `name`: The function name (e.g., "getfield")
    /// - `tfunc`: The contextual transfer function to register
    pub fn register_contextual(&mut self, name: &str, tfunc: ContextualTransferFn) {
        self.contextual_functions.insert(name.to_string(), tfunc);
    }

    /// Infers the return type of a function call with context.
    ///
    /// This method first checks for a contextual transfer function, which can
    /// use the provided context (struct table, etc.) for more precise inference.
    /// Falls back to the regular transfer function if no contextual one exists.
    ///
    /// # Arguments
    /// - `function_name`: The name of the function being called
    /// - `arg_types`: The types of the arguments
    /// - `ctx`: The context containing type information
    ///
    /// # Returns
    /// The inferred return type, or `Top` if the function is unknown.
    pub fn infer_return_type_with_context(
        &self,
        function_name: &str,
        arg_types: &[LatticeType],
        ctx: &TFuncContext,
    ) -> LatticeType {
        // First, try contextual transfer function
        if let Some(tfunc) = self.contextual_functions.get(function_name) {
            return tfunc(arg_types, ctx);
        }

        // Fall back to regular transfer function
        self.infer_return_type(function_name, arg_types)
    }

    /// Returns true if a contextual transfer function is registered for the given function name.
    pub fn has_contextual_function(&self, function_name: &str) -> bool {
        self.contextual_functions.contains_key(function_name)
    }

    /// Returns true if a transfer function is registered for the given function name.
    pub fn has_function(&self, function_name: &str) -> bool {
        self.functions.contains_key(function_name)
    }

    /// Returns the number of registered transfer functions.
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Returns true if no transfer functions are registered.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

impl Default for TransferFunctions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;

    fn dummy_tfunc(_args: &[LatticeType]) -> LatticeType {
        LatticeType::Concrete(ConcreteType::Int64)
    }

    #[test]
    fn test_new_registry_is_empty() {
        let registry = TransferFunctions::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_function() {
        let mut registry = TransferFunctions::new();
        registry.register("test_fn", dummy_tfunc);

        assert!(!registry.is_empty());
        assert_eq!(registry.len(), 1);
        assert!(registry.has_function("test_fn"));
    }

    #[test]
    fn test_infer_return_type_registered() {
        let mut registry = TransferFunctions::new();
        registry.register("test_fn", dummy_tfunc);

        let result = registry.infer_return_type("test_fn", &[]);
        assert_eq!(result, LatticeType::Concrete(ConcreteType::Int64));
    }

    #[test]
    fn test_infer_return_type_unknown() {
        let registry = TransferFunctions::new();
        let result = registry.infer_return_type("unknown_fn", &[]);
        assert_eq!(result, LatticeType::Top);
    }

    #[test]
    fn test_has_function() {
        let mut registry = TransferFunctions::new();
        registry.register("exists", dummy_tfunc);

        assert!(registry.has_function("exists"));
        assert!(!registry.has_function("does_not_exist"));
    }

    #[test]
    fn test_multiple_registrations() {
        let mut registry = TransferFunctions::new();
        registry.register("fn1", dummy_tfunc);
        registry.register("fn2", dummy_tfunc);
        registry.register("fn3", dummy_tfunc);

        assert_eq!(registry.len(), 3);
        assert!(registry.has_function("fn1"));
        assert!(registry.has_function("fn2"));
        assert!(registry.has_function("fn3"));
    }
}
