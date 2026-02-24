//! Function stability report.
//!
//! This module defines the stability report for a single function.

use crate::compile::lattice::types::LatticeType;
use crate::compile::type_stability::reason::TypeStabilityReason;
use serde::{Deserialize, Serialize};

/// The stability status of a function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StabilityStatus {
    /// The function is type-stable.
    /// Its return type is Concrete or Const.
    Stable,

    /// The function is type-unstable.
    /// Its return type is Top, Union, or cannot be determined.
    Unstable,

    /// The stability could not be determined.
    /// This may happen with complex recursive patterns.
    Unknown,
}

impl std::fmt::Display for StabilityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StabilityStatus::Stable => write!(f, "Stable"),
            StabilityStatus::Unstable => write!(f, "Unstable"),
            StabilityStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Report for a single function's type stability analysis.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionStabilityReport {
    /// The name of the function.
    pub function_name: String,

    /// The source location (line number) where the function is defined.
    pub line: usize,

    /// Input parameter types as (name, type) pairs.
    pub input_signature: Vec<(String, LatticeType)>,

    /// The inferred return type.
    pub return_type: LatticeType,

    /// The stability status.
    pub status: StabilityStatus,

    /// Reasons for type instability (empty if stable).
    pub reasons: Vec<TypeStabilityReason>,

    /// Suggestions for improving type stability.
    pub suggestions: Vec<String>,
}

impl FunctionStabilityReport {
    /// Creates a new function stability report.
    pub fn new(
        function_name: String,
        line: usize,
        input_signature: Vec<(String, LatticeType)>,
        return_type: LatticeType,
    ) -> Self {
        let status = Self::determine_status(&return_type);
        Self {
            function_name,
            line,
            input_signature,
            return_type,
            status,
            reasons: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Determines the stability status from the return type.
    fn determine_status(return_type: &LatticeType) -> StabilityStatus {
        match return_type {
            LatticeType::Bottom => StabilityStatus::Unknown,
            LatticeType::Const(_) => StabilityStatus::Stable,
            LatticeType::Concrete(_) => StabilityStatus::Stable,
            LatticeType::Union(_) => StabilityStatus::Unstable,
            LatticeType::Conditional { .. } => StabilityStatus::Unstable,
            LatticeType::Top => StabilityStatus::Unstable,
        }
    }

    /// Adds a reason for type instability.
    pub fn add_reason(&mut self, reason: TypeStabilityReason) {
        // Also add the suggestion from the reason
        self.suggestions.push(reason.suggestion());
        self.reasons.push(reason);
    }

    /// Returns true if the function is type-stable.
    pub fn is_stable(&self) -> bool {
        self.status == StabilityStatus::Stable
    }

    /// Returns true if the function is type-unstable.
    pub fn is_unstable(&self) -> bool {
        self.status == StabilityStatus::Unstable
    }

    /// Returns a formatted signature string.
    pub fn format_signature(&self) -> String {
        let params: Vec<String> = self
            .input_signature
            .iter()
            .map(|(name, ty)| format!("{}::{}", name, Self::format_lattice_type(ty)))
            .collect();
        format!("{}({})", self.function_name, params.join(", "))
    }

    /// Formats a LatticeType for display.
    fn format_lattice_type(ty: &LatticeType) -> String {
        match ty {
            LatticeType::Bottom => "Bottom".to_string(),
            LatticeType::Const(cv) => format!("Const({:?})", cv),
            LatticeType::Concrete(ct) => format!("{:?}", ct),
            LatticeType::Union(types) => {
                let type_strs: Vec<String> = types.iter().map(|t| format!("{:?}", t)).collect();
                format!("Union{{{}}}", type_strs.join(", "))
            }
            LatticeType::Conditional { .. } => "Conditional".to_string(),
            LatticeType::Top => "Any".to_string(),
        }
    }

    /// Formats the return type for display.
    pub fn format_return_type(&self) -> String {
        Self::format_lattice_type(&self.return_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::lattice::types::ConcreteType;

    #[test]
    fn test_stable_function() {
        let report = FunctionStabilityReport::new(
            "add".to_string(),
            1,
            vec![
                ("x".to_string(), LatticeType::Concrete(ConcreteType::Int64)),
                ("y".to_string(), LatticeType::Concrete(ConcreteType::Int64)),
            ],
            LatticeType::Concrete(ConcreteType::Int64),
        );
        assert!(report.is_stable());
        assert!(!report.is_unstable());
    }

    #[test]
    fn test_unstable_union_function() {
        let mut types = std::collections::BTreeSet::new();
        types.insert(ConcreteType::Int64);
        types.insert(ConcreteType::Float64);

        let report = FunctionStabilityReport::new(
            "compute".to_string(),
            10,
            vec![(
                "x".to_string(),
                LatticeType::Concrete(ConcreteType::Float64),
            )],
            LatticeType::Union(types),
        );
        assert!(report.is_unstable());
        assert!(!report.is_stable());
    }

    #[test]
    fn test_unstable_top_function() {
        let report =
            FunctionStabilityReport::new("dynamic_func".to_string(), 20, vec![], LatticeType::Top);
        assert!(report.is_unstable());
    }

    #[test]
    fn test_format_signature() {
        let report = FunctionStabilityReport::new(
            "add".to_string(),
            1,
            vec![
                ("x".to_string(), LatticeType::Concrete(ConcreteType::Int64)),
                (
                    "y".to_string(),
                    LatticeType::Concrete(ConcreteType::Float64),
                ),
            ],
            LatticeType::Concrete(ConcreteType::Float64),
        );
        let sig = report.format_signature();
        assert!(sig.contains("add"));
        assert!(sig.contains("x::Int64"));
        assert!(sig.contains("y::Float64"));
    }
}
