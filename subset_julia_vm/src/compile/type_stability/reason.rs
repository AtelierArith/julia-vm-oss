//! Type stability reason enumeration.
//!
//! This module defines the reasons why a function might be type-unstable.

use crate::compile::lattice::types::ConcreteType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Reasons why a function might be type-unstable.
///
/// Type stability means the return type of a function can be uniquely determined
/// from the types of its input arguments. When this is not possible, one of these
/// reasons explains why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TypeStabilityReason {
    /// The function returns `Any` (Top type), meaning the return type cannot be determined.
    ReturnsTop,

    /// The function returns a Union type, meaning multiple concrete return types are possible.
    ReturnsUnion {
        /// The set of concrete types that make up the union.
        types: BTreeSet<ConcreteType>,
    },

    /// The function has a recursive call cycle that prevents type determination.
    RecursiveCycle {
        /// The function names involved in the cycle.
        cycle: Vec<String>,
    },

    /// The function calls an unknown function whose return type cannot be determined.
    UnknownCallReturn {
        /// The name of the unknown function.
        function_name: String,
    },

    /// A conditional branch returns different types in then vs else branches.
    ConditionalBranchMismatch {
        /// Type returned from the then branch.
        then_type: String,
        /// Type returned from the else branch.
        else_type: String,
    },

    /// The function has untyped parameters, preventing precise inference.
    UntypedParameters {
        /// Names of parameters without type annotations.
        param_names: Vec<String>,
    },

    /// The function uses dynamic dispatch that prevents static type inference.
    DynamicDispatch {
        /// Description of the dynamic dispatch site.
        description: String,
    },

    /// Parameter types were inferred from usage patterns (informational).
    ///
    /// This is not necessarily a problem - it indicates that the analyzer
    /// used usage-based inference to determine parameter types.
    InferredParameterTypes {
        /// Map of parameter names to their inferred types.
        inferred: Vec<(String, String)>,
    },
}

impl TypeStabilityReason {
    /// Returns a human-readable description of the reason.
    pub fn description(&self) -> String {
        match self {
            TypeStabilityReason::ReturnsTop => {
                "return type is Any (cannot be determined)".to_string()
            }
            TypeStabilityReason::ReturnsUnion { types } => {
                let type_names: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
                format!("return type is Union{{{}}}", type_names.join(", "))
            }
            TypeStabilityReason::RecursiveCycle { cycle } => {
                format!("recursive cycle detected: {}", cycle.join(" -> "))
            }
            TypeStabilityReason::UnknownCallReturn { function_name } => {
                format!("unknown function call: {}", function_name)
            }
            TypeStabilityReason::ConditionalBranchMismatch {
                then_type,
                else_type,
            } => {
                format!(
                    "conditional branches return different types: {} vs {}",
                    then_type, else_type
                )
            }
            TypeStabilityReason::UntypedParameters { param_names } => {
                format!("untyped parameters: {}", param_names.join(", "))
            }
            TypeStabilityReason::DynamicDispatch { description } => {
                format!("dynamic dispatch: {}", description)
            }
            TypeStabilityReason::InferredParameterTypes { inferred } => {
                let param_strs: Vec<String> = inferred
                    .iter()
                    .map(|(name, ty)| format!("{}::{}", name, ty))
                    .collect();
                format!(
                    "parameter types inferred from usage: {}",
                    param_strs.join(", ")
                )
            }
        }
    }

    /// Returns a suggestion for fixing the type instability.
    pub fn suggestion(&self) -> String {
        match self {
            TypeStabilityReason::ReturnsTop => {
                "Add type annotations to parameters and ensure all return paths have consistent types".to_string()
            }
            TypeStabilityReason::ReturnsUnion { .. } => {
                "Use type conversion to ensure consistent return type across all branches".to_string()
            }
            TypeStabilityReason::RecursiveCycle { .. } => {
                "Add return type annotation to break the inference cycle".to_string()
            }
            TypeStabilityReason::UnknownCallReturn { function_name } => {
                format!("Ensure function '{}' is defined or add type annotation to the result", function_name)
            }
            TypeStabilityReason::ConditionalBranchMismatch { .. } => {
                "Convert values to a common type in both branches".to_string()
            }
            TypeStabilityReason::UntypedParameters { .. } => {
                "Add type annotations to function parameters".to_string()
            }
            TypeStabilityReason::DynamicDispatch { .. } => {
                "Use concrete types instead of abstract types to enable static dispatch".to_string()
            }
            TypeStabilityReason::InferredParameterTypes { .. } => {
                "Consider adding explicit type annotations matching the inferred types".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_returns_top_description() {
        let reason = TypeStabilityReason::ReturnsTop;
        assert!(reason.description().contains("Any"));
    }

    #[test]
    fn test_returns_union_description() {
        let mut types = BTreeSet::new();
        types.insert(ConcreteType::Int64);
        types.insert(ConcreteType::Float64);
        let reason = TypeStabilityReason::ReturnsUnion { types };
        assert!(reason.description().contains("Union"));
    }

    #[test]
    fn test_recursive_cycle_description() {
        let reason = TypeStabilityReason::RecursiveCycle {
            cycle: vec!["f".to_string(), "g".to_string(), "f".to_string()],
        };
        assert!(reason.description().contains("recursive cycle"));
    }
}
