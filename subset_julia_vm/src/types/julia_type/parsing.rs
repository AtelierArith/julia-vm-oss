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

/// Parse a contravariant type bound like `>:Int` into an anonymous TypeVar whose
/// bound string carries a `>:` prefix (the dual of `parse_covariant_bound`). The
/// inner type name is normalized (`Int` -> native word type) so `Vector{>:Int}`
/// displays using the canonical concrete alias target; the display TypeVar arm
/// renders the `>:`-prefixed bound verbatim (Issue #5650).
fn parse_contravariant_bound(s: &str) -> Option<JuliaType> {
    let trimmed = s.trim();
    if let Some(bound_name) = trimmed.strip_prefix(">:") {
        let bound_name = bound_name.trim();
        if !bound_name.is_empty() {
            let normalized = JuliaType::from_name_or_struct(bound_name)
                .name()
                .to_string();
            return Some(JuliaType::TypeVar(
                "_".to_string(),
                Some(format!(">:{}", normalized)),
            ));
        }
    }
    None
}

/// Parse a type parameter (inner part of Vector{...}, Array{...}, etc.)
/// Handles:
/// - Simple type variables: T -> TypeVar("T", None)
/// - Covariant bounds: <:Number -> TypeVar("_", Some("Number"))
/// - Contravariant bounds: >:Int -> TypeVar("_", Some(">:Int64"))
/// - Concrete types: Int64 -> Int64
fn parse_parametric_inner(inner: &str) -> JuliaType {
    let inner = inner.trim();
    // Check for covariant / contravariant bound patterns first.
    if let Some(ty) = parse_covariant_bound(inner) {
        return ty;
    }
    if let Some(ty) = parse_contravariant_bound(inner) {
        return ty;
    }
    // Check for type variable
    if is_type_variable_name(inner) {
        return JuliaType::TypeVar(inner.to_string(), None);
    }
    // Otherwise, parse as a concrete type or struct
    JuliaType::from_name_or_struct(inner)
}

/// Recognize a trailing unbounded one-arg `Vararg{T}` marker that lives as the
/// last element of a `TupleOf` (Issue #4857).
///
/// `Tuple{Vararg{T}}` is the canonical spelling for a homogeneous tuple of any
/// length whose elements share type `T`. The single-argument `Vararg{T}` form
/// carries no length parameter, so it cannot be expanded into an `NTuple{N,T}`
/// the way the two-argument `Vararg{element,length}` form is (see
/// `JuliaType::from_name`). Instead the parser leaves it as the leaf
/// `Struct("Vararg{T}")` inside the tuple's element list, and the dispatch
/// matchers treat that trailing leaf as "binds all remaining arguments of type
/// `T`".
///
/// Returns the parsed element type when `ty` is such a marker, mirroring how
/// `CoreType::Vararg` is handled on the structured-type side.
pub(crate) fn unbounded_vararg_element(ty: &JuliaType) -> Option<JuliaType> {
    let JuliaType::Struct(name) = ty else {
        return None;
    };
    let name = name.trim();
    if !(name.starts_with("Vararg{") && name.ends_with('}')) {
        return None;
    }
    let inner = name[7..name.len() - 1].trim();
    // The two-argument form `Vararg{element, length}` is canonicalized into
    // `NTuple{N, element}` upstream of this point; only the single-argument
    // unbounded form should reach here.
    if split_parametric_args(inner).len() != 1 {
        return None;
    }
    Some(parse_parametric_inner(inner))
}

/// Parse a parametric type name like "Complex{Float64}" into ("Complex", vec!["Float64"]).
/// Non-parametric names like "Int64" return ("Int64", vec![]).
pub(super) fn parse_parametric_name(name: &str) -> (&str, Vec<&str>) {
    if let Some(brace_idx) = name.find('{') {
        let base = &name[..brace_idx];
        let params_str = &name[brace_idx + 1..name.len() - 1]; // Remove { and }
        let params = split_parametric_args(params_str);
        (base, params)
    } else {
        (name, vec![])
    }
}

fn split_parametric_args(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return vec![];
    }

    let mut result = Vec::new();
    let mut brace_depth = 0;
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }

        match c {
            '\'' | '"' => quote = Some(c),
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                result.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(s[start..].trim());
    result
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
/// - "Point{Int}" -> "Point{Int64}" on 64-bit targets, "Point{Int32}" on 32-bit targets
/// - "Point{UInt}" -> "Point{UInt64}" on 64-bit targets, "Point{UInt32}" on 32-bit targets
/// - "Complex{Int, Int}" -> concrete native-word aliases for both params
fn normalize_type_aliases(name: &str) -> String {
    // Type alias mappings: replace bare "Int" and "UInt" type parameters with their
    // canonical native-word names. We must use word-boundary-aware replacement to
    // avoid corrupting compound names like "BigInt" → "BigInt64". (Issue #2497)
    //
    // Valid contexts where "Int" means "Int64":
    //   {Int}  {Int,  ,Int}  ,Int,  , Int}  , Int,
    // Invalid (should NOT be replaced):
    //   {BigInt}  {BigInt,  - "Int" is a suffix of "BigInt"

    // Process each type parameter position by splitting on delimiters
    // We need to handle patterns like "Foo{Int, Bar}" → "Foo{Int64, Bar}"
    // without affecting "Foo{BigInt, Bar}"
    if let Some(brace_start) = name.find('{') {
        if let Some(brace_end) = name.rfind('}') {
            let prefix = &name[..brace_start + 1];
            let params_str = &name[brace_start + 1..brace_end];
            let suffix = &name[brace_end..];

            let normalized_params: Vec<String> = split_parametric_args(params_str)
                .into_iter()
                .map(normalize_parametric_arg_token)
                .collect();

            return format!("{}{}{}", prefix, normalized_params.join(", "), suffix);
        }
    }

    canonicalize_typed_unsigned_value_param(name).unwrap_or_else(|| name.to_string())
}

fn normalize_parametric_arg_token(param: &str) -> String {
    let trimmed = param.trim();
    match trimmed {
        "Int" => crate::types::native_int_type_name().to_string(),
        "UInt" => crate::types::native_uint_type_name().to_string(),
        _ => canonicalize_typed_unsigned_value_param(trimmed).unwrap_or_else(|| {
            if trimmed.contains('{') {
                normalize_type_aliases(trimmed)
            } else {
                trimmed.to_string()
            }
        }),
    }
}

fn canonicalize_typed_unsigned_value_param(token: &str) -> Option<String> {
    for (bits, width) in [(8_u16, 2_usize), (16, 4), (32, 8), (64, 16), (128, 32)] {
        let prefix = format!("UInt{bits}(");
        let Some(inner) = token
            .strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix(')'))
        else {
            continue;
        };
        let value = inner.parse::<u128>().ok()?;
        let in_range = match bits {
            8 => u8::try_from(value).is_ok(),
            16 => u16::try_from(value).is_ok(),
            32 => u32::try_from(value).is_ok(),
            64 => u64::try_from(value).is_ok(),
            128 => true,
            _ => false,
        };
        if !in_range {
            return None;
        }
        return Some(format!("0x{value:0width$x}"));
    }
    None
}

/// Parse the parenthesized field-name tuple of a type-level `NamedTuple`
/// (Issue #5063). Accepts the `(:a, :b)` / `(:a,)` symbol-tuple spelling that
/// upstream uses for the first `NamedTuple` parameter and returns the bare
/// field names (`["a", "b"]`).
///
/// Returns `None` when the first parameter is not a literal symbol tuple — for
/// example a type variable (`NamedTuple{names, T} where names`), which is the
/// fully generic form deferred by Issue #5063.
fn parse_named_tuple_field_names(names_param: &str) -> Option<Vec<String>> {
    let trimmed = names_param.trim();
    let inner = trimmed.strip_prefix('(')?.strip_suffix(')')?;
    // `()` is the empty named tuple `NamedTuple{(), Tuple{}}`.
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut names = Vec::new();
    for raw in parse_union_type_args(inner) {
        // Each entry is a quoted symbol `:name`; reject anything else (a type
        // variable or a non-symbol value) so the generic form stays deferred.
        let sym = raw.trim().strip_prefix(':')?;
        if sym.is_empty() || !is_plain_identifier(sym) {
            return None;
        }
        names.push(sym.to_string());
    }
    Some(names)
}

/// Whether `s` is a plain Julia identifier (used to validate `NamedTuple` field
/// names). Field names are always simple identifiers in the supported forms.
fn is_plain_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Canonicalize the inner parameters of a type-level `NamedTuple{...}`
/// (Issue #5063) into the internal named-tuple representation.
///
/// Two supported forms:
/// - `NamedTuple{(:a, :b), Tuple{Int, Float64}}` → the concrete named-tuple
///   type `Struct("@NamedTuple{a::Int64, b::Float64}")`, identical to
///   `typeof((a=1, b=2.0))` and the `@NamedTuple` macro result. A field typed
///   `Any` collapses to the bare `name`, matching the canonical printed form.
/// - `NamedTuple{(:a, :b)}` (field type omitted) → the names-only UnionAll
///   marker `Struct("NamedTuple{(:a, :b)}")`, which `isa`/subtype treat as a
///   supertype of every concrete named tuple with exactly those field names in
///   that order.
///
/// Returns `None` for the fully generic form (`NamedTuple{names, T}` over type
/// variables) or a field-type tuple whose arity disagrees with the names; those
/// remain deferred to Issue #5063.
/// Split the top-level parameters of a `NamedTuple{...}` body, respecting both
/// `{}` and `()` nesting. The names parameter is a parenthesized symbol tuple
/// such as `(:a, :b)` whose inner comma must NOT split the parameters, so the
/// shared `parse_union_type_args` (which only tracks `{}`) is not sufficient
/// here (Issue #5063).
fn split_named_tuple_params(inner: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                let arg = inner[start..i].trim();
                if !arg.is_empty() {
                    result.push(arg);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        result.push(last);
    }
    result
}

fn canonicalize_named_tuple_type(inner: &str) -> Option<JuliaType> {
    let params = split_named_tuple_params(inner);
    if params.is_empty() {
        // Bare `NamedTuple{}` is not a valid spelling; let the caller fall back
        // to the unparameterized `NamedTuple`.
        return None;
    }

    let names = parse_named_tuple_field_names(params[0])?;

    match params.len() {
        // Names-only form: `NamedTuple{(:a, :b)}`.
        1 => {
            let canonical_names = names
                .iter()
                .map(|n| format!(":{}", n))
                .collect::<Vec<_>>()
                .join(", ");
            let trailing = if names.len() == 1 { "," } else { "" };
            Some(JuliaType::Struct(format!(
                "NamedTuple{{({}{})}}",
                canonical_names, trailing
            )))
        }
        // Names + field-type tuple: `NamedTuple{(:a, :b), Tuple{Int, Float64}}`.
        2 => {
            let types_param = params[1].trim();
            let field_types_str = types_param
                .strip_prefix("Tuple{")
                .and_then(|s| s.strip_suffix('}'))?;
            let field_type_args: Vec<&str> = if field_types_str.trim().is_empty() {
                Vec::new()
            } else {
                parse_union_type_args(field_types_str)
            };
            if field_type_args.len() != names.len() {
                // Arity mismatch is not a well-formed concrete named tuple.
                return None;
            }
            let fields: Vec<String> = names
                .iter()
                .zip(field_type_args.iter())
                .map(|(name, ty)| {
                    let canonical_ty = JuliaType::from_name_or_struct(ty.trim()).name().to_string();
                    // Upstream prints an `Any`-typed field as the bare name, so
                    // collapse `::Any` to match the canonical `@NamedTuple{...}`.
                    if canonical_ty == "Any" {
                        name.clone()
                    } else {
                        format!("{}::{}", name, canonical_ty)
                    }
                })
                .collect();
            Some(JuliaType::Struct(format!(
                "@NamedTuple{{{}}}",
                fields.join(", ")
            )))
        }
        // More than two parameters is not a valid `NamedTuple` spelling.
        _ => None,
    }
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
            "Int64" => Some(JuliaType::Int64),
            "Int" => Some(crate::types::native_int_julia_type()),
            "Int128" => Some(JuliaType::Int128),
            "BigInt" => Some(JuliaType::BigInt),
            // Unsigned integers
            "UInt8" => Some(JuliaType::UInt8),
            "UInt16" => Some(JuliaType::UInt16),
            "UInt32" => Some(JuliaType::UInt32),
            "UInt64" => Some(JuliaType::UInt64),
            "UInt" => Some(crate::types::native_uint_julia_type()),
            "UInt128" => Some(JuliaType::UInt128),
            // Boolean (subtype of Integer in Julia)
            "Bool" => Some(JuliaType::Bool),
            // Floating point
            "Float16" => Some(JuliaType::Float16),
            "Float32" => Some(JuliaType::Float32),
            "Float64" => Some(JuliaType::Float64),
            "BigFloat" => Some(JuliaType::BigFloat),
            "ComplexF64" => Some(JuliaType::Struct("Complex{Float64}".to_string())),
            "ComplexF32" => Some(JuliaType::Struct("Complex{Float32}".to_string())),
            // Note: Complex is now a user-defined struct, handled by from_name_or_struct
            // Other concrete types
            "String" => Some(JuliaType::String),
            "Char" => Some(JuliaType::Char),
            "Array" => Some(JuliaType::Array),
            "Vector" => Some(JuliaType::Struct("Vector".to_string())),
            "Matrix" => Some(JuliaType::Struct("Matrix".to_string())),
            "DenseArray" => Some(JuliaType::Struct("DenseArray".to_string())),
            "DenseVector" => Some(JuliaType::Struct("DenseVector".to_string())),
            "DenseMatrix" => Some(JuliaType::Struct("DenseMatrix".to_string())),
            "BitArray" => Some(JuliaType::Struct("BitArray".to_string())),
            "BitVector" => Some(JuliaType::Struct("BitVector".to_string())),
            "BitMatrix" => Some(JuliaType::Struct("BitMatrix".to_string())),
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
            "AbstractArray" => Some(JuliaType::AbstractArray),
            "AbstractVector" | "AbstractMatrix" => Some(JuliaType::Struct(name.to_string())),
            "AbstractRange" => Some(JuliaType::AbstractRange),
            "IO" => Some(JuliaType::IO),
            "IOBuffer" => Some(JuliaType::IOBuffer),
            "Module" => Some(JuliaType::Module),
            "Type" => Some(JuliaType::Type),
            "DataType" => Some(JuliaType::DataType),
            "Union" => Some(JuliaType::Struct("Union".to_string())),
            "UnionAll" => Some(JuliaType::Struct("UnionAll".to_string())),
            "TypeVar" => Some(JuliaType::Struct("TypeVar".to_string())),
            "Ptr" => Some(JuliaType::Struct("Ptr".to_string())),
            "NTuple" => Some(JuliaType::Struct("NTuple".to_string())),
            // Macro system types
            "Symbol" => Some(JuliaType::Symbol),
            "Expr" => Some(JuliaType::Expr),
            "QuoteNode" => Some(JuliaType::QuoteNode),
            "LineNumberNode" => Some(JuliaType::LineNumberNode),
            "GlobalRef" => Some(JuliaType::GlobalRef),
            "Generator" | "Base.Generator" => Some(JuliaType::Generator),
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
                // Issue #5066: canonicalize (flatten / dedup / subtype-absorb /
                // sort / collapse) so equal Unions share one normal form and
                // compare `===` regardless of nesting depth, order, or dups.
                Some(canonicalize_union(parsed_types))
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
                            // Preserve higher-dimensional and symbolic rank forms as
                            // `Array{T,N}`. Julia's `Vector`/`Matrix` aliases only
                            // cover ranks 1 and 2.
                            Some(JuliaType::Struct(format!(
                                "Array{{{}, {}}}",
                                elem_type.name(),
                                dim_str
                            )))
                        }
                    }
                } else {
                    // `Array{T}` is a UnionAll over the rank parameter, not
                    // the `Vector{T}` alias. Keep it as an Array pattern so
                    // methods like `size(a::Array{T}) where T` match matrices
                    // and higher-dimensional arrays.
                    let inner_type = parse_parametric_inner(inner);
                    Some(JuliaType::Struct(format!("Array{{{}}}", inner_type.name())))
                }
            }

            // BitArray{N} aliases: BitArray{1} === BitVector,
            // BitArray{2} === BitMatrix. Higher ranks keep the explicit
            // BitArray{N} surface while storage remains Bool-array backed
            // (Issue #5498).
            _ if name.starts_with("BitArray{") && name.ends_with('}') => {
                let inner = name["BitArray{".len()..name.len() - 1].trim();
                match inner.parse::<usize>().ok() {
                    Some(1) => Some(JuliaType::Struct("BitVector".to_string())),
                    Some(2) => Some(JuliaType::Struct("BitMatrix".to_string())),
                    Some(n) => Some(JuliaType::Struct(format!("BitArray{{{n}}}"))),
                    None => Some(JuliaType::Struct(name.to_string())),
                }
            }

            // Matrix{T} pattern - parametric 2D array
            // Handles: Matrix{T}, Matrix{Int64}, Matrix{<:Number}
            _ if name.starts_with("Matrix{") && name.ends_with('}') => {
                let inner = &name[7..name.len() - 1];
                let inner_type = parse_parametric_inner(inner);
                Some(JuliaType::MatrixOf(Box::new(inner_type)))
            }

            // NTuple{N} / NTuple{N,T} alias - fixed-length tuple.
            // Official Julia treats NTuple{5, Int64} as Tuple{Int64, ...}
            // with five slots, and NTuple{5} as Tuple{Any, ...}. Keep symbolic
            // lengths in the structured CoreType path; JuliaType can safely
            // expand concrete integer lengths.
            _ if name.starts_with("NTuple{") && name.ends_with('}') => {
                let inner = &name[7..name.len() - 1]; // Remove "NTuple{" and "}"
                let params = parse_union_type_args(inner);
                if !(params.len() == 1 || params.len() == 2) {
                    return None;
                }
                let len = params[0].trim().parse::<usize>().ok()?;
                let elem_type = params
                    .get(1)
                    .map(|elem| parse_parametric_inner(elem))
                    .unwrap_or(JuliaType::Any);
                Some(JuliaType::TupleOf(vec![elem_type; len]))
            }

            // Tuple{T1, T2, ...} pattern - parametric tuple types
            // Handles: Tuple{Int64, Int64}, Tuple{Union{Int64, String}, Float64}
            _ if name.starts_with("Tuple{") && name.ends_with('}') => {
                let inner = &name[6..name.len() - 1]; // Remove "Tuple{" and "}"
                if inner.is_empty() {
                    return Some(JuliaType::TupleOf(Vec::new()));
                }
                // Parse comma-separated type list, respecting nested braces
                let types = parse_union_type_args(inner);
                if types.is_empty() {
                    return Some(JuliaType::TupleOf(Vec::new()));
                }
                // Issue #4841: Tuple{Vararg{T,N}} is upstream's canonical
                // spelling for NTuple{N,T}. Translate the two-argument
                // Vararg form (`Tuple{Vararg{element, length}}`) into the
                // equivalent `NTuple{length, element}` so the existing
                // NTuple dispatch, val-parameter, and runtime-binding paths
                // (vm/mod.rs ~L2600, compile/mod.rs ~L1733) pick it up
                // unchanged. The single-argument `Vararg{T}` (unbounded
                // length) is left untranslated and tracked separately —
                // it does not bind a length parameter and would require a
                // distinct path.
                if types.len() == 1 {
                    let only = types[0].trim();
                    if only.starts_with("Vararg{") && only.ends_with('}') {
                        let vararg_inner = &only[7..only.len() - 1];
                        let vararg_params = parse_union_type_args(vararg_inner);
                        if vararg_params.len() == 2 {
                            let elem = vararg_params[0].trim();
                            let len = vararg_params[1].trim();
                            return Some(
                                Self::from_name(&format!("NTuple{{{}, {}}}", len, elem))
                                    .unwrap_or_else(|| {
                                        JuliaType::Struct(format!("NTuple{{{}, {}}}", len, elem))
                                    }),
                            );
                        }
                    }
                }
                let parsed_types: Vec<JuliaType> = types
                    .iter()
                    .map(|t| parse_parametric_inner(t.trim()))
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

            // Type-level `NamedTuple{names, T}` / `NamedTuple{names}` (Issue #5063).
            // Canonicalize the upstream spelling into the same internal form that
            // `typeof((a=1, b=2))` and the `@NamedTuple` macro (Issue #5120)
            // produce, so subtype / `isa` / dispatch / `===` all reuse the
            // existing named-tuple machinery.
            _ if name.starts_with("NamedTuple{") && name.ends_with('}') => {
                let inner = &name[11..name.len() - 1];
                canonicalize_named_tuple_type(inner)
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
            // (e.g., "Point{Int}" -> native concrete word type)
            let normalized = normalize_type_aliases(name);
            JuliaType::Struct(normalized)
        })
    }
}

/// Sort rank for the canonical `Union` ordering (Issue #5066).
///
/// Mirrors upstream `union_sort_cmp` in `julia/src/jltypes.c`, which orders
/// `Union` members as: singleton `DataType`s, then `isbits` `DataType`s, then
/// other `DataType`s, and finally non-`DataType`s (e.g. `UnionAll` aliases such
/// as `Vector`/`Dict`). Within a rank, ties break by name (`datatype_name_cmp`).
fn union_sort_rank(ty: &JuliaType) -> u8 {
    if union_member_is_singleton(ty) {
        0
    } else if union_member_is_isbits(ty) {
        1
    } else if union_member_is_non_datatype(ty) {
        3
    } else {
        2
    }
}

/// Whether `ty` is a singleton `DataType` for the purpose of `Union` ordering.
///
/// Upstream sorts singleton `DataType`s (single-instance types such as the
/// types of `nothing` and `missing`) ahead of every other member.
fn union_member_is_singleton(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Nothing | JuliaType::Missing)
}

/// Whether `ty` is an `isbits` `DataType` for the purpose of `Union` ordering.
///
/// These are the immutable, pointer-free concrete types — the numeric
/// primitives plus `Bool` and `Char`. `String`/`Symbol` are *not* `isbits`
/// (they reference heap data) and so sort after this rank.
fn union_member_is_isbits(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Int8
            | JuliaType::Int16
            | JuliaType::Int32
            | JuliaType::Int64
            | JuliaType::Int128
            | JuliaType::UInt8
            | JuliaType::UInt16
            | JuliaType::UInt32
            | JuliaType::UInt64
            | JuliaType::UInt128
            | JuliaType::Bool
            | JuliaType::Float16
            | JuliaType::Float32
            | JuliaType::Float64
            | JuliaType::Char
    )
}

/// Whether `ty` is *not* a `DataType` for the purpose of `Union` ordering.
///
/// `UnionAll` aliases (`Vector`, `Matrix`, `Dict`, `Set`, `Array`, ...) and
/// explicit `UnionAll`s sort after all `DataType`s, matching upstream's
/// `union_sort_cmp` (which puts non-`DataType`s last).
fn union_member_is_non_datatype(ty: &JuliaType) -> bool {
    matches!(
        ty,
        JuliaType::Array
            | JuliaType::VectorOf(_)
            | JuliaType::MatrixOf(_)
            | JuliaType::Dict
            | JuliaType::Set
            | JuliaType::UnionAll { .. }
    )
}

/// Compare two `Union` members by upstream's canonical order (Issue #5066).
///
/// First by sort rank (singleton < isbits < other DataType < non-DataType),
/// then alphabetically by display name (`datatype_name_cmp`). This keeps
/// `Union{Int, Float64}` and `Union{Float64, Int}` identical after sorting.
fn union_sort_cmp(a: &JuliaType, b: &JuliaType) -> std::cmp::Ordering {
    union_sort_rank(a)
        .cmp(&union_sort_rank(b))
        .then_with(|| a.name().cmp(&b.name()))
}

/// Build the canonical normal form of `Union{...}` from its members
/// (Issue #5066), matching upstream `jl_type_union` in `julia/src/jltypes.c`.
///
/// Steps:
/// 1. **Flatten** nested `Union`s and drop `Bottom` (`Union{}`) members.
/// 2. **Subtype absorption / dedup**: drop any member that is a (non-strict)
///    subtype of another distinct member (`Int <: Integer` removes `Int`;
///    duplicates are removed because `A <: A`).
/// 3. **Sort** survivors into the canonical order via [`union_sort_cmp`].
/// 4. **Collapse**: zero members → `Bottom`, one member → that member,
///    otherwise a sorted `Union`.
///
/// Equal `Union`s therefore share one normal form and compare `===` regardless
/// of nesting depth, member order, or duplicates.
pub(crate) fn canonicalize_union(members: Vec<JuliaType>) -> JuliaType {
    // 1. Recursively flatten nested unions and drop Bottom members.
    let mut flat: Vec<JuliaType> = Vec::new();
    fn flatten(ty: JuliaType, out: &mut Vec<JuliaType>) {
        match ty {
            JuliaType::Bottom => {}
            JuliaType::Union(inner) => {
                for member in inner {
                    flatten(member, out);
                }
            }
            other => out.push(other),
        }
    }
    for member in members {
        flatten(member, &mut flat);
    }

    // 2. Subtype absorption (also removes exact duplicates since `A <: A`).
    //    Keep `flat[i]` only if no *distinct* surviving member is a supertype of
    //    it. We compare against the original list so that, e.g., `Union{Int,Int}`
    //    drops exactly one copy rather than both.
    let mut kept: Vec<JuliaType> = Vec::new();
    'outer: for (i, candidate) in flat.iter().enumerate() {
        for (j, other) in flat.iter().enumerate() {
            if i == j {
                continue;
            }
            // `candidate <: other`: candidate is absorbed by `other`. To avoid
            // dropping *both* members of an equal pair, when the two are
            // mutually subtypes (i.e. equal) keep only the first occurrence.
            if candidate.is_subtype_of(other) {
                let mutual = other.is_subtype_of(candidate);
                if !mutual || j < i {
                    continue 'outer;
                }
            }
        }
        kept.push(candidate.clone());
    }

    // 3. Canonical sort (stable so equal-ranked equal-name members are stable).
    kept.sort_by(union_sort_cmp);

    // 4. Collapse singleton / empty unions.
    match kept.len() {
        0 => JuliaType::Bottom,
        1 => kept.into_iter().next().unwrap_or(JuliaType::Bottom),
        _ => JuliaType::Union(kept),
    }
}
