//! Type parameter declarations with optional bounds.
//!
//! `TypeParam` represents type parameter declarations like `T`, `T<:Number`,
//! `T>:Integer`, or `Integer<:T<:Real`.

use serde::{Deserialize, Serialize};

/// A type parameter declaration with optional upper and lower bounds.
///
/// Represents declarations like:
/// - `T` - unbounded type parameter
/// - `T<:Number` - type parameter with upper bound (covariant)
/// - `T>:Integer` - type parameter with lower bound (contravariant)
/// - `Integer<:T<:Real` - type parameter with both bounds
///
/// Bounds are stored as strings to support user-defined abstract types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TypeParam {
    /// The name of the type parameter (e.g., "T", "S")
    pub name: String,
    /// Optional upper bound (covariant constraint): T <: upper_bound
    /// Example: `T<:Number` means T must be a subtype of Number
    #[serde(default)]
    pub upper_bound: Option<String>,
    /// Optional lower bound (contravariant constraint): lower_bound <: T
    /// Example: `T>:Integer` means Integer must be a subtype of T
    #[serde(default)]
    pub lower_bound: Option<String>,
    /// Legacy field for backward compatibility (maps to upper_bound)
    /// Deprecated: Use upper_bound instead
    /// Note: skipped in all serialization formats (including bincode) to avoid
    /// non-self-describing format incompatibility. Always reconstructed from upper_bound.
    #[serde(skip)]
    pub bound: Option<String>,
}

impl TypeParam {
    /// Create a new unbounded type parameter.
    pub fn new(name: String) -> Self {
        Self {
            name,
            upper_bound: None,
            lower_bound: None,
            bound: None,
        }
    }

    /// Create a new type parameter with an upper bound.
    pub fn with_bound(name: String, bound: String) -> Self {
        Self {
            name,
            upper_bound: Some(bound.clone()),
            lower_bound: None,
            bound: Some(bound),
        }
    }

    /// Create a new type parameter with an upper bound (explicit).
    pub fn with_upper_bound(name: String, upper: String) -> Self {
        Self {
            name,
            upper_bound: Some(upper.clone()),
            lower_bound: None,
            bound: Some(upper),
        }
    }

    /// Create a new type parameter with a lower bound.
    pub fn with_lower_bound(name: String, lower: String) -> Self {
        Self {
            name,
            upper_bound: None,
            lower_bound: Some(lower),
            bound: None,
        }
    }

    /// Create a new type parameter with both upper and lower bounds.
    pub fn with_both_bounds(name: String, lower: String, upper: String) -> Self {
        Self {
            name,
            upper_bound: Some(upper.clone()),
            lower_bound: Some(lower),
            bound: Some(upper),
        }
    }

    /// Get the effective upper bound (checks both new and legacy fields).
    pub fn get_upper_bound(&self) -> Option<&String> {
        self.upper_bound.as_ref().or(self.bound.as_ref())
    }

    /// Check if this type parameter has any constraints.
    pub fn has_constraints(&self) -> bool {
        self.upper_bound.is_some() || self.lower_bound.is_some() || self.bound.is_some()
    }
}

impl std::fmt::Display for TypeParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.lower_bound, self.get_upper_bound()) {
            (Some(lower), Some(upper)) => write!(f, "{}<:{}<:{}", lower, self.name, upper),
            (None, Some(upper)) => write!(f, "{}<:{}", self.name, upper),
            (Some(lower), None) => write!(f, "{}>:{}", self.name, lower),
            (None, None) => write!(f, "{}", self.name),
        }
    }
}
