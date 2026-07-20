use super::{
    builtin_abstract_name_is_subtype_of, nominal_family_name, substitute_typevars, CoreType,
    CoreTypeSubstitution, CoreTypeVar,
};
use crate::types::{qualified_family_name, StructHierarchy, StructHierarchyEntry};

/// Direct declared parent base name of a struct in the supplied hierarchy.
pub(super) fn registered_struct_parent_in(
    hierarchy: &StructHierarchy,
    name: &str,
) -> Option<String> {
    registered_struct_parent_template_in(hierarchy, name)
        .map(|entry| qualified_family_name(&entry.parent).to_string())
}

/// The declared parent template of a struct, wrapped as a standalone
/// existential over the struct type-parameters that occur free in it: e.g.
/// `struct MyVec{T} <: Wrapper{T}` -> `Wrapper{T} where T`.
pub fn registered_struct_parent_existential_in(
    hierarchy: &StructHierarchy,
    name: &str,
) -> Option<String> {
    let entry = registered_struct_parent_template_in(hierarchy, name)?;
    let parent = entry.parent;
    let free: Vec<String> = match parent.find('{') {
        Some(open) if parent.ends_with('}') => {
            let inner = &parent[open + 1..parent.len() - 1];
            let args = split_top_level_commas(inner);
            entry
                .type_params
                .iter()
                .filter(|p| args.contains(&p.as_str()))
                .cloned()
                .collect()
        }
        _ => Vec::new(),
    };
    Some(match free.len() {
        0 => parent,
        1 => format!("{parent} where {}", free[0]),
        _ => format!("{parent} where {{{}}}", free.join(", ")),
    })
}

/// Decide whether a registered struct family reaches `target` through its
/// declared parent chain. Returns `None` when `name` is not registered, so
/// callers can keep their existing fallback for unknown runtime families.
pub fn registered_struct_parent_family_decision_in(
    hierarchy: &StructHierarchy,
    name: &str,
    target: &str,
) -> Option<bool> {
    let target = qualified_family_name(target);
    let mut current = qualified_family_name(name).to_string();

    registered_struct_parent_in(hierarchy, &current)?;

    for _ in 0..32 {
        let Some(parent) = registered_struct_parent_in(hierarchy, &current) else {
            return Some(false);
        };
        if type_family_matches(&parent, target)
            || builtin_abstract_family_name_is_subtype_of(&parent, target)
        {
            return Some(true);
        }
        current = parent;
    }

    Some(false)
}

/// Authoritative nominal subtype decision for user-defined names registered by
/// the compiler. Returns `None` when either side is unknown so VM callers can
/// fall back to their local hierarchy maps instead of turning missing registry
/// data into a negative subtype result.
pub fn registered_nominal_subtype_decision_in(
    hierarchy: &StructHierarchy,
    name: &str,
    target: &str,
) -> Option<bool> {
    if name == target || registered_struct_is_subtype_of_in(hierarchy, name, target) {
        return Some(true);
    }

    (registered_type_name_known_in(hierarchy, name)
        && registered_type_name_known_in(hierarchy, target))
    .then_some(false)
}

/// Whether `name`'s declared-supertype chain (struct parents, then the built-in
/// abstract hierarchy) reaches the built-in abstract `target` (Issue #5157).
pub(super) fn registered_struct_is_subtype_of_in(
    hierarchy: &StructHierarchy,
    name: &str,
    target: &str,
) -> bool {
    // This is a nominal family walk: declared parents are stored as family
    // names, and the target may now carry value/type parameters
    // (`AbsM{2,2,T}`, Issue #7960), so reduce it to its family name before
    // comparing. The value parameters are enforced separately by the dispatch
    // matcher (`abstract_value_param_match`).
    let target = qualified_family_name(target);
    let mut current = qualified_family_name(name).to_string();
    for _ in 0..64 {
        let Some(parent) = registered_struct_parent_in(hierarchy, &current) else {
            return false;
        };
        if type_family_matches(&parent, target) {
            return true;
        }
        // Once the chain reaches a built-in abstract (no further struct parent),
        // continue up the built-in abstract hierarchy (e.g. `Real <: Number`).
        if registered_struct_parent_in(hierarchy, &parent).is_none() {
            return builtin_abstract_family_name_is_subtype_of(&parent, target);
        }
        current = parent;
    }
    false
}

/// Walk `name{params}` up its declared parent chain, substituting the actual
/// parameters into each parent template at every step, until it reaches a type
/// whose family is `target_family`, returning that ancestor's *instantiated*
/// `CoreType::Struct` (e.g. `ConM{2,2,Float64}` -> `AbsM{2,2,Float64}` for
/// `struct ConM{M,N,T} <: AbsM{M,N,T}`). This is the value-parameter-preserving
/// supertype projection dispatch needs to compare a concrete subtype's value
/// parameters against a parametric abstract supertype pattern (Issue #7960).
///
/// Returns `None` when `target_family` is unreachable, the chain leaves the
/// `Struct` shape (e.g. reaches a built-in abstract), or the depth guard trips.
pub fn registered_instantiated_struct_supertype_in(
    hierarchy: &StructHierarchy,
    name: &str,
    params: &[CoreType],
    target_family: &str,
) -> Option<CoreType> {
    let target_family = qualified_family_name(target_family);
    let mut current = CoreType::Struct {
        name: qualified_family_name(name).to_string(),
        params: params.to_vec(),
    };
    for _ in 0..64 {
        let CoreType::Struct {
            name: cur_name,
            params: cur_params,
        } = &current
        else {
            return None;
        };
        if type_family_matches(cur_name, target_family) {
            return Some(current);
        }
        current = registered_instantiated_struct_parent_in(hierarchy, cur_name, cur_params)?;
    }
    None
}

pub(super) fn registered_instantiated_struct_parent_in(
    hierarchy: &StructHierarchy,
    name: &str,
    params: &[CoreType],
) -> Option<CoreType> {
    let entry = registered_struct_parent_template_in(hierarchy, name)?;
    let parent = CoreType::from_julia_name(&entry.parent);
    if entry.type_params.is_empty() || params.is_empty() {
        return Some(parent);
    }

    let substitutions = entry
        .type_params
        .iter()
        .zip(params.iter())
        .map(|(param, actual)| {
            CoreTypeSubstitution::new(CoreTypeVar::unscoped(param.clone()), actual.clone())
        })
        .collect::<Vec<_>>();
    Some(substitute_typevars(&parent, &substitutions))
}

fn registered_struct_parent_template_in(
    hierarchy: &StructHierarchy,
    name: &str,
) -> Option<RegisteredStructParent> {
    hierarchy.entry(name).and_then(registered_parent_from_entry)
}

fn registered_type_name_known_in(hierarchy: &StructHierarchy, name: &str) -> bool {
    hierarchy.contains_name(name)
}

#[derive(Clone, Debug)]
struct RegisteredStructParent {
    parent: String,
    type_params: Vec<String>,
}

fn registered_parent_from_entry(entry: &StructHierarchyEntry) -> Option<RegisteredStructParent> {
    entry.parent().map(|parent| RegisteredStructParent {
        parent: parent.to_string(),
        type_params: entry.type_params().to_vec(),
    })
}

fn type_family_matches(candidate: &str, target: &str) -> bool {
    candidate == target || (!target.contains('.') && nominal_family_name(candidate) == target)
}

fn builtin_abstract_family_name_is_subtype_of(candidate: &str, target: &str) -> bool {
    !candidate.contains('.') && builtin_abstract_name_is_subtype_of(candidate, target)
}

/// Split a comma-separated parametric argument list, respecting `{...}` nesting
/// (so `Pair{A,B}, C` yields `["Pair{A,B}", "C"]`).
fn split_top_level_commas(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() || !parts.is_empty() {
        parts.push(tail);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_hierarchy_helpers_match_registered_parent_semantics() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Mammal", Some("Animal".to_string()), Vec::new());
        hierarchy.insert("Dog", Some("Mammal".to_string()), Vec::new());
        hierarchy.insert(
            "Pairs",
            Some("AbstractDict{K,V}".to_string()),
            vec![
                "K".to_string(),
                "V".to_string(),
                "I".to_string(),
                "A".to_string(),
            ],
        );

        assert_eq!(
            registered_struct_parent_in(&hierarchy, "Dog"),
            Some("Mammal".to_string())
        );
        assert!(registered_struct_is_subtype_of_in(
            &hierarchy, "Dog", "Animal"
        ));
        assert_eq!(
            registered_struct_parent_family_decision_in(&hierarchy, "Pairs", "AbstractDict"),
            Some(true)
        );
        assert_eq!(
            registered_struct_parent_existential_in(&hierarchy, "Pairs"),
            Some("AbstractDict{K,V} where {K, V}".to_string())
        );
        assert_eq!(
            registered_nominal_subtype_decision_in(&hierarchy, "Rock", "Animal"),
            None
        );
    }

    #[test]
    fn qualified_module_abstract_family_does_not_inherit_same_named_base_family_issue_8858() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("AbstractSet", Some("Any".to_string()), Vec::new());
        hierarchy.insert("Set", Some("AbstractSet".to_string()), Vec::new());
        hierarchy.insert("AbstractAlgebra.Set", None, Vec::new());
        hierarchy.insert(
            "AbstractAlgebra.NCRing",
            Some("AbstractAlgebra.Set".to_string()),
            Vec::new(),
        );
        hierarchy.insert(
            "AbstractAlgebra.Ring",
            Some("AbstractAlgebra.NCRing".to_string()),
            Vec::new(),
        );
        hierarchy.insert(
            "AbstractAlgebra.Integers",
            Some("AbstractAlgebra.Ring".to_string()),
            vec!["T".to_string()],
        );

        assert!(registered_struct_is_subtype_of_in(
            &hierarchy,
            "AbstractAlgebra.Integers{BigInt}",
            "Ring"
        ));
        assert!(!registered_struct_is_subtype_of_in(
            &hierarchy,
            "AbstractAlgebra.Integers{BigInt}",
            "AbstractSet"
        ));

        let actual = CoreType::from_julia_name("AbstractAlgebra.Integers{BigInt}");
        assert!(actual.is_subtype_of_with_hierarchy(&CoreType::from_julia_name("Ring"), &hierarchy));
        assert!(!actual
            .is_subtype_of_with_hierarchy(&CoreType::from_julia_name("AbstractSet"), &hierarchy));
    }

    /// A parametric struct family whose declared parent threads its type
    /// parameters through a value-parameter intermediate
    /// (`StaticVector{N,T} <: StaticVecOrMat{Tuple{N},T,1}`) must keep preserving
    /// the concrete element/rank parameters all the way up to the built-in
    /// `AbstractArray{T,N}` ancestor, so `SVector{3,Int64} <:
    /// AbstractArray{Int64,1}` is true (Issue #7728 / #7819). This guards the
    /// substitution chain `registered_instantiated_struct_parent_in` walks.
    #[test]
    fn parametric_value_param_intermediate_preserves_abstractarray_edge() {
        use crate::inference_core::CoreType;
        let mut h = StructHierarchy::new();
        h.insert(
            "SVector",
            Some("StaticVector{N,T}".to_string()),
            vec!["N".to_string(), "T".to_string()],
        );
        h.insert(
            "StaticVector",
            Some("StaticVecOrMat{Tuple{N},T,1}".to_string()),
            vec!["N".to_string(), "T".to_string()],
        );
        h.insert(
            "StaticVecOrMat",
            Some("StaticArray{S,T,N}".to_string()),
            vec!["S".to_string(), "T".to_string(), "N".to_string()],
        );
        h.insert(
            "StaticArray",
            Some("AbstractArray{T,N}".to_string()),
            vec!["S".to_string(), "T".to_string(), "N".to_string()],
        );

        let svec_params = vec![
            CoreType::from_julia_name("3"),
            CoreType::from_julia_name("Int64"),
        ];
        let lhs = CoreType::Struct {
            name: "SVector".to_string(),
            params: svec_params,
        };
        assert!(lhs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractArray{Int64,1}"),
            &h
        ));
        assert!(lhs
            .is_subtype_of_with_hierarchy(&CoreType::from_julia_name("StaticVector{3,Int64}"), &h));
        // Element type is invariant: a different element type must NOT match.
        assert!(!lhs.is_subtype_of_with_hierarchy(
            &CoreType::from_julia_name("AbstractArray{Float64,1}"),
            &h
        ));
    }
}
