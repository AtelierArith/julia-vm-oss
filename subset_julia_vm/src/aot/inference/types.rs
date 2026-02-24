//! Type definitions for the AoT type inference engine.
//!
//! Contains function signatures, struct type info, typed program,
//! inference results, and call site tracking.

use super::super::types::StaticType;
use std::collections::HashMap;

// ============================================================================
// Supporting Structures
// ============================================================================

/// Type environment mapping variable names to types
pub type TypeEnv = HashMap<String, StaticType>;

/// Function signature with inferred types
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Function name
    pub name: String,
    /// Parameter types
    pub param_types: Vec<StaticType>,
    /// Parameter names
    pub param_names: Vec<String>,
    /// Return type
    pub return_type: StaticType,
    /// Type inference level (1-4)
    pub inference_level: u8,
}

impl FunctionSignature {
    /// Create a new function signature
    pub fn new(
        name: String,
        param_names: Vec<String>,
        param_types: Vec<StaticType>,
        return_type: StaticType,
    ) -> Self {
        let inference_level = Self::compute_level(&param_types, &return_type);
        Self {
            name,
            param_types,
            param_names,
            return_type,
            inference_level,
        }
    }

    /// Compute inference level based on types
    fn compute_level(param_types: &[StaticType], return_type: &StaticType) -> u8 {
        let all_static =
            param_types.iter().all(|t| t.is_fully_static()) && return_type.is_fully_static();

        if all_static {
            1 // Fully static
        } else if param_types
            .iter()
            .any(|t| matches!(t, StaticType::Union { .. }))
            || matches!(return_type, StaticType::Union { .. })
        {
            3 // Conditional
        } else if param_types.iter().any(|t| matches!(t, StaticType::Any))
            || matches!(return_type, StaticType::Any)
        {
            4 // Dynamic
        } else {
            2 // Inferred
        }
    }

    /// Check if this signature is fully static (Level 1)
    pub fn is_fully_static(&self) -> bool {
        self.inference_level == 1
    }
}

/// Struct type information
#[derive(Debug, Clone)]
pub struct StructTypeInfo {
    /// Struct name
    pub name: String,
    /// Field names and types
    pub fields: Vec<(String, StaticType)>,
    /// Whether this struct is mutable
    pub is_mutable: bool,
    /// Parent type (for inheritance)
    pub parent: Option<String>,
    /// Type parameters
    pub type_params: Vec<String>,
}

impl StructTypeInfo {
    /// Create new struct type info
    pub fn new(name: String, is_mutable: bool) -> Self {
        Self {
            name,
            fields: Vec::new(),
            is_mutable,
            parent: None,
            type_params: Vec::new(),
        }
    }

    /// Add a field
    pub fn add_field(&mut self, name: String, ty: StaticType) {
        self.fields.push((name, ty));
    }

    /// Get field type by name
    pub fn get_field_type(&self, name: &str) -> Option<&StaticType> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }
}

/// Type information for a function
#[derive(Debug, Clone)]
pub struct TypedFunction {
    /// Function signature
    pub signature: FunctionSignature,
    /// Local variable types
    pub locals: TypeEnv,
    /// Variables needing runtime type guards
    pub needs_guard: Vec<String>,
}

impl TypedFunction {
    /// Create a new typed function
    pub fn new(signature: FunctionSignature) -> Self {
        Self {
            signature,
            locals: HashMap::new(),
            needs_guard: Vec::new(),
        }
    }

    /// Add a local variable type
    pub fn add_local(&mut self, name: String, ty: StaticType) {
        if !ty.is_fully_static() {
            self.needs_guard.push(name.clone());
        }
        self.locals.insert(name, ty);
    }
}

/// Program with type information
#[derive(Debug, Clone)]
pub struct TypedProgram {
    /// Struct type information
    pub structs: HashMap<String, StructTypeInfo>,
    /// Function type information
    pub functions: HashMap<String, Vec<TypedFunction>>,
    /// Global variable types
    pub globals: TypeEnv,
    /// Overall inference level (max of all functions)
    pub inference_level: u8,
}

impl TypedProgram {
    /// Create a new typed program
    pub fn new() -> Self {
        Self {
            structs: HashMap::new(),
            functions: HashMap::new(),
            globals: HashMap::new(),
            inference_level: 1,
        }
    }

    /// Add a struct type
    pub fn add_struct(&mut self, info: StructTypeInfo) {
        self.structs.insert(info.name.clone(), info);
    }

    /// Add a typed function
    pub fn add_function(&mut self, func: TypedFunction) {
        let name = func.signature.name.clone();
        let level = func.signature.inference_level;

        self.functions
            .entry(name)
            .or_default()
            .push(func);

        if level > self.inference_level {
            self.inference_level = level;
        }
    }

    /// Get struct info by name
    pub fn get_struct(&self, name: &str) -> Option<&StructTypeInfo> {
        self.structs.get(name)
    }

    /// Get function signatures by name
    pub fn get_functions(&self, name: &str) -> Option<&Vec<TypedFunction>> {
        self.functions.get(name)
    }

    /// Get total number of function variants
    pub fn function_count(&self) -> usize {
        self.functions.values().map(|v| v.len()).sum()
    }
}

impl Default for TypedProgram {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of type inference for a single expression or statement
#[derive(Debug, Clone)]
pub struct InferenceResult {
    /// Inferred types for each variable
    pub types: TypeEnv,
    /// Whether all types are concrete
    pub is_fully_typed: bool,
    /// Variables that need runtime type checks
    pub needs_guard: Vec<String>,
}

impl InferenceResult {
    /// Create a new empty inference result
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
            is_fully_typed: true,
            needs_guard: Vec::new(),
        }
    }

    /// Add a type binding
    pub fn bind(&mut self, name: String, ty: StaticType) {
        if !ty.is_fully_static() {
            self.is_fully_typed = false;
        }
        self.types.insert(name, ty);
    }

    /// Mark a variable as needing a runtime guard
    pub fn add_guard(&mut self, name: String) {
        if !self.needs_guard.contains(&name) {
            self.needs_guard.push(name);
        }
    }

    /// Get the type of a variable
    pub fn get_type(&self, name: &str) -> Option<&StaticType> {
        self.types.get(name)
    }
}

impl Default for InferenceResult {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Type Inference Engine
// ============================================================================

/// Call site information for function specialization
#[derive(Debug, Clone)]
pub struct CallSite {
    /// Function being called
    pub function: String,
    /// Argument types at this call site
    pub arg_types: Vec<StaticType>,
}
