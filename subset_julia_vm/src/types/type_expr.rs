//! Type expression support for parametric types.
//!
//! `TypeExpr` represents type expressions that can reference type parameters,
//! used in parametric struct field definitions.

use serde::{Deserialize, Serialize};

use super::julia_type::JuliaType;
use super::type_param::TypeParam;
use std::collections::HashMap;

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

    /// Return the simple type-name surface for type expressions that can be
    /// represented without nested parameters or runtime evaluation.
    pub fn as_simple_type_name(&self) -> Option<String> {
        match self {
            TypeExpr::Concrete(jt) => Some(jt.to_string()),
            TypeExpr::TypeVar(name) => Some(name.clone()),
            TypeExpr::Parameterized { .. } | TypeExpr::RuntimeExpr(_) => None,
        }
    }

    /// Render a comma-separated parameter list using the canonical `TypeExpr`
    /// display surface.
    pub fn render_param_list(params: &[TypeExpr]) -> String {
        let mut rendered = String::new();
        for (idx, param) in params.iter().enumerate() {
            if idx > 0 {
                rendered.push_str(", ");
            }
            rendered.push_str(&param.to_string());
        }
        rendered
    }

    /// Render `Base{P1, P2}` using the canonical `TypeExpr` parameter display.
    pub fn format_parameterized(base: &str, params: &[TypeExpr]) -> String {
        format!("{}{{{}}}", base, Self::render_param_list(params))
    }

    /// Project this type expression to a `JuliaType`.
    ///
    /// Issue #6720: routed through the structured `TypeExpr → CoreType →
    /// JuliaType` hub instead of the former `from_name_or_struct(&self.to_string())`
    /// string render + reparse (`TYPE_REPRESENTATIONS.md` §3.3.1 / conversion
    /// #34). Parametric applications keep their parameter structure inside
    /// `CoreType` rather than collapsing to an opaque `Struct(String)` mid-flight;
    /// the projected `JuliaType` is byte-identical to the old round-trip for
    /// lowering-produced shapes (pinned by
    /// `tests::to_julia_type_lossy_matches_string_round_trip_issue_6720`).
    /// Unresolved names still become `JuliaType::Struct` placeholders.
    pub fn to_julia_type_lossy(&self) -> JuliaType {
        match self {
            // Parametric applications (and already-parsed `Concrete`) flow
            // through the structured hub so a `Parameterized` keeps its
            // parameter structure inside `CoreType` instead of collapsing to an
            // opaque `Struct(String)` mid-flight (the #34 wart).
            TypeExpr::Concrete(_) | TypeExpr::Parameterized { .. } => {
                crate::inference_core::core_type_to_julia_type(
                    &crate::inference_core::CoreType::from(self),
                )
            }
            // A top-level leaf name carries no parametric structure to preserve.
            // Keep the single-name parse verbatim so an unresolved type-var
            // reference (`T`) stays the `Struct("T")` placeholder the rest of the
            // pipeline expects, rather than being reinterpreted as a `TypeVar`.
            TypeExpr::TypeVar(_) | TypeExpr::RuntimeExpr(_) => {
                JuliaType::from_name_or_struct(&self.to_string())
            }
        }
    }

    /// Substitute type variables with concrete `JuliaType` arguments and then
    /// project the result to `JuliaType`. Unbound variables and runtime
    /// expressions widen to `Any`, matching existing reflection behavior.
    ///
    /// Issue #6720 (follow-up): the `Parameterized` arm is routed through the
    /// structured `TypeExpr → CoreType → JuliaType` hub
    /// (`substitute_to_core` + `core_type_to_julia_type`) instead of rendering
    /// the substituted param names and re-parsing the joined `Base{...}` string.
    /// The substituted parameters keep their `CoreType` structure mid-flight;
    /// the projection is byte-identical to the old round-trip for the
    /// substitution scenarios pinned by
    /// `tests::substitute_to_julia_type_lossy_matches_string_round_trip_issue_6720`.
    /// The leaf arms keep their direct behavior (a bound var clones its concrete
    /// argument; unbound vars / runtime exprs widen to `Any`).
    pub fn substitute_to_julia_type_lossy(&self, subst: &HashMap<&str, &JuliaType>) -> JuliaType {
        match self {
            TypeExpr::Concrete(jt) => jt.clone(),
            TypeExpr::TypeVar(name) => subst
                .get(name.as_str())
                .map_or(JuliaType::Any, |ty| (*ty).clone()),
            TypeExpr::Parameterized { .. } => {
                crate::inference_core::core_type_to_julia_type(&self.substitute_to_core(subst))
            }
            TypeExpr::RuntimeExpr(_) => JuliaType::Any,
        }
    }

    /// Substitute type variables with their concrete `JuliaType` arguments and
    /// project into the structured `CoreType` hub (Issue #6720). The structural
    /// companion of [`Self::substitute_to_julia_type_lossy`]: a bound type var
    /// becomes `CoreType::from(arg)`, an unbound var / runtime expr widens to
    /// `CoreType::Any`, and a `Parameterized` application keeps its parameter
    /// structure (`CoreType::Tuple` / canonical `CoreType::Union` /
    /// `CoreType::Struct { name, params }`) rather than collapsing to a string.
    fn substitute_to_core(
        &self,
        subst: &HashMap<&str, &JuliaType>,
    ) -> crate::inference_core::CoreType {
        use crate::inference_core::CoreType;
        match self {
            TypeExpr::Concrete(jt) => CoreType::from(jt),
            TypeExpr::TypeVar(name) => subst
                .get(name.as_str())
                .map_or(CoreType::Any, |ty| CoreType::from(*ty)),
            TypeExpr::RuntimeExpr(_) => CoreType::Any,
            TypeExpr::Parameterized { base, params } => {
                let core_params: Vec<CoreType> =
                    params.iter().map(|p| p.substitute_to_core(subst)).collect();
                match base.as_str() {
                    "Tuple" => CoreType::Tuple(core_params),
                    "Union" => {
                        let members: Vec<JuliaType> = params
                            .iter()
                            .map(|p| p.substitute_to_julia_type_lossy(subst))
                            .collect();
                        CoreType::from(&crate::types::canonicalize_union(members))
                    }
                    _ => CoreType::Struct {
                        name: base.clone(),
                        params: core_params,
                    },
                }
            }
        }
    }
}

impl std::fmt::Display for TypeExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeExpr::Concrete(jt) => write!(f, "{}", jt),
            TypeExpr::TypeVar(name) => write!(f, "{}", name),
            TypeExpr::Parameterized { base, params } => {
                write!(f, "{}", TypeExpr::format_parameterized(base, params))
            }
            TypeExpr::RuntimeExpr(expr) => write!(f, "{}", expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #6720: `to_julia_type_lossy` is rerouted through the structured
    /// `TypeExpr → CoreType → JuliaType` hub (no `to_string()` +
    /// `from_name_or_struct` round-trip). This pins that the new path is
    /// byte-identical to the old string round-trip across the realistic
    /// lowering-produced `TypeExpr` shapes (struct field types).
    #[test]
    fn to_julia_type_lossy_matches_string_round_trip_issue_6720() {
        let cases = vec![
            // Concrete primitives / abstracts
            TypeExpr::Concrete(JuliaType::Int64),
            TypeExpr::Concrete(JuliaType::Float64),
            TypeExpr::Concrete(JuliaType::Bool),
            TypeExpr::Concrete(JuliaType::String),
            TypeExpr::Concrete(JuliaType::Char),
            TypeExpr::Concrete(JuliaType::Symbol),
            TypeExpr::Concrete(JuliaType::Nothing),
            TypeExpr::Concrete(JuliaType::Missing),
            TypeExpr::Concrete(JuliaType::Number),
            TypeExpr::Concrete(JuliaType::Real),
            TypeExpr::Concrete(JuliaType::Integer),
            TypeExpr::Concrete(JuliaType::Any),
            // Concrete containers
            TypeExpr::Concrete(JuliaType::VectorOf(Box::new(JuliaType::Int64))),
            TypeExpr::Concrete(JuliaType::MatrixOf(Box::new(JuliaType::Float64))),
            TypeExpr::Concrete(JuliaType::TupleOf(vec![
                JuliaType::Int64,
                JuliaType::String,
            ])),
            TypeExpr::Concrete(JuliaType::Array),
            TypeExpr::Concrete(JuliaType::Dict),
            TypeExpr::Concrete(JuliaType::Set),
            // Concrete struct spellings
            TypeExpr::Concrete(JuliaType::Struct("Foo".to_string())),
            TypeExpr::Concrete(JuliaType::Struct("Pair{Int64, String}".to_string())),
            TypeExpr::Concrete(JuliaType::Struct("Complex{Float64}".to_string())),
            // Concrete union
            TypeExpr::Concrete(JuliaType::Union(vec![JuliaType::Nothing, JuliaType::Int64])),
            // Type-var leaves
            TypeExpr::TypeVar("T".to_string()),
            TypeExpr::TypeVar("S".to_string()),
            // Parameterized (the round-trip wart #34): user + known containers
            TypeExpr::Parameterized {
                base: "Pair".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::Concrete(JuliaType::String),
                ],
            },
            TypeExpr::Parameterized {
                base: "Vector".to_string(),
                params: vec![TypeExpr::Concrete(JuliaType::Int64)],
            },
            TypeExpr::Parameterized {
                base: "Matrix".to_string(),
                params: vec![TypeExpr::Concrete(JuliaType::Float64)],
            },
            TypeExpr::Parameterized {
                base: "Tuple".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::Concrete(JuliaType::String),
                ],
            },
            TypeExpr::Parameterized {
                base: "Dict".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::Concrete(JuliaType::String),
                ],
            },
            TypeExpr::Parameterized {
                base: "Set".to_string(),
                params: vec![TypeExpr::Concrete(JuliaType::Int64)],
            },
            TypeExpr::Parameterized {
                base: "MyBox".to_string(),
                params: vec![TypeExpr::Concrete(JuliaType::Int64)],
            },
            // Nested parametric
            TypeExpr::Parameterized {
                base: "Vector".to_string(),
                params: vec![TypeExpr::Parameterized {
                    base: "Pair".to_string(),
                    params: vec![
                        TypeExpr::Concrete(JuliaType::Int64),
                        TypeExpr::Concrete(JuliaType::Float64),
                    ],
                }],
            },
            // Parameterized union
            TypeExpr::Parameterized {
                base: "Union".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Nothing),
                    TypeExpr::Concrete(JuliaType::Int64),
                ],
            },
        ];
        for te in &cases {
            let expected = JuliaType::from_name_or_struct(&te.to_string());
            assert_eq!(
                te.to_julia_type_lossy(),
                expected,
                "to_julia_type_lossy diverged from string round-trip for {te:?}"
            );
        }
    }

    /// Issue #6720 (follow-up): `substitute_to_julia_type_lossy` is rerouted
    /// through the structured `TypeExpr → CoreType → JuliaType` hub for the
    /// `Parameterized` arm (no `name()` render + `from_name_or_struct` reparse).
    /// This pins that the new path is byte-identical to the old string
    /// round-trip across substitution scenarios.
    #[test]
    fn substitute_to_julia_type_lossy_matches_string_round_trip_issue_6720() {
        use std::collections::HashMap;
        let int = JuliaType::Int64;
        let flt = JuliaType::Float64;
        let vec_int = JuliaType::VectorOf(Box::new(JuliaType::Int64));
        let foo = JuliaType::Struct("Foo".to_string());
        let mut subst: HashMap<&str, &JuliaType> = HashMap::new();
        subst.insert("T", &int);
        subst.insert("S", &flt);
        subst.insert("V", &vec_int);
        subst.insert("U", &foo);

        // Reference implementation of the *old* render + reparse algorithm.
        fn old_subst(te: &TypeExpr, subst: &HashMap<&str, &JuliaType>) -> JuliaType {
            match te {
                TypeExpr::Concrete(jt) => jt.clone(),
                TypeExpr::TypeVar(name) => subst
                    .get(name.as_str())
                    .map_or(JuliaType::Any, |ty| (*ty).clone()),
                TypeExpr::Parameterized { base, params } => {
                    let rendered = params
                        .iter()
                        .map(|p| old_subst(p, subst).name().to_string())
                        .collect::<Vec<_>>();
                    JuliaType::from_name_or_struct(&format!("{}{{{}}}", base, rendered.join(", ")))
                }
                TypeExpr::RuntimeExpr(_) => JuliaType::Any,
            }
        }

        let tv = |n: &str| TypeExpr::TypeVar(n.to_string());
        let cases = vec![
            TypeExpr::Concrete(JuliaType::Int64),
            tv("T"), // bound -> Int64
            tv("X"), // unbound -> Any
            TypeExpr::RuntimeExpr("Symbol(s)".to_string()),
            TypeExpr::Parameterized {
                base: "Pair".to_string(),
                params: vec![tv("T"), tv("S")],
            },
            TypeExpr::Parameterized {
                base: "Vector".to_string(),
                params: vec![tv("T")],
            },
            TypeExpr::Parameterized {
                base: "Tuple".to_string(),
                params: vec![tv("T"), TypeExpr::Concrete(JuliaType::String)],
            },
            TypeExpr::Parameterized {
                base: "MyBox".to_string(),
                params: vec![tv("V")], // -> MyBox{Vector{Int64}}
            },
            TypeExpr::Parameterized {
                base: "Holder".to_string(),
                params: vec![tv("U"), tv("X")], // user struct + unbound -> Any
            },
            TypeExpr::Parameterized {
                base: "Array".to_string(),
                params: vec![TypeExpr::Parameterized {
                    base: "Pair".to_string(),
                    params: vec![tv("T"), tv("S")],
                }],
            },
            TypeExpr::Parameterized {
                base: "Union".to_string(),
                params: vec![tv("T"), TypeExpr::Concrete(JuliaType::Nothing)],
            },
        ];
        for te in &cases {
            assert_eq!(
                te.substitute_to_julia_type_lossy(&subst),
                old_subst(te, &subst),
                "substitute_to_julia_type_lossy diverged from string round-trip for {te:?}"
            );
        }
    }

    #[test]
    fn simple_type_name_accepts_concrete_types_and_typevars_issue_5916() {
        assert_eq!(
            TypeExpr::Concrete(JuliaType::Int64).as_simple_type_name(),
            Some("Int64".to_string())
        );
        assert_eq!(
            TypeExpr::Concrete(JuliaType::Float64).as_simple_type_name(),
            Some("Float64".to_string())
        );
        assert_eq!(
            TypeExpr::TypeVar("T".to_string()).as_simple_type_name(),
            Some("T".to_string())
        );
    }

    #[test]
    fn simple_type_name_rejects_nested_and_runtime_exprs_issue_5916() {
        let nested = TypeExpr::Parameterized {
            base: "Vector".to_string(),
            params: vec![TypeExpr::Concrete(JuliaType::Int64)],
        };
        assert_eq!(nested.as_simple_type_name(), None);
        assert_eq!(
            TypeExpr::RuntimeExpr("some_expr".to_string()).as_simple_type_name(),
            None
        );
    }

    #[test]
    fn display_renders_nested_type_expr_without_compile_helper_issue_5916() {
        let nested = TypeExpr::Parameterized {
            base: "Array".to_string(),
            params: vec![TypeExpr::Parameterized {
                base: "Pair".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::TypeVar("T".to_string()),
                ],
            }],
        };
        assert_eq!(nested.to_string(), "Array{Pair{Int64, T}}");
    }

    #[test]
    fn parameterized_render_helpers_share_type_expr_display_issue_5916() {
        let params = vec![
            TypeExpr::Concrete(JuliaType::Int64),
            TypeExpr::Parameterized {
                base: "Vector".to_string(),
                params: vec![TypeExpr::Concrete(JuliaType::String)],
            },
        ];
        assert_eq!(
            TypeExpr::render_param_list(&params),
            "Int64, Vector{String}"
        );
        assert_eq!(
            TypeExpr::format_parameterized("Tuple", &params),
            "Tuple{Int64, Vector{String}}"
        );
    }
}
