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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
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
    /// Legacy mirror of `upper_bound` (deprecated: use `upper_bound`).
    ///
    /// This is a LIVE field, not dead state: it is read by the
    /// parametric-binding dispatch match (`types/julia_type/comparison.rs`,
    /// enforcing `where T<:Bound`) and by parametric-struct bound checking
    /// (`compile/context.rs`). Every production constructor keeps
    /// `bound == upper_bound`.
    ///
    /// The field is `#[serde(skip)]` to keep the wire format independent of
    /// non-self-describing formats (bincode), but it is **reconstructed from
    /// `upper_bound` on deserialization** by the hand-written `Deserialize`
    /// impl below. A plain `#[serde(skip)]` derive would default it to `None`,
    /// silently dropping the bound constraint and making the prelude-program /
    /// method-table caches non-transparent (the `bound: None` vs
    /// `Some("Real")` divergence for `Complex{T} where T<:Real`, Issue #6518).
    #[serde(skip)]
    pub bound: Option<String>,
}

impl<'de> Deserialize<'de> for TypeParam {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Wire shape mirrors the serialized fields (`bound` is `#[serde(skip)]`,
        // so it never appears on the wire). On the way back in, `bound` is
        // reconstructed from `upper_bound` so the value equals what every
        // production constructor (`with_upper_bound`, `with_both_bounds`) and
        // the `core_signature` reconstruction (`core_type_var_to_type_param`)
        // produce — making the prelude-program and method-table caches
        // round-trip exactly (Issue #6518).
        #[derive(Deserialize)]
        struct TypeParamWire {
            name: String,
            #[serde(default)]
            upper_bound: Option<String>,
            #[serde(default)]
            lower_bound: Option<String>,
        }

        let wire = TypeParamWire::deserialize(deserializer)?;
        Ok(Self {
            name: wire.name,
            bound: wire.upper_bound.clone(),
            upper_bound: wire.upper_bound,
            lower_bound: wire.lower_bound,
        })
    }
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
