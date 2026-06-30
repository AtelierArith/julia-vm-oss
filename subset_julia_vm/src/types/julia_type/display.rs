//! Display name and formatting for JuliaType.

use super::JuliaType;

impl JuliaType {
    /// Get the display name for this type.
    pub fn name(&self) -> std::borrow::Cow<'static, str> {
        match self {
            // Signed integers
            JuliaType::Int8 => "Int8".into(),
            JuliaType::Int16 => "Int16".into(),
            JuliaType::Int32 => "Int32".into(),
            JuliaType::Int64 => "Int64".into(),
            JuliaType::Int128 => "Int128".into(),
            JuliaType::BigInt => "BigInt".into(),
            // Unsigned integers
            JuliaType::UInt8 => "UInt8".into(),
            JuliaType::UInt16 => "UInt16".into(),
            JuliaType::UInt32 => "UInt32".into(),
            JuliaType::UInt64 => "UInt64".into(),
            JuliaType::UInt128 => "UInt128".into(),
            // Boolean
            JuliaType::Bool => "Bool".into(),
            // Floating point
            JuliaType::Float16 => "Float16".into(),
            JuliaType::Float32 => "Float32".into(),
            JuliaType::Float64 => "Float64".into(),
            JuliaType::BigFloat => "BigFloat".into(),
            // Other concrete types
            JuliaType::String => "String".into(),
            JuliaType::Char => "Char".into(),
            JuliaType::Array => "Array".into(),
            JuliaType::VectorOf(elem_type) => format!("Vector{{{}}}", elem_type.name()).into(),
            JuliaType::MatrixOf(elem_type) => format!("Matrix{{{}}}", elem_type.name()).into(),
            JuliaType::Tuple => "Tuple".into(),
            JuliaType::TupleOf(types) => {
                let type_names: Vec<String> = types.iter().map(|t| t.name().to_string()).collect();
                format!("Tuple{{{}}}", type_names.join(", ")).into()
            }
            JuliaType::NamedTuple => "NamedTuple".into(),
            JuliaType::Dict => "Dict".into(),
            JuliaType::Set => "Set".into(),
            JuliaType::UnitRange => "UnitRange".into(),
            JuliaType::StepRange => "StepRange".into(),
            // Abstract types
            JuliaType::Any => "Any".into(),
            JuliaType::Number => "Number".into(),
            JuliaType::Real => "Real".into(),
            JuliaType::Integer => "Integer".into(),
            JuliaType::Signed => "Signed".into(),
            JuliaType::Unsigned => "Unsigned".into(),
            JuliaType::AbstractFloat => "AbstractFloat".into(),
            JuliaType::AbstractString => "AbstractString".into(),
            JuliaType::AbstractChar => "AbstractChar".into(),
            JuliaType::AbstractArray => "AbstractArray".into(),
            JuliaType::AbstractRange => "AbstractRange".into(),
            JuliaType::Function => "Function".into(),
            JuliaType::IO => "IO".into(),
            JuliaType::IOBuffer => "IOBuffer".into(),
            // Special types
            JuliaType::Nothing => "Nothing".into(),
            JuliaType::Missing => "Missing".into(),
            JuliaType::Module => "Module".into(),
            JuliaType::Type => "Type".into(),
            JuliaType::DataType => "DataType".into(),
            // Macro system types
            JuliaType::Symbol => "Symbol".into(),
            JuliaType::Expr => "Expr".into(),
            JuliaType::QuoteNode => "QuoteNode".into(),
            JuliaType::LineNumberNode => "LineNumberNode".into(),
            JuliaType::GlobalRef => "GlobalRef".into(),
            // Base.Pairs type (for kwargs...)
            JuliaType::Pairs => "Base.Pairs".into(),
            // Base.Generator type (for generator expressions)
            JuliaType::Generator => "Base.Generator".into(),
            JuliaType::Struct(name) => name.clone().into(),
            JuliaType::AbstractUser(name, _) => name.clone().into(),
            // An ANONYMOUS bounded typevar — the internal placeholder name `_`,
            // produced when parsing the covariant shorthand `Vector{<:Integer}` —
            // prints with the bound-only shorthand upstream (`<:Upper`), never
            // echoing the `_` placeholder. A named typevar keeps its name
            // (`T<:Integer`) (Issue #5644).
            JuliaType::TypeVar(name, bound) => match bound {
                // A `>:`-prefixed bound encodes the anonymous contravariant
                // shorthand `Vector{>:Int}` (a lower bound on an unnamed var);
                // render it verbatim (already normalized to `>:Int64`) (#5650).
                Some(b) if name == "_" && b.starts_with(">:") => b.clone().into(),
                Some(b) if name == "_" => format!("<:{}", normalize_bound(b)).into(),
                Some(b) => format!("{}<:{}", name, normalize_bound(b)).into(),
                None => name.clone().into(),
            },
            // Bottom type
            JuliaType::Bottom => "Union{}".into(),
            // Union type
            JuliaType::Union(types) => {
                let type_names: Vec<String> = types.iter().map(|t| t.name().to_string()).collect();
                format!("Union{{{}}}", type_names.join(", ")).into()
            }
            // Type{T} pattern
            JuliaType::TypeOf(inner) => format!("Type{{{}}}", inner.name()).into(),
            // UnionAll type
            JuliaType::UnionAll {
                var,
                lower_bound,
                bound,
                body,
            } => format_unionall_name(
                var,
                lower_bound.as_deref().map(String::as_str),
                bound.as_deref().map(String::as_str),
                body,
            )
            .into(),
            // Enum type
            JuliaType::Enum(name) => name.clone().into(),
        }
    }
}

fn format_unionall_name(
    var: &str,
    lower_bound: Option<&str>,
    bound: Option<&str>,
    body: &JuliaType,
) -> String {
    let mut clauses = vec![format_where_clause(var, lower_bound, bound)];
    let mut base = body;
    while let JuliaType::UnionAll {
        var,
        lower_bound,
        bound,
        body,
    } = base
    {
        clauses.push(format_where_clause(
            var,
            lower_bound.as_deref().map(String::as_str),
            bound.as_deref().map(String::as_str),
        ));
        base = body;
    }

    if clauses.len() == 1 {
        format!("{} where {}", base.name(), clauses[0])
    } else {
        format!("{} where {{{}}}", base.name(), clauses.join(", "))
    }
}

fn format_where_clause(var: &str, lower_bound: Option<&str>, bound: Option<&str>) -> String {
    match (lower_bound, bound) {
        (None, None) => var.to_string(),
        (None, Some(u)) => format!("{}<:{}", var, normalize_bound(u)),
        // `where var>:Lower` keeps only the lower bound (#5650).
        (Some(l), None) => format!("{}>:{}", var, normalize_bound(l)),
        // `where Lower<:var<:Upper`.
        (Some(l), Some(u)) => format!("{}<:{}<:{}", normalize_bound(l), var, normalize_bound(u)),
    }
}

/// Normalize a where-bound type name so nested word aliases like `Int`/`UInt`
/// render as their canonical `Int64`/`UInt64` spellings (matching how upstream
/// prints `where Int64<:T<:Real`). A name that is not a known type round-trips
/// unchanged (Issue #5650).
fn normalize_bound(bound: &str) -> String {
    JuliaType::from_name_or_struct(bound).name().to_string()
}

impl std::fmt::Display for JuliaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
