//! Type name parsing and construction for JuliaType.

use super::JuliaType;

/// Check if a name looks like a type variable (e.g., T, S, A, B, etc.)
/// Type variables are typically single uppercase letters.
/// We only accept single uppercase letters to avoid confusing type variables
/// with concrete types like Float64, Int64, etc.
pub(crate) fn is_type_variable_name(name: &str) -> bool {
    // Type variable names: single uppercase letter optionally followed by digits.
    // This covers: T, S, A, B (single letter), T1, T2, T3, S1 (letter + digits).
    // Needed for multi-parameter where clauses like `where {T1, T2, T3}` (Issue #2248).
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// Parse a covariant type bound like `<:Number` into a TypeVar with bound.
/// Returns None if the string doesn't match the `<:Type` pattern.
fn parse_covariant_bound(s: &str) -> Option<JuliaType> {
    let trimmed = s.trim();
    if let Some(bound_name) = trimmed.strip_prefix("<:") {
        let bound_name = bound_name.trim();
        if !bound_name.is_empty() {
            // Create an anonymous TypeVar with the bound
            // Use "_" as a placeholder name for anonymous covariant bounds
            return Some(JuliaType::TypeVar(
                "_".to_string(),
                Some(bound_name.to_string()),
            ));
        }
    }
    None
}

/// Parse a type parameter (inner part of Vector{...}, Array{...}, etc.)
/// Handles:
/// - Simple type variables: T -> TypeVar("T", None)
/// - Covariant bounds: <:Number -> TypeVar("_", Some("Number"))
/// - Concrete types: Int64 -> Int64
fn parse_parametric_inner(inner: &str) -> JuliaType {
    let inner = inner.trim();
    // Check for covariant bound pattern first
    if let Some(ty) = parse_covariant_bound(inner) {
        return ty;
    }
    // Check for type variable
    if is_type_variable_name(inner) {
        return JuliaType::TypeVar(inner.to_string(), None);
    }
    // Otherwise, parse as a concrete type or struct
    JuliaType::from_name_or_struct(inner)
}

/// Parse a parametric type name like "Complex{Float64}" into ("Complex", vec!["Float64"]).
/// Non-parametric names like "Int64" return ("Int64", vec![]).
pub(super) fn parse_parametric_name(name: &str) -> (&str, Vec<&str>) {
    if let Some(brace_idx) = name.find('{') {
        let base = &name[..brace_idx];
        let params_str = &name[brace_idx + 1..name.len() - 1]; // Remove { and }
        let params: Vec<&str> = params_str.split(',').map(|s| s.trim()).collect();
        (base, params)
    } else {
        (name, vec![])
    }
}

/// Parse union type arguments, respecting nested braces.
/// "Int64, Float64" -> vec!["Int64", "Float64"]
/// "Int64, Complex{Float64}" -> vec!["Int64", "Complex{Float64}"]
fn parse_union_type_args(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                let arg = s[start..i].trim();
                if !arg.is_empty() {
                    result.push(arg);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // Don't forget the last argument
    let last = s[start..].trim();
    if !last.is_empty() {
        result.push(last);
    }

    result
}

/// Normalize type aliases in parametric type names.
/// Converts short aliases to their canonical forms, e.g.:
/// - "Point{Int}" -> "Point{Int64}"
/// - "Point{UInt}" -> "Point{UInt64}"
/// - "Complex{Int, Int}" -> "Complex{Int64, Int64}"
fn normalize_type_aliases(name: &str) -> String {
    // Type alias mappings: replace bare "Int" and "UInt" type parameters with their
    // canonical names "Int64" and "UInt64". We must use word-boundary-aware replacement
    // to avoid corrupting compound names like "BigInt" → "BigInt64". (Issue #2497)
    //
    // Valid contexts where "Int" means "Int64":
    //   {Int}  {Int,  ,Int}  ,Int,  , Int}  , Int,
    // Invalid (should NOT be replaced):
    //   {BigInt}  {BigInt,  - "Int" is a suffix of "BigInt"

    let mut result = name.to_string();

    // Process each type parameter position by splitting on delimiters
    // We need to handle patterns like "Foo{Int, Bar}" → "Foo{Int64, Bar}"
    // without affecting "Foo{BigInt, Bar}"
    if result.contains('{') {
        if let Some(brace_start) = result.find('{') {
            if let Some(brace_end) = result.rfind('}') {
                let prefix = &result[..brace_start + 1];
                let params_str = &result[brace_start + 1..brace_end];
                let suffix = &result[brace_end..];

                let normalized_params: Vec<String> = params_str
                    .split(',')
                    .map(|param| {
                        let trimmed = param.trim();
                        match trimmed {
                            "Int" => param.replace("Int", "Int64"),
                            "UInt" => param.replace("UInt", "UInt64"),
                            _ => param.to_string(),
                        }
                    })
                    .collect();

                result = format!("{}{}{}", prefix, normalized_params.join(","), suffix);
            }
        }
    }

    result
}

impl JuliaType {
    /// Parse a type name string into a JuliaType.
    ///
    /// Returns `None` for unknown type names (including user-defined struct names).
    /// Use `from_name_or_struct` when you want to treat unknown names as struct types.
    ///
    /// # Important: Maintaining This Function
    ///
    /// When adding new `JuliaType` variants that have a standard Julia type name
    /// (e.g., `Function`, `Symbol`, `Expr`), you MUST add a corresponding match arm
    /// in this function. Failure to do so will cause type annotations like `f::Function`
    /// to be incorrectly treated as user-defined struct types.
    ///
    /// A unit test `test_from_name_builtin_coverage` validates that all builtin types
    /// with standard names have proper mappings. If you add a new type, add it to
    /// the test as well.
    ///
    /// See Issue #1328 for an example of bugs caused by missing mappings.
    pub fn from_name(name: &str) -> Option<JuliaType> {
        match name {
            // Signed integers
            "Int8" => Some(JuliaType::Int8),
            "Int16" => Some(JuliaType::Int16),
            "Int32" => Some(JuliaType::Int32),
            "Int64" | "Int" => Some(JuliaType::Int64),
            "Int128" => Some(JuliaType::Int128),
            "BigInt" => Some(JuliaType::BigInt),
            // Unsigned integers
            "UInt8" => Some(JuliaType::UInt8),
            "UInt16" => Some(JuliaType::UInt16),
            "UInt32" => Some(JuliaType::UInt32),
            "UInt64" | "UInt" => Some(JuliaType::UInt64),
            "UInt128" => Some(JuliaType::UInt128),
            // Boolean (subtype of Integer in Julia)
            "Bool" => Some(JuliaType::Bool),
            // Floating point
            "Float16" => Some(JuliaType::Float16),
            "Float32" => Some(JuliaType::Float32),
            "Float64" => Some(JuliaType::Float64),
            "BigFloat" => Some(JuliaType::BigFloat),
            // Note: Complex is now a user-defined struct, handled by from_name_or_struct
            // Other concrete types
            "String" => Some(JuliaType::String),
            "Char" => Some(JuliaType::Char),
            "Array" | "Vector" => Some(JuliaType::Array),
            "Tuple" => Some(JuliaType::Tuple),
            "NamedTuple" => Some(JuliaType::NamedTuple),
            "Dict" | "Dictionary" => Some(JuliaType::Dict),
            "Set" => Some(JuliaType::Set),
            "UnitRange" => Some(JuliaType::UnitRange),
            "StepRange" => Some(JuliaType::StepRange),
            "Nothing" => Some(JuliaType::Nothing),
            "Missing" => Some(JuliaType::Missing),

            // Abstract types
            "Any" => Some(JuliaType::Any),
            "Number" => Some(JuliaType::Number),
            "Real" => Some(JuliaType::Real),
            "Integer" => Some(JuliaType::Integer),
            "Signed" => Some(JuliaType::Signed),
            "Unsigned" => Some(JuliaType::Unsigned),
            "AbstractFloat" => Some(JuliaType::AbstractFloat),
            "AbstractString" => Some(JuliaType::AbstractString),
            "AbstractChar" => Some(JuliaType::AbstractChar),
            "AbstractArray" | "AbstractVector" => Some(JuliaType::AbstractArray),
            "AbstractRange" => Some(JuliaType::AbstractRange),
            "IO" => Some(JuliaType::IO),
            "IOBuffer" => Some(JuliaType::IOBuffer),
            "Module" => Some(JuliaType::Module),
            "Type" => Some(JuliaType::Type),
            "DataType" => Some(JuliaType::DataType),
            // Macro system types
            "Symbol" => Some(JuliaType::Symbol),
            "Expr" => Some(JuliaType::Expr),
            "QuoteNode" => Some(JuliaType::QuoteNode),
            "LineNumberNode" => Some(JuliaType::LineNumberNode),
            "GlobalRef" => Some(JuliaType::GlobalRef),
            // Function type (abstract supertype of all functions)
            "Function" => Some(JuliaType::Function),
            // Bottom type
            "Union{}" | "Bottom" => Some(JuliaType::Bottom),

            // Union{T1, T2, ...} pattern - union types
            _ if name.starts_with("Union{") && name.ends_with('}') => {
                let inner = &name[6..name.len() - 1]; // Remove "Union{" and "}"
                if inner.is_empty() {
                    return Some(JuliaType::Bottom);
                }
                // Parse comma-separated type list, respecting nested braces
                let types = parse_union_type_args(inner);
                if types.is_empty() {
                    return Some(JuliaType::Bottom);
                }
                let parsed_types: Vec<JuliaType> = types
                    .iter()
                    .map(|t| JuliaType::from_name_or_struct(t.trim()))
                    .collect();
                Some(JuliaType::Union(parsed_types))
            }

            // Vector{T} pattern - parametric 1D array
            // Handles: Vector{T}, Vector{Int64}, Vector{<:Number}
            _ if name.starts_with("Vector{") && name.ends_with('}') => {
                let inner = &name[7..name.len() - 1];
                let inner_type = parse_parametric_inner(inner);
                Some(JuliaType::VectorOf(Box::new(inner_type)))
            }

            // Array{T} or Array{T,N} pattern - parametric array
            // Handles: Array{T}, Array{Int64}, Array{<:Number}, Array{Int64,1}, Array{Int64,2}
            _ if name.starts_with("Array{") && name.ends_with('}') => {
                let inner = &name[6..name.len() - 1];
                // Check if there's a dimension parameter (e.g., "Int64,2" or "T,2")
                if let Some(comma_pos) = inner.rfind(',') {
                    let elem_type_str = inner[..comma_pos].trim();
                    let dim_str = inner[comma_pos + 1..].trim();
                    let elem_type = parse_parametric_inner(elem_type_str);

                    // Parse dimension: 1 = Vector, 2 = Matrix, other = general Array
                    match dim_str {
                        "1" => Some(JuliaType::VectorOf(Box::new(elem_type))),
                        "2" => Some(JuliaType::MatrixOf(Box::new(elem_type))),
                        _ => {
                            // For higher dimensions or type variables, use VectorOf as fallback
                            Some(JuliaType::VectorOf(Box::new(elem_type)))
                        }
                    }
                } else {
                    // No dimension parameter, treat as 1D array (Vector)
                    let inner_type = parse_parametric_inner(inner);
                    Some(JuliaType::VectorOf(Box::new(inner_type)))
                }
            }

            // Matrix{T} pattern - parametric 2D array
            // Handles: Matrix{T}, Matrix{Int64}, Matrix{<:Number}
            _ if name.starts_with("Matrix{") && name.ends_with('}') => {
                let inner = &name[7..name.len() - 1];
                let inner_type = parse_parametric_inner(inner);
                Some(JuliaType::MatrixOf(Box::new(inner_type)))
            }

            // Tuple{T1, T2, ...} pattern - parametric tuple types
            // Handles: Tuple{Int64, Int64}, Tuple{Union{Int64, String}, Float64}
            _ if name.starts_with("Tuple{") && name.ends_with('}') => {
                let inner = &name[6..name.len() - 1]; // Remove "Tuple{" and "}"
                if inner.is_empty() {
                    return Some(JuliaType::Tuple);
                }
                // Parse comma-separated type list, respecting nested braces
                let types = parse_union_type_args(inner);
                if types.is_empty() {
                    return Some(JuliaType::Tuple);
                }
                let parsed_types: Vec<JuliaType> = types
                    .iter()
                    .map(|t| JuliaType::from_name_or_struct(t.trim()))
                    .collect();
                Some(JuliaType::TupleOf(parsed_types))
            }

            // Type{T} pattern - matches type objects
            _ if name.starts_with("Type{") && name.ends_with('}') => {
                let inner = &name[5..name.len() - 1];
                // Use parse_parametric_inner to handle covariant bounds (Issue #2526)
                // e.g., Type{<:Animal} → TypeOf(TypeVar("_", Some("Animal")))
                let inner_type = parse_parametric_inner(inner);
                Some(JuliaType::TypeOf(Box::new(inner_type)))
            }
            _ => None,
        }
    }

    /// Parse a type name, treating unknown names as user-defined struct types.
    ///
    /// This should be used when parsing function signatures where the type
    /// might be a user-defined struct.
    pub fn from_name_or_struct(name: &str) -> JuliaType {
        Self::from_name(name).unwrap_or_else(|| {
            // Normalize type aliases in parametric struct types
            // (e.g., "Point{Int}" -> "Point{Int64}")
            let normalized = normalize_type_aliases(name);
            JuliaType::Struct(normalized)
        })
    }
}
