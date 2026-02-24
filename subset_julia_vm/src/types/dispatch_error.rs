//! Error types for method dispatch.

use super::julia_type::JuliaType;

/// Error types for method dispatch.
#[derive(Debug, Clone)]
pub enum DispatchError {
    /// No method found matching the given argument types.
    NoMethodFound {
        name: String,
        arg_types: Vec<JuliaType>,
    },
    /// Multiple methods match with equal specificity.
    AmbiguousMethod {
        name: String,
        arg_types: Vec<JuliaType>,
        candidates: Vec<Vec<JuliaType>>,
    },
}

impl std::fmt::Display for DispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DispatchError::NoMethodFound { name, arg_types } => {
                let types: Vec<_> = arg_types.iter().map(|t| format!("::{}", t)).collect();
                write!(
                    f,
                    "MethodError: no method matching {}({})",
                    name,
                    types.join(", ")
                )
            }
            DispatchError::AmbiguousMethod {
                name,
                arg_types,
                candidates,
            } => {
                let types: Vec<_> = arg_types.iter().map(|t| format!("::{}", t)).collect();
                let mut msg = format!(
                    "MethodError: {}({}) is ambiguous. Candidates:\n",
                    name,
                    types.join(", ")
                );
                for sig in candidates {
                    let sig_str: Vec<_> = sig.iter().map(|t| format!("::{}", t)).collect();
                    msg.push_str(&format!("  {}({})\n", name, sig_str.join(", ")));
                }
                write!(f, "{}", msg)
            }
        }
    }
}

impl std::error::Error for DispatchError {}
