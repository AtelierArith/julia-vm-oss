use super::{CoreAbstract, CorePrimitive, CoreType, CoreTypeVar};

impl From<&crate::types::JuliaType> for CoreType {
    fn from(ty: &crate::types::JuliaType) -> Self {
        use crate::types::JuliaType as JT;
        match ty {
            JT::Int8 => Self::Primitive(CorePrimitive::Int8),
            JT::Int16 => Self::Primitive(CorePrimitive::Int16),
            JT::Int32 => Self::Primitive(CorePrimitive::Int32),
            JT::Int64 => Self::Primitive(CorePrimitive::Int64),
            JT::Int128 => Self::Primitive(CorePrimitive::Int128),
            JT::BigInt => Self::Primitive(CorePrimitive::BigInt),
            JT::UInt8 => Self::Primitive(CorePrimitive::UInt8),
            JT::UInt16 => Self::Primitive(CorePrimitive::UInt16),
            JT::UInt32 => Self::Primitive(CorePrimitive::UInt32),
            JT::UInt64 => Self::Primitive(CorePrimitive::UInt64),
            JT::UInt128 => Self::Primitive(CorePrimitive::UInt128),
            JT::Bool => Self::Primitive(CorePrimitive::Bool),
            JT::Float16 => Self::Primitive(CorePrimitive::Float16),
            JT::Float32 => Self::Primitive(CorePrimitive::Float32),
            JT::Float64 => Self::Primitive(CorePrimitive::Float64),
            JT::BigFloat => Self::Primitive(CorePrimitive::BigFloat),
            JT::String => Self::Primitive(CorePrimitive::String),
            JT::Char => Self::Primitive(CorePrimitive::Char),
            JT::Symbol => Self::Primitive(CorePrimitive::Symbol),
            JT::Nothing => Self::Primitive(CorePrimitive::Nothing),
            JT::Missing => Self::Primitive(CorePrimitive::Missing),
            JT::Any => Self::Any,
            JT::Bottom => Self::Bottom,
            JT::Number => Self::Abstract(CoreAbstract::Number),
            JT::Real => Self::Abstract(CoreAbstract::Real),
            JT::Integer => Self::Abstract(CoreAbstract::Integer),
            JT::Signed => Self::Abstract(CoreAbstract::Signed),
            JT::Unsigned => Self::Abstract(CoreAbstract::Unsigned),
            JT::AbstractFloat => Self::Abstract(CoreAbstract::AbstractFloat),
            JT::AbstractString => Self::Abstract(CoreAbstract::AbstractString),
            JT::AbstractChar => Self::Abstract(CoreAbstract::AbstractChar),
            JT::AbstractArray => Self::Abstract(CoreAbstract::AbstractArray),
            JT::AbstractRange => Self::Abstract(CoreAbstract::AbstractRange),
            JT::Function => Self::Abstract(CoreAbstract::Function),
            JT::IO => Self::Abstract(CoreAbstract::IO),
            JT::Type => Self::Abstract(CoreAbstract::Type),
            JT::DataType => Self::Abstract(CoreAbstract::DataType),
            JT::Array => Self::Struct {
                name: "Array".to_string(),
                params: vec![],
            },
            JT::VectorOf(elem) => Self::Struct {
                name: "Vector".to_string(),
                params: vec![Self::from(elem.as_ref())],
            },
            JT::MatrixOf(elem) => Self::Struct {
                name: "Matrix".to_string(),
                params: vec![Self::from(elem.as_ref())],
            },
            JT::Tuple => Self::Struct {
                name: "Tuple".to_string(),
                params: vec![],
            },
            JT::TupleOf(elements) => {
                Self::Tuple(elements.iter().map(Self::from).collect::<Vec<_>>())
            }
            JT::NamedTuple => Self::Struct {
                name: "NamedTuple".to_string(),
                params: vec![],
            },
            JT::Dict => Self::Struct {
                name: "Dict".to_string(),
                params: vec![],
            },
            JT::Set => Self::Struct {
                name: "Set".to_string(),
                params: vec![],
            },
            JT::UnitRange => Self::Struct {
                name: "UnitRange".to_string(),
                params: vec![],
            },
            JT::StepRange => Self::Struct {
                name: "StepRange".to_string(),
                params: vec![],
            },
            JT::Struct(name) => Self::from_julia_name(name),
            JT::Module => Self::Module("Module".to_string()),
            JT::Expr | JT::QuoteNode | JT::LineNumberNode | JT::GlobalRef => {
                Self::from_julia_name(&ty.to_string())
            }
            JT::Enum(_) => Self::Named(ty.to_string()),
            JT::Pairs | JT::Generator | JT::IOBuffer => Self::from_julia_name(&ty.to_string()),
            JT::AbstractUser(name, parent) => Self::AbstractUser {
                name: name.clone(),
                parent: parent
                    .as_ref()
                    .map(|p| Box::new(Self::from_julia_name(p.as_str()))),
            },
            JT::TypeVar(name, upper_bound) => Self::TypeVar(CoreTypeVar {
                name: name.clone(),
                lower_bound: None,
                upper_bound: upper_bound
                    .as_ref()
                    .and_then(|b| core_upper_bound_from_name(b)),
            }),
            JT::Union(types) => Self::Union(types.iter().map(Self::from).collect()),
            JT::TypeOf(inner) => Self::TypeOf(Box::new(Self::from(inner.as_ref()))),
            JT::UnionAll {
                var, bound, body, ..
            } => {
                let var = CoreTypeVar {
                    name: var.clone(),
                    lower_bound: None,
                    upper_bound: bound.as_ref().and_then(|b| core_upper_bound_from_name(b)),
                };
                Self::UnionAll {
                    var,
                    body: Box::new(Self::from(body.as_ref())),
                }
            }
        }
    }
}

/// Structured `TypeExpr → CoreType` conversion (Issue #6720).
///
/// The direct, **string-free** resolver that lets lowering project a structured
/// `TypeExpr` into the structured `CoreType` hub. It replaces the
/// `to_string()` + `from_name_or_struct` round-trip
/// (`TYPE_REPRESENTATIONS.md` §3.3.1 / conversion #34) that collapses a
/// parametric application's params into an opaque `JuliaType::Struct(String)`:
/// a parametric `TypeExpr::Parameterized` now lands as a structured
/// `CoreType::Struct { name, params }` (or `CoreType::Tuple` / canonical
/// `CoreType::Union`), so the parameter structure survives into the hub.
///
/// Behaviour contract: `core_type_to_julia_type(&CoreType::from(te))` equals the
/// former `JuliaType::from_name_or_struct(&te.to_string())` for the
/// lowering-produced `TypeExpr` shapes (pinned by
/// `type_expr::tests::to_julia_type_lossy_matches_string_round_trip_issue_6720`).
impl From<&crate::types::TypeExpr> for CoreType {
    fn from(te: &crate::types::TypeExpr) -> Self {
        use crate::types::TypeExpr as TE;
        match te {
            // `Concrete` already holds a parsed `JuliaType`; reuse the canonical
            // `JuliaType → CoreType` hub directly (no render + reparse).
            TE::Concrete(jt) => CoreType::from(jt),
            // Leaf names: a bare type-var reference, a value parameter (e.g. the
            // `5` in `Val{5}`), or an unresolved user name. The structured
            // CoreType name parser resolves value params / known names exactly as
            // the old `from_name_or_struct` leaf path did.
            TE::TypeVar(name) => CoreType::from_julia_name(name),
            TE::RuntimeExpr(expr) => CoreType::from_julia_name(expr),
            TE::Parameterized { base, params } => {
                let core_params: Vec<CoreType> = params.iter().map(CoreType::from).collect();
                match base.as_str() {
                    // `Tuple{...}` is a structural tuple, not a nominal struct.
                    "Tuple" => CoreType::Tuple(core_params),
                    // `Union{...}` must be canonicalized (flatten / dedup /
                    // subtype-absorb / sort, Issue #5066) to match the
                    // `from_name_or_struct` parse of the rendered union.
                    "Union" => {
                        let members: Vec<crate::types::JuliaType> =
                            params.iter().map(|p| p.to_julia_type_lossy()).collect();
                        CoreType::from(&crate::types::canonicalize_union(members))
                    }
                    // Everything else is a nominal type application whose params
                    // are preserved structurally (`Pair{Int64, String}`,
                    // `Vector{Int64}`, user `MyBox{T}`, …). `core_type_to_julia_type`
                    // inverts the known containers (`Vector`/`Matrix`/…) and
                    // renders the rest back to a canonical `Struct(name)` spelling.
                    _ => CoreType::Struct {
                        name: base.clone(),
                        params: core_params,
                    },
                }
            }
        }
    }
}

impl From<&crate::types::TypeParam> for CoreTypeVar {
    fn from(param: &crate::types::TypeParam) -> Self {
        Self {
            name: param.name.clone(),
            lower_bound: param
                .lower_bound
                .as_ref()
                .and_then(|b| core_lower_bound_from_name(b)),
            upper_bound: param
                .get_upper_bound()
                .and_then(|b| core_upper_bound_from_name(b)),
        }
    }
}

#[cfg(feature = "aot")]
impl From<&crate::aot::types::StaticType> for CoreType {
    fn from(ty: &crate::aot::types::StaticType) -> Self {
        use crate::aot::types::StaticType as ST;
        match ty {
            ST::I64 => Self::Primitive(CorePrimitive::Int64),
            ST::I128 => Self::Primitive(CorePrimitive::Int128),
            ST::I32 => Self::Primitive(CorePrimitive::Int32),
            ST::I16 => Self::Primitive(CorePrimitive::Int16),
            ST::I8 => Self::Primitive(CorePrimitive::Int8),
            ST::U64 => Self::Primitive(CorePrimitive::UInt64),
            ST::U128 => Self::Primitive(CorePrimitive::UInt128),
            ST::U32 => Self::Primitive(CorePrimitive::UInt32),
            ST::U16 => Self::Primitive(CorePrimitive::UInt16),
            ST::U8 => Self::Primitive(CorePrimitive::UInt8),
            ST::F64 => Self::Primitive(CorePrimitive::Float64),
            ST::F32 => Self::Primitive(CorePrimitive::Float32),
            ST::F16 => Self::Primitive(CorePrimitive::Float16),
            ST::Bool => Self::Primitive(CorePrimitive::Bool),
            ST::Str => Self::Primitive(CorePrimitive::String),
            ST::Char => Self::Primitive(CorePrimitive::Char),
            ST::Nothing => Self::Primitive(CorePrimitive::Nothing),
            ST::Missing => Self::Primitive(CorePrimitive::Missing),
            ST::DataType => Self::Abstract(CoreAbstract::DataType),
            ST::Any => Self::Any,
            ST::Array { element, .. } => Self::Struct {
                name: "Array".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
            ST::Dict { key, value } => Self::Struct {
                name: "Dict".to_string(),
                params: vec![Self::from(key.as_ref()), Self::from(value.as_ref())],
            },
            ST::Set { element } => Self::Struct {
                name: "Set".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
            ST::Tuple(elements) => Self::Tuple(elements.iter().map(Self::from).collect()),
            ST::NamedTuple(fields) => Self::NamedTuple(
                fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), Self::from(ty)))
                    .collect(),
            ),
            ST::Union { variants } => Self::Union(variants.iter().map(Self::from).collect()),
            ST::Struct { name, .. } => Self::from_julia_name(name),
            ST::Function { .. } => Self::Abstract(CoreAbstract::Function),
            ST::Range { element } => Self::Struct {
                name: "AbstractRange".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
            ST::Generator { element } => Self::Struct {
                name: "Base.Generator".to_string(),
                params: vec![Self::from(element.as_ref())],
            },
        }
    }
}

/// Inverse of the `JuliaType → CoreType` bridge for the canonical spellings
/// produced by lowering (Issue #6336).
///
/// `JuliaType → CoreType` is not injective in general (e.g. `JT::Pairs` and
/// `JT::Struct("Pairs")` share one image), so this inverse picks, for every
/// `CoreType` shape, the canonical spelling used by cold projection accessors
/// — `Expr` resolves through `JuliaType::from_name` to the dedicated variant,
/// while `Pairs` has no `from_name` arm and stays a `Struct` spelling, etc.
/// The choice is *pinned over the whole Base corpus* by
/// `compile::cache::tests::base_method_signature_accessors_are_canonical_issue_6495`,
/// which fails the suite if any accessor stops matching this inverse.
///
/// Production use: cold JuliaType views derived from a `MethodSig`
/// `core_signature` for diagnostics and the small compatibility rules that
/// still operate on JuliaType (Issue #6495).
pub(crate) fn core_type_to_julia_type(core: &CoreType) -> crate::types::JuliaType {
    use crate::types::JuliaType as JT;
    match core {
        CoreType::Bottom => JT::Bottom,
        CoreType::Any => JT::Any,
        CoreType::Primitive(p) => match p {
            CorePrimitive::Int8 => JT::Int8,
            CorePrimitive::Int16 => JT::Int16,
            CorePrimitive::Int32 => JT::Int32,
            CorePrimitive::Int64 => JT::Int64,
            CorePrimitive::Int128 => JT::Int128,
            CorePrimitive::BigInt => JT::BigInt,
            CorePrimitive::UInt8 => JT::UInt8,
            CorePrimitive::UInt16 => JT::UInt16,
            CorePrimitive::UInt32 => JT::UInt32,
            CorePrimitive::UInt64 => JT::UInt64,
            CorePrimitive::UInt128 => JT::UInt128,
            CorePrimitive::Bool => JT::Bool,
            CorePrimitive::Float16 => JT::Float16,
            CorePrimitive::Float32 => JT::Float32,
            CorePrimitive::Float64 => JT::Float64,
            CorePrimitive::BigFloat => JT::BigFloat,
            CorePrimitive::String => JT::String,
            CorePrimitive::Char => JT::Char,
            CorePrimitive::Symbol => JT::Symbol,
            CorePrimitive::Nothing => JT::Nothing,
            CorePrimitive::Missing => JT::Missing,
        },
        CoreType::Abstract(a) => match a {
            CoreAbstract::Number => JT::Number,
            CoreAbstract::Real => JT::Real,
            CoreAbstract::Integer => JT::Integer,
            CoreAbstract::Signed => JT::Signed,
            CoreAbstract::Unsigned => JT::Unsigned,
            CoreAbstract::AbstractFloat => JT::AbstractFloat,
            CoreAbstract::AbstractString => JT::AbstractString,
            CoreAbstract::AbstractChar => JT::AbstractChar,
            CoreAbstract::AbstractArray => JT::AbstractArray,
            CoreAbstract::AbstractRange => JT::AbstractRange,
            CoreAbstract::Function => JT::Function,
            CoreAbstract::IO => JT::IO,
            CoreAbstract::Type => JT::Type,
            CoreAbstract::DataType => JT::DataType,
            // Abstract families WITHOUT a dedicated `JuliaType` variant keep
            // their `from_name_or_struct` spelling (`JuliaType::Struct(name)`).
            other => JT::Struct(CoreType::Abstract(other.clone()).to_julia_name()),
        },
        CoreType::AbstractUser { name, parent } => {
            JT::AbstractUser(name.clone(), parent.as_ref().map(|p| p.to_julia_name()))
        }
        CoreType::Module(_) => JT::Module,
        CoreType::Tuple(elems) => JT::TupleOf(elems.iter().map(core_type_to_julia_type).collect()),
        CoreType::Union(arms) => JT::Union(arms.iter().map(core_type_to_julia_type).collect()),
        CoreType::TypeOf(inner) => JT::TypeOf(Box::new(core_type_to_julia_type(inner))),
        CoreType::TypeVar(var) => JT::TypeVar(
            var.name.clone(),
            var.upper_bound.as_ref().map(|b| b.to_julia_name()),
        ),
        CoreType::UnionAll { var, body } => JT::UnionAll {
            var: var.name.clone(),
            lower_bound: var
                .lower_bound
                .as_ref()
                .map(|b| Box::new(b.to_julia_name())),
            bound: var
                .upper_bound
                .as_ref()
                .map(|b| Box::new(b.to_julia_name())),
            body: Box::new(core_type_to_julia_type(body)),
        },
        CoreType::Struct { name, params } => match (name.as_str(), params.len()) {
            ("Vector", 1) => JT::VectorOf(Box::new(core_type_to_julia_type(&params[0]))),
            ("Matrix", 1) => JT::MatrixOf(Box::new(core_type_to_julia_type(&params[0]))),
            ("Tuple", 0) => JT::Tuple,
            ("Array", 0) => JT::Array,
            ("Set", 0) => JT::Set,
            ("Dict", 0) => JT::Dict,
            ("NamedTuple", 0) => JT::NamedTuple,
            ("UnitRange", 0) => JT::UnitRange,
            ("StepRange", 0) => JT::StepRange,
            ("Generator", 0) => JT::Generator,
            ("IOBuffer", 0) => JT::IOBuffer,
            // The macro-system types resolve through `JuliaType::from_name` to
            // dedicated variants, so a `Struct` spelling of these names can
            // never be lowering-produced.
            ("Expr", 0) => JT::Expr,
            ("QuoteNode", 0) => JT::QuoteNode,
            ("LineNumberNode", 0) => JT::LineNumberNode,
            ("GlobalRef", 0) => JT::GlobalRef,
            _ => JT::Struct(render_canonical_struct_name(name, params)),
        },
        other => JT::Struct(other.to_julia_name()),
    }
}

/// Inverse of `CoreTypeVar::from(&TypeParam)` for the canonical bound
/// spellings, used to reconstruct `MethodSig.type_params` from the
/// `core_signature` `UnionAll` wrappers (Issue #6336).
pub(crate) fn core_type_var_to_type_param(var: &CoreTypeVar) -> crate::types::TypeParam {
    let upper = var.upper_bound.as_ref().map(|b| b.to_julia_name());
    let lower = var.lower_bound.as_ref().map(|b| b.to_julia_name());
    crate::types::TypeParam {
        name: var.name.clone(),
        upper_bound: upper.clone(),
        lower_bound: lower,
        // Legacy mirror of `upper_bound` (every production constructor keeps
        // them in sync; the field is `#[serde(skip)]`).
        bound: upper,
    }
}

/// Render a parametric struct spelling the way lowering writes it
/// (`AbstractVector{<:Integer}`, `Rational{T}`, `Pair{Int64, String}`):
/// bounded typevars keep their `<:` form instead of the `to_julia_name`
/// bare-variable rendering.
fn render_canonical_struct_name(name: &str, params: &[CoreType]) -> String {
    if params.is_empty() {
        return name.to_string();
    }
    let rendered: Vec<String> = params.iter().map(render_canonical_param).collect();
    format!("{name}{{{}}}", rendered.join(", "))
}

fn render_canonical_param(param: &CoreType) -> String {
    match param {
        CoreType::TypeVar(v) => match (&v.name, &v.upper_bound) {
            (n, Some(ub)) if n == "_" => format!("<:{}", ub.to_julia_name()),
            (n, Some(ub)) => format!("{n}<:{}", ub.to_julia_name()),
            (n, None) => n.clone(),
        },
        CoreType::Struct { name, params } => render_canonical_struct_name(name, params),
        other => other.to_julia_name(),
    }
}

fn core_upper_bound_from_name(bound: &str) -> Option<Box<CoreType>> {
    let normalized = bound
        .rsplit_once("<:")
        .map_or(bound, |(_, upper)| upper)
        .trim();
    (!normalized.is_empty() && normalized != "<:")
        .then(|| Box::new(CoreType::from_julia_name(normalized)))
}

fn core_lower_bound_from_name(bound: &str) -> Option<Box<CoreType>> {
    let normalized = bound
        .split_once(">:")
        .map_or(bound, |(_, lower)| lower)
        .trim();
    (!normalized.is_empty() && normalized != ">:")
        .then(|| Box::new(CoreType::from_julia_name(normalized)))
}

#[cfg(test)]
mod type_expr_to_core_tests {
    use super::*;
    use crate::types::{JuliaType, TypeExpr};

    /// Issue #6720: a parametric `TypeExpr::Parameterized` lands as a structured
    /// `CoreType::Struct { name, params }` — the parameters survive as real
    /// `CoreType`s instead of being collapsed into an opaque name string.
    #[test]
    fn parameterized_lands_as_structured_struct_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "Pair".to_string(),
            params: vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::Concrete(JuliaType::String),
            ],
        };
        assert_eq!(
            CoreType::from(&te),
            CoreType::Struct {
                name: "Pair".to_string(),
                params: vec![
                    CoreType::Primitive(CorePrimitive::Int64),
                    CoreType::Primitive(CorePrimitive::String),
                ],
            }
        );
    }

    /// `Tuple{...}` is structural, not nominal.
    #[test]
    fn parameterized_tuple_lands_as_core_tuple_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "Tuple".to_string(),
            params: vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::Concrete(JuliaType::Float64),
            ],
        };
        assert_eq!(
            CoreType::from(&te),
            CoreType::Tuple(vec![
                CoreType::Primitive(CorePrimitive::Int64),
                CoreType::Primitive(CorePrimitive::Float64),
            ])
        );
    }

    /// `Union{...}` is canonicalized (sorted/deduped) to match the parser.
    #[test]
    fn parameterized_union_is_canonicalized_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "Union".to_string(),
            params: vec![
                TypeExpr::Concrete(JuliaType::Int64),
                TypeExpr::Concrete(JuliaType::Nothing),
            ],
        };
        // Canonical order puts the singleton `Nothing` first (Issue #5066).
        assert_eq!(
            CoreType::from(&te),
            CoreType::Union(vec![
                CoreType::Primitive(CorePrimitive::Nothing),
                CoreType::Primitive(CorePrimitive::Int64),
            ])
        );
    }

    /// Nested parametric params recurse structurally (no string round-trip).
    #[test]
    fn nested_parameterized_recurses_structurally_issue_6720() {
        let te = TypeExpr::Parameterized {
            base: "MyBox".to_string(),
            params: vec![TypeExpr::Parameterized {
                base: "Pair".to_string(),
                params: vec![
                    TypeExpr::Concrete(JuliaType::Int64),
                    TypeExpr::Concrete(JuliaType::Float64),
                ],
            }],
        };
        assert_eq!(
            CoreType::from(&te),
            CoreType::Struct {
                name: "MyBox".to_string(),
                params: vec![CoreType::Struct {
                    name: "Pair".to_string(),
                    params: vec![
                        CoreType::Primitive(CorePrimitive::Int64),
                        CoreType::Primitive(CorePrimitive::Float64),
                    ],
                }],
            }
        );
    }
}
