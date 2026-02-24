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
            JuliaType::Set => "Set{Any}".into(),
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
            JuliaType::TypeVar(name, bound) => match bound {
                Some(b) => format!("{}<:{}", name, b).into(),
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
            JuliaType::UnionAll { var, bound, body } => {
                let bound_str = match bound {
                    Some(b) => format!("{}<:{}", var, b),
                    None => var.clone(),
                };
                format!("{} where {}", body.name(), bound_str).into()
            }
            // Enum type
            JuliaType::Enum(name) => name.clone().into(),
        }
    }
}

impl std::fmt::Display for JuliaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
