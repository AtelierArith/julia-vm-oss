//! Type expression support for parametric types.
//!
//! `TypeExpr` represents type expressions that can reference type parameters,
//! used in parametric struct field definitions.

use serde::{Deserialize, Serialize};

use super::julia_type::JuliaType;
use super::type_param::TypeParam;

/// A type expression that can reference type parameters.
///
/// Used in parametric struct field definitions where the type may be:
/// - A concrete type like `Int64`
/// - A type variable reference like `T`
/// - A parameterized type like `Point{Float64}`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeExpr {
    /// Concrete type: Int64, Float64, etc.
    Concrete(JuliaType),
    /// Type variable reference: T, S, etc.
    TypeVar(String),
    /// Parameterized type: Point{Float64}, Pair{Int64, String}
    Parameterized { base: String, params: Vec<TypeExpr> },
    /// Runtime expression that evaluates to a type/value (e.g., Symbol(s) in MIME{Symbol(s)})
    /// The expression is stored as the source text and needs to be evaluated at runtime
    RuntimeExpr(String),
}

impl TypeExpr {
    /// Create a TypeExpr from a type name string.
    ///
    /// If the name matches a known JuliaType, returns Concrete.
    /// Otherwise, returns TypeVar (assuming it's a type parameter reference).
    pub fn from_name(name: &str, type_params: &[TypeParam]) -> Self {
        // First check if it's a type parameter reference
        if type_params.iter().any(|p| p.name == name) {
            return TypeExpr::TypeVar(name.to_string());
        }
        // Otherwise, try to parse as a concrete type
        match JuliaType::from_name(name) {
            Some(jt) => TypeExpr::Concrete(jt),
            None => TypeExpr::TypeVar(name.to_string()), // Unknown type treated as type var
        }
    }

    /// Check if this type expression is a type variable reference.
    pub fn is_type_var(&self) -> bool {
        matches!(self, TypeExpr::TypeVar(_))
    }

    /// Check if this type expression is concrete (no type variables).
    pub fn is_concrete(&self) -> bool {
        match self {
            TypeExpr::Concrete(_) => true,
            TypeExpr::TypeVar(_) => false,
            TypeExpr::Parameterized { params, .. } => params.iter().all(|p| p.is_concrete()),
            TypeExpr::RuntimeExpr(_) => false, // Runtime expressions are not concrete
        }
    }

    /// Check if this is a runtime expression that needs evaluation
    pub fn is_runtime_expr(&self) -> bool {
        matches!(self, TypeExpr::RuntimeExpr(_))
    }
}

impl std::fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeExpr::Concrete(jt) => write!(f, "{}", jt),
            TypeExpr::TypeVar(name) => write!(f, "{}", name),
            TypeExpr::Parameterized { base, params } => {
                let params_str: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "{}{{{}}}", base, params_str.join(", "))
            }
            TypeExpr::RuntimeExpr(expr) => write!(f, "{}", expr),
        }
    }
}
