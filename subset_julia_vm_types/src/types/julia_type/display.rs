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
                Some(b) if two_sided_typevar_bound(b, name) => b.clone().into(),
                // A `>:`-prefixed bound encodes the anonymous contravariant
                // shorthand `Vector{>:Int}` (a lower bound on an unnamed var);
                // render it verbatim (already normalized to `>:Int64`) (#5650).
                Some(b) if name == "_" && b.starts_with(">:") => b.clone().into(),
                Some(b) if b.starts_with(">:") => format!("{name}{b}").into(),
                Some(b) if name == "_" => format!("<:{}", normalize_bound(b)).into(),
                Some(b) => format!("{}<:{}", name, normalize_bound(b)).into(),
                None => name.clone().into(),
            },
            JuliaType::RuntimeTypeVar {
                name,
                lower_bound,
                upper_bound,
                ..
            } => format_typevar_interval(name, lower_bound, upper_bound).into(),
            JuliaType::RuntimeParametric { base, params } => format!(
                "{base}{{{}}}",
                params
                    .iter()
                    .map(|param| param.name().into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into(),
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
            } => format_unionall_name_with(
                var,
                lower_bound.as_deref().map(String::as_str),
                bound.as_deref().map(String::as_str),
                body,
                false,
            )
            .into(),
            JuliaType::RuntimeUnionAll { .. } => format_runtime_unionall_name(self).into(),
            // Enum type
            JuliaType::Enum(name) => name.clone().into(),
        }
    }

    /// User-facing rendering of the type (Issue #10505): like [`Self::name`],
    /// but trailing UNBOUNDED `where` binders that upstream's `show_can_elide`
    /// (base/show.jl) drops are elided — `Array{T,N} where {T<:Real,N}`
    /// prints `Array{T} where T<:Real`. `name()` deliberately keeps the full
    /// rendering: it is also the string the subtype/isa engines compare
    /// through (`subtype_operand_name`, #10635), where the elided form is NOT
    /// equivalent (a 5-param instance would stop matching a 4-param-displayed
    /// partial UnionAll). Use this ONLY at display boundaries.
    pub fn display_name(&self) -> String {
        match self {
            JuliaType::UnionAll {
                var,
                lower_bound,
                bound,
                body,
            } => format_unionall_name_with(
                var,
                lower_bound.as_deref().map(String::as_str),
                bound.as_deref().map(String::as_str),
                body,
                true,
            ),
            JuliaType::RuntimeUnionAll { .. } => self
                .semantic_alpha_projection()
                .map(|projection| {
                    let projection = projection
                        .canonical_generic_unionall_alias()
                        .unwrap_or(projection);
                    projection.display_name()
                })
                .unwrap_or_else(|| "UnionAll".to_string()),
            _ => self.name().into_owned(),
        }
    }
}

fn format_runtime_unionall_name(ty: &JuliaType) -> String {
    ty.semantic_alpha_projection()
        .map(|projection| {
            projection
                .canonical_generic_unionall_alias()
                .unwrap_or(projection)
                .name()
                .into_owned()
        })
        .unwrap_or_else(|| "UnionAll".to_string())
}

fn format_typevar_interval(name: &str, lower: &JuliaType, upper: &JuliaType) -> String {
    match (
        matches!(lower, JuliaType::Bottom),
        matches!(upper, JuliaType::Any),
    ) {
        (true, true) => name.to_string(),
        (true, false) if name == "_" => format!("<:{}", upper.name()),
        (true, false) => format!("{name}<:{}", upper.name()),
        (false, true) if name == "_" => format!(">:{}", lower.name()),
        (false, true) => format!("{name}>:{}", lower.name()),
        (false, false) => format!("{}<:{name}<:{}", lower.name(), upper.name()),
    }
}

fn two_sided_typevar_bound(bound: &str, name: &str) -> bool {
    let mut parts = bound.split("<:");
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(lower), Some(middle), Some(upper), None)
            if !lower.trim().is_empty()
                && middle.trim() == name
                && !upper.trim().is_empty()
    )
}

fn format_unionall_name_with(
    var: &str,
    lower_bound: Option<&str>,
    bound: Option<&str>,
    body: &JuliaType,
    elide_trailing_unbounded: bool,
) -> String {
    let mut clauses = vec![format_where_clause(var, lower_bound, bound)];
    let mut vars = vec![var.to_string()];
    let mut vars_are_unbounded = vec![lower_bound.is_none() && bound.is_none()];
    let projected_body = matches!(body, JuliaType::RuntimeUnionAll { .. })
        .then(|| body.semantic_alpha_projection())
        .flatten();
    let mut base = projected_body.as_ref().unwrap_or(body);
    while let JuliaType::UnionAll {
        var,
        lower_bound,
        bound,
        body,
    } = base
    {
        vars.push(var.clone());
        vars_are_unbounded.push(lower_bound.is_none() && bound.is_none());
        clauses.push(format_where_clause(
            var,
            lower_bound.as_deref().map(String::as_str),
            bound.as_deref().map(String::as_str),
        ));
        base = body;
    }

    if let Some(partial) = format_left_prefix_partial_unionall(base, &vars, &vars_are_unbounded) {
        return partial;
    }

    // Upstream `show_can_elide` (base/show.jl) drops a TRAILING unbounded
    // binder whose variable is exactly the struct's last parameter and occurs
    // nowhere else — iteratively, innermost first — so
    // `Array{T,N} where {T<:Real, N}` prints `Array{T} where T<:Real`
    // (Issue #10505). The all-unbounded / all-trailing cases are already
    // handled by `format_left_prefix_partial_unionall` above; this handles the
    // MIXED case where bounded binders remain after the elision.
    // DISPLAY-ONLY (Issue #10505): `name()` is also the string the subtype /
    // isa engines compare through (`subtype_operand_name`, #10635), where an
    // elided binder CHANGES semantics (a 5-param instance no longer matches a
    // 4-param-displayed partial UnionAll — the QuadGK BatchIntegrand
    // regression). Elision therefore only runs from `display_name()`.
    if elide_trailing_unbounded {
        if let Some(elided) =
            format_trailing_elided_unionall(base, &vars, &vars_are_unbounded, &clauses)
        {
            return elided;
        }
    }

    if clauses.len() == 1 {
        format!("{} where {}", base.name(), clauses[0])
    } else {
        format!("{} where {{{}}}", base.name(), clauses.join(", "))
    }
}

/// Iteratively elide innermost trailing unbounded `where` binders that appear
/// only as the struct base's LAST parameter (Issue #10505; upstream
/// `show_can_elide`). Returns `None` when nothing can be elided (callers keep
/// the full `where` rendering) — in particular when a candidate variable is
/// mentioned in any other parameter or in a remaining binder's bounds (the
/// #10635 same-name-binder guard, extended to bounds).
fn format_trailing_elided_unionall(
    base: &JuliaType,
    vars: &[String],
    vars_are_unbounded: &[bool],
    clauses: &[String],
) -> Option<String> {
    let JuliaType::Struct(name) = base else {
        return None;
    };
    let (base_name, mut params) = split_struct_parametric_name(name)?;

    let mut vars = vars.to_vec();
    let mut unbounded = vars_are_unbounded.to_vec();
    let mut clauses = clauses.to_vec();
    let mut elided_any = false;
    while let (Some(last_var), Some(last_param)) = (vars.last(), params.last()) {
        if !unbounded.last().copied().unwrap_or(false) || last_param != last_var {
            break;
        }
        let mentioned_elsewhere = params[..params.len() - 1]
            .iter()
            .any(|p| type_name_mentions_typevar(p, last_var))
            || clauses[..clauses.len() - 1]
                .iter()
                .any(|c| type_name_mentions_typevar(c, last_var));
        if mentioned_elsewhere {
            break;
        }
        params.pop();
        vars.pop();
        unbounded.pop();
        clauses.pop();
        elided_any = true;
    }
    if !elided_any || clauses.is_empty() {
        // Fully-elided forms are `format_left_prefix_partial_unionall`'s job
        // (it also validates the exact-distinct-vars precondition); reaching
        // empty here means that precondition failed, so keep full rendering.
        return None;
    }

    let shown_base = if params.is_empty() {
        base_name
    } else {
        format!("{}{{{}}}", base_name, params.join(", "))
    };
    Some(if clauses.len() == 1 {
        format!("{} where {}", shown_base, clauses[0])
    } else {
        format!("{} where {{{}}}", shown_base, clauses.join(", "))
    })
}

fn format_left_prefix_partial_unionall(
    base: &JuliaType,
    vars: &[String],
    vars_are_unbounded: &[bool],
) -> Option<String> {
    let JuliaType::Struct(name) = base else {
        return None;
    };
    let (base_name, params) = split_struct_parametric_name(name)?;
    if vars_are_unbounded.iter().all(|is_unbounded| *is_unbounded)
        && params_are_exact_distinct_where_vars(&params, vars)
    {
        return Some(base_name);
    }
    if params.len() <= vars.len() || params[params.len() - vars.len()..] != *vars {
        return None;
    }
    if !vars_are_unbounded.iter().all(|is_unbounded| *is_unbounded) {
        return None;
    }

    let prefix = &params[..params.len() - vars.len()];
    // A trailing where-bound variable may be elided from the printed parameter
    // list ONLY IF it does not also occur (free) among the retained prefix
    // parameters — upstream `show_can_elide` (base/show.jl) rejects the elision
    // when the variable appears in any other parameter. Without this guard an
    // alpha-renamed same-name binder like `Pair{T, T} where T` wrongly collapses
    // to `Pair{T}` (dropping a real parameter and its binder), and
    // `Pair{Vector{B}, B} where B` collapses to `Pair{Vector{B}}` (Issue #10635).
    // Because this collapsed name is what `<:` stringifies through
    // (`subtype_operand_name`), the collapse also broke structural subtyping of
    // such types. Fall back to the full `where` printing instead.
    if vars
        .iter()
        .any(|v| prefix.iter().any(|p| type_name_mentions_typevar(p, v)))
    {
        return None;
    }

    if prefix.is_empty() {
        Some(base_name)
    } else {
        Some(format!("{}{{{}}}", base_name, prefix.join(", ")))
    }
}

fn params_are_exact_distinct_where_vars(params: &[String], vars: &[String]) -> bool {
    if params.len() != vars.len() {
        return false;
    }
    params.iter().all(|param| vars.contains(param))
        && vars.iter().all(|var| params.contains(var))
        && params.iter().all(|param| {
            params
                .iter()
                .filter(|candidate| *candidate == param)
                .count()
                == 1
        })
}

/// True when `var` occurs as a standalone type-variable token inside the
/// (possibly nested) type-name string `name`. Splits on any non-identifier
/// character so that e.g. the binder `B` is found inside `Vector{B}` but NOT
/// inside `Bool` or `AbstractBar`.
fn type_name_mentions_typevar(name: &str, var: &str) -> bool {
    name.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|token| token == var)
}

fn split_struct_parametric_name(name: &str) -> Option<(String, Vec<String>)> {
    let open = name.find('{')?;
    let close = name.rfind('}')?;
    if close <= open || close + 1 != name.len() {
        return None;
    }

    let base = name[..open].to_string();
    let params = split_top_level_params(&name[open + 1..close])
        .into_iter()
        .map(str::to_string)
        .collect();
    Some((base, params))
}

fn split_top_level_params(params: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in params.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                result.push(params[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    result.push(params[start..].trim());
    result
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
    let bound = strip_typevar_bound(bound);
    JuliaType::from_name_or_struct(bound).name().to_string()
}

fn strip_typevar_bound(bound: &str) -> &str {
    bound
        .find("<:")
        .or_else(|| bound.find(">:"))
        .filter(|&idx| idx > 0)
        .map(|idx| &bound[..idx])
        .filter(|name| name.chars().all(|ch| ch == '_' || ch.is_alphanumeric()))
        .unwrap_or(bound)
}

impl std::fmt::Display for JuliaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_clause_bound_refers_to_outer_typevar_by_name_issue_9721() {
        assert_eq!(normalize_bound("S<:Real"), "S");
        assert_eq!(
            JuliaType::TypeVar("T".to_string(), Some("S<:Real".to_string())).name(),
            "T<:S"
        );
        assert_eq!(
            JuliaType::UnionAll {
                var: "S".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Real".to_string())),
                body: Box::new(JuliaType::UnionAll {
                    var: "T".to_string(),
                    lower_bound: None,
                    bound: Some(Box::new("S<:Real".to_string())),
                    body: Box::new(JuliaType::VectorOf(Box::new(JuliaType::TypeVar(
                        "T".to_string(),
                        None,
                    )))),
                }),
            }
            .name(),
            "Vector{T} where {S<:Real, T<:S}"
        );
    }

    #[test]
    fn two_sided_typevar_bound_keeps_lower_name_and_upper() {
        assert_eq!(
            JuliaType::TypeVar("_".to_string(), Some("Int64<:_<:Real".to_string())).name(),
            "Int64<:_<:Real"
        );
        assert_eq!(
            JuliaType::TypeVar("T".to_string(), Some("Int64<:T<:Real".to_string())).name(),
            "Int64<:T<:Real"
        );
    }

    #[test]
    fn runtime_unionall_alpha_renames_same_name_binders_10613() {
        let outer = JuliaType::RuntimeTypeVar {
            id: 1,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let inner = JuliaType::RuntimeTypeVar {
            id: 2,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(outer.clone()),
        };
        let nested = JuliaType::RuntimeUnionAll {
            var: Box::new(outer.clone()),
            body: Box::new(JuliaType::RuntimeUnionAll {
                var: Box::new(inner.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "Pair".to_string(),
                    params: vec![outer, inner],
                }),
            }),
        };

        assert_eq!(nested.name(), "Pair{T, T1} where {T, T1<:T}");

        let outer = JuliaType::RuntimeTypeVar {
            id: 3,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Real),
        };
        let inner = JuliaType::RuntimeTypeVar {
            id: 4,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(outer.clone()),
        };
        let dependent_only = JuliaType::RuntimeUnionAll {
            var: Box::new(outer),
            body: Box::new(JuliaType::RuntimeUnionAll {
                var: Box::new(inner.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "Vector".to_string(),
                    params: vec![inner],
                }),
            }),
        };
        assert_eq!(dependent_only.name(), "Vector{T} where {T<:Real, T<:T}");
    }

    #[test]
    fn runtime_unionall_generated_alias_avoids_original_names_10613() {
        let first = JuliaType::RuntimeTypeVar {
            id: 1,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Any),
        };
        let second = JuliaType::RuntimeTypeVar {
            id: 2,
            name: "T1".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(first.clone()),
        };
        let third = JuliaType::RuntimeTypeVar {
            id: 3,
            name: "T".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(second.clone()),
        };
        let nested = JuliaType::RuntimeUnionAll {
            var: Box::new(first.clone()),
            body: Box::new(JuliaType::RuntimeUnionAll {
                var: Box::new(second.clone()),
                body: Box::new(JuliaType::RuntimeUnionAll {
                    var: Box::new(third.clone()),
                    body: Box::new(JuliaType::RuntimeParametric {
                        base: "Triple".to_string(),
                        params: vec![first, second, third],
                    }),
                }),
            }),
        };

        assert_eq!(nested.name(), "Triple{T, T1, T2} where {T, T1<:T, T2<:T1}");
    }

    #[test]
    fn mixed_legacy_runtime_unionall_chain_groups_where_clauses_10613() {
        let inner = JuliaType::RuntimeTypeVar {
            id: 11,
            name: "S".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::TypeVar("T".to_string(), None)),
        };
        let mixed = JuliaType::UnionAll {
            var: "T".to_string(),
            lower_bound: None,
            bound: None,
            body: Box::new(JuliaType::RuntimeUnionAll {
                var: Box::new(inner.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "Tuple".to_string(),
                    params: vec![inner],
                }),
            }),
        };

        assert_eq!(mixed.name(), "Tuple{S} where {T, S<:T}");
    }

    #[test]
    fn prefix_partial_unionall_prints_like_upstream_issue_10192() {
        assert_eq!(
            JuliaType::UnionAll {
                var: "B".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("Pair10192{Int64, B}".to_string())),
            }
            .name(),
            "Pair10192{Int64}"
        );

        assert_eq!(
            JuliaType::UnionAll {
                var: "B".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::UnionAll {
                    var: "C".to_string(),
                    lower_bound: None,
                    bound: None,
                    body: Box::new(JuliaType::Struct("Tri10192{Int64, B, C}".to_string())),
                }),
            }
            .name(),
            "Tri10192{Int64}"
        );

        assert_eq!(
            JuliaType::UnionAll {
                var: "A".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("Pair10192{A, Float64}".to_string())),
            }
            .name(),
            "Pair10192{A, Float64} where A"
        );
    }

    #[test]
    fn diagonal_unionall_does_not_print_as_partial_application_issue_10635() {
        assert_eq!(
            JuliaType::UnionAll {
                var: "K".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::UnionAll {
                    var: "V".to_string(),
                    lower_bound: None,
                    bound: None,
                    body: Box::new(JuliaType::Struct("Pair{K, V}".to_string())),
                }),
            }
            .name(),
            "Pair"
        );

        assert_eq!(
            JuliaType::UnionAll {
                var: "K".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::UnionAll {
                    var: "V".to_string(),
                    lower_bound: None,
                    bound: None,
                    body: Box::new(JuliaType::Struct("Pair{V, K}".to_string())),
                }),
            }
            .name(),
            "Pair"
        );

        // `Pair{T, T} where T` must NOT collapse to `Pair{T}`: the trailing
        // binder `T` still occurs in the retained prefix, so upstream keeps the
        // full `where` form.
        assert_eq!(
            JuliaType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("Pair{T, T}".to_string())),
            }
            .name(),
            "Pair{T, T} where T"
        );

        assert_eq!(
            JuliaType::UnionAll {
                var: "A".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Integer".to_string())),
                body: Box::new(JuliaType::UnionAll {
                    var: "B".to_string(),
                    lower_bound: None,
                    bound: Some(Box::new("Real".to_string())),
                    body: Box::new(JuliaType::Struct("Pair{A, B}".to_string())),
                }),
            }
            .name(),
            "Pair{A, B} where {A<:Integer, B<:Real}"
        );

        assert_eq!(
            JuliaType::UnionAll {
                var: "B".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Real".to_string())),
                body: Box::new(JuliaType::Struct("Pair{Int64, B}".to_string())),
            }
            .name(),
            "Pair{Int64, B} where B<:Real"
        );

        // A binder nested inside a prefix parameter also blocks elision:
        // `Pair{Vector{B}, B} where B` stays intact.
        assert_eq!(
            JuliaType::UnionAll {
                var: "B".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("Pair{Vector{B}, B}".to_string())),
            }
            .name(),
            "Pair{Vector{B}, B} where B"
        );

        // A user-defined two-parameter family collapses the same way as `Pair`.
        assert_eq!(
            JuliaType::UnionAll {
                var: "T".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("MyP10635{T, T}".to_string())),
            }
            .name(),
            "MyP10635{T, T} where T"
        );

        // Guard against a false positive: the binder name must match as a whole
        // token, so `Bool` in the prefix does not block eliding binder `B`.
        assert_eq!(
            JuliaType::UnionAll {
                var: "B".to_string(),
                lower_bound: None,
                bound: None,
                body: Box::new(JuliaType::Struct("Pair10635{Bool, B}".to_string())),
            }
            .name(),
            "Pair10635{Bool}"
        );
    }
}
