use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

// =============================================================================
// Registered concrete/abstract type-name registry (Issue #9464)
// =============================================================================
//
// Declared type names are recorded in a VM-local set so late dispatch and
// reflection paths can tell nominal user types from otherwise-opaque strings.
// TypeVar-ness itself is resolved from `where` / type-parameter scope, not from
// this registry or from identifier spelling (Issue #9563).
thread_local! {
    static REGISTERED_TYPE_NAMES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static REGISTERED_QUALIFIED_TYPE_FAMILIES: RefCell<HashMap<String, HashSet<String>>> =
        RefCell::new(HashMap::new());
}

/// Record `name` (a declared struct or abstract type) for nominal user-type
/// lookup in late dispatch/reflection paths (Issue #9464).
///
/// The name is stored by its bare nominal family (module path and `{...}`
/// parameters stripped).
pub fn register_type_name(name: &str) {
    let family = nominal_family_name(name);
    if family.is_empty() {
        return;
    }
    REGISTERED_TYPE_NAMES.with(|reg| {
        reg.borrow_mut().insert(family.to_string());
    });
    let qualified = qualified_family_name(name);
    if qualified.contains('.') {
        REGISTERED_QUALIFIED_TYPE_FAMILIES.with(|reg| {
            let mut reg = reg.borrow_mut();
            let qualified_set = reg.entry(family.to_string()).or_default();
            qualified_set.insert(qualified.to_string());
        });
    }
}

/// Whether `name` is a registered concrete/abstract type name (Issue #9464).
///
/// Whether `name` is a registered concrete/abstract type name (Issue #9464).
pub fn is_registered_type_name(name: &str) -> bool {
    let family = nominal_family_name(name);
    REGISTERED_TYPE_NAMES.with(|reg| reg.borrow().contains(family))
}

/// Whether the current compile/VM context has registered more than one
/// explicitly-qualified declaration with this bare family tail.
///
/// Owner identity is only dispatch-relevant in that collision domain. Keeping
/// every qualification would incorrectly split Base submodule spellings such
/// as `Order.ReverseOrdering` from their historical bare projection (#11076).
pub fn has_qualified_nominal_family_collision(name: &str) -> bool {
    fn contains_exact_family(rendered: &str, qualified_family: &str) -> bool {
        rendered
            .split(|ch: char| {
                ch.is_whitespace()
                    || matches!(
                        ch,
                        '{' | '}' | '(' | ')' | '[' | ']' | ',' | ':' | '<' | '>'
                    )
            })
            .any(|token| token == qualified_family)
    }

    let family = nominal_family_name(name);
    REGISTERED_QUALIFIED_TYPE_FAMILIES.with(|reg| {
        let reg = reg.borrow();
        reg.get(family).is_some_and(|qualified| qualified.len() > 1)
            || reg.values().any(|qualified| {
                qualified.len() > 1
                    && qualified
                        .iter()
                        .any(|qualified_family| contains_exact_family(name, qualified_family))
            })
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructHierarchyEntry {
    parent: Option<String>,
    type_params: Vec<String>,
}

impl StructHierarchyEntry {
    pub fn new(parent: Option<String>, type_params: Vec<String>) -> Self {
        Self {
            parent,
            type_params,
        }
    }

    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    pub fn type_params(&self) -> &[String] {
        &self.type_params
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructHierarchy {
    entries: HashMap<String, StructHierarchyEntry>,
}

impl StructHierarchy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_parent_map(map: &HashMap<String, (Option<String>, Vec<String>)>) -> Self {
        let mut hierarchy = Self::new();
        for (name, (parent, type_params)) in map {
            hierarchy.insert(name, parent.clone(), type_params.clone());
        }
        hierarchy
    }

    pub fn insert(
        &mut self,
        name: impl AsRef<str>,
        parent: Option<String>,
        type_params: Vec<String>,
    ) {
        register_type_name(name.as_ref());
        self.entries.insert(
            qualified_family_name(name.as_ref()).to_string(),
            StructHierarchyEntry::new(parent, type_params),
        );
    }

    pub fn insert_if_absent(
        &mut self,
        name: impl AsRef<str>,
        parent: Option<String>,
        type_params: Vec<String>,
    ) {
        register_type_name(name.as_ref());
        self.entries
            .entry(qualified_family_name(name.as_ref()).to_string())
            .or_insert_with(|| StructHierarchyEntry::new(parent, type_params));
    }

    pub fn entry(&self, name: &str) -> Option<&StructHierarchyEntry> {
        let qualified = qualified_family_name(name);
        self.entries.get(qualified).or_else(|| {
            let bare = nominal_family_name(name);
            unique_nominal_entry(&self.entries, bare)
        })
    }

    pub fn parent_for(&self, name: &str) -> Option<Option<String>> {
        self.entry(name)
            .map(|entry| entry.parent().map(str::to_string))
    }

    pub fn parent_family_for(&self, name: &str) -> Option<Option<String>> {
        self.entry(name).map(|entry| {
            entry
                .parent()
                .map(|parent| qualified_family_name(parent).to_string())
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &StructHierarchyEntry)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry))
    }

    pub fn contains_name(&self, name: &str) -> bool {
        let qualified = qualified_family_name(name);
        let base = nominal_family_name(name);
        self.entries.contains_key(qualified)
            || unique_nominal_entry(&self.entries, base).is_some()
            || self.entries.values().any(|entry| {
                entry.parent().is_some_and(|parent| {
                    qualified_family_name(parent) == qualified
                        || nominal_family_name(parent) == base
                })
            })
    }
}

pub fn qualified_family_name(name: &str) -> &str {
    name.split('{').next().unwrap_or(name)
}

pub fn nominal_family_name(name: &str) -> &str {
    let base = name.rfind('.').map_or(name, |idx| &name[idx + 1..]);
    base.split('{').next().unwrap_or(base)
}

/// Whether two nominal type spellings can denote the same declaration.
///
/// A bare name may be an in-scope alias of a qualified runtime name, but two
/// explicitly-qualified names retain their owners. Collapsing `A.Box` and
/// `B.Box` to the same `Box` tail makes unrelated sibling declarations both
/// applicable during method dispatch (Issue #11076).
pub fn nominal_family_names_compatible(left: &str, right: &str) -> bool {
    let left_family = qualified_family_name(left);
    let right_family = qualified_family_name(right);
    if left_family == right_family {
        return true;
    }
    if left_family.contains('.') && right_family.contains('.') {
        return false;
    }
    if nominal_family_name(left_family) != nominal_family_name(right_family) {
        return false;
    }
    // Exactly one side is qualified here. A bare native array family spelling
    // (`Array`/`Vector`/`Matrix`) denotes the Base-owned wrapper, whose
    // genuine runtime carriers are never module-qualified — so a USER
    // module's same-leaf declaration (`Faux.Array`) is an unrelated nominal
    // type and must not collapse into the family (Issues #11388/#11395).
    // Base/Core/Main qualifiers keep their historical bare-carrier
    // compatibility, as do all other families (import-alias spellings,
    // Issue #8019).
    let qualified = if left_family.contains('.') {
        left_family
    } else {
        right_family
    };
    !native_array_family(nominal_family_name(qualified)) || qualified_owner_is_builtin(qualified)
}

/// The Base-owned array families recognized by the native array-wrapper fast
/// paths. Genuine wrapper carriers spell these bare (`Array{Int64, 1}`);
/// the general per-family owner authority is tracked by Issue #11395.
fn native_array_family(leaf: &str) -> bool {
    matches!(leaf, "Array" | "Vector" | "Matrix")
}

/// Whether a qualified family spelling is owned by Base/Core (optionally
/// reached through `Main.`), e.g. `Base.Array` or `Main.Base.Vector`.
fn qualified_owner_is_builtin(family: &str) -> bool {
    family.rsplit_once('.').is_some_and(|(owner, _)| {
        owner
            .split('.')
            .all(|seg| matches!(seg, "Base" | "Core" | "Main"))
    })
}

/// Whether two spellings are definitely different declarations solely from
/// their explicit owners. Different family tails may still have a declared
/// subtype relation and therefore are not a conflict here.
pub fn explicit_sibling_nominal_family_conflict(left: &str, right: &str) -> bool {
    let left_family = qualified_family_name(left);
    let right_family = qualified_family_name(right);
    left_family.contains('.')
        && right_family.contains('.')
        && left_family != right_family
        && nominal_family_name(left_family) == nominal_family_name(right_family)
}

/// Whether two complete nominal type spellings (family plus parameters) can
/// denote the same type. This is stricter than family compatibility: the
/// parameter suffix must remain identical.
pub fn nominal_type_names_compatible(left: &str, right: &str) -> bool {
    let left_family = qualified_family_name(left);
    let right_family = qualified_family_name(right);
    nominal_family_names_compatible(left_family, right_family)
        && left.strip_prefix(left_family) == right.strip_prefix(right_family)
}

/// Whether a bare nominal parameter owned by Base conflicts with an explicitly
/// external actual type of the same family.
///
/// Cached Base signatures can retain bare names, while user-module values carry
/// their qualified owner. Candidate provenance supplies the missing Base owner;
/// this predicate only detects that one asymmetric case. `Base` and `Core`
/// qualified actuals remain compatible with historical bare cache projections.
pub fn base_bare_nominal_origin_conflict_with<P, O>(
    param: &super::JuliaType,
    actual: &super::JuliaType,
    pattern_matches: &mut P,
    origin_conflicts: &mut O,
) -> bool
where
    P: FnMut(&super::JuliaType, &super::JuliaType) -> bool,
    O: FnMut(&str, &str) -> bool,
{
    use super::JuliaType;

    fn concrete_param_name(ty: &JuliaType) -> Option<&str> {
        match ty {
            JuliaType::Struct(name) | JuliaType::RuntimeParametric { base: name, .. } => Some(name),
            _ => None,
        }
    }

    fn actual_nominal_name(ty: &JuliaType) -> Option<&str> {
        match ty {
            JuliaType::Struct(name)
            | JuliaType::AbstractUser(name, _)
            | JuliaType::RuntimeParametric { base: name, .. } => Some(name),
            _ => None,
        }
    }

    fn direct_conflict<O>(param: &JuliaType, actual: &JuliaType, origin_conflicts: &mut O) -> bool
    where
        O: FnMut(&str, &str) -> bool,
    {
        let (Some(param_name), Some(actual_name)) =
            (concrete_param_name(param), actual_nominal_name(actual))
        else {
            return false;
        };
        let param_family = qualified_family_name(param_name);
        let actual_family = qualified_family_name(actual_name);
        if param_family.contains('.') || !actual_family.contains('.') {
            return false;
        }
        let actual_root = actual_family.split('.').next().unwrap_or(actual_family);
        actual_root != "Base"
            && actual_root != "Core"
            && nominal_family_name(param_family) == nominal_family_name(actual_family)
            && origin_conflicts(param_family, actual_family)
    }

    if direct_conflict(param, actual, origin_conflicts) {
        return true;
    }
    match (param, actual) {
        (JuliaType::TypeOf(param), JuliaType::TypeOf(actual))
        | (JuliaType::VectorOf(param), JuliaType::VectorOf(actual))
        | (JuliaType::MatrixOf(param), JuliaType::MatrixOf(actual)) => {
            base_bare_nominal_origin_conflict_with(param, actual, pattern_matches, origin_conflicts)
        }
        (JuliaType::TupleOf(params), JuliaType::TupleOf(actuals))
            if params.len() == actuals.len() =>
        {
            params.iter().zip(actuals).any(|(param, actual)| {
                base_bare_nominal_origin_conflict_with(
                    param,
                    actual,
                    pattern_matches,
                    origin_conflicts,
                )
            })
        }
        (
            JuliaType::RuntimeParametric {
                params: param_args, ..
            },
            JuliaType::RuntimeParametric {
                params: actual_args,
                ..
            },
        ) if param_args.len() == actual_args.len() => {
            param_args.iter().zip(actual_args).any(|(param, actual)| {
                base_bare_nominal_origin_conflict_with(
                    param,
                    actual,
                    pattern_matches,
                    origin_conflicts,
                )
            })
        }
        (JuliaType::Union(members), actual) => {
            let matching_members: Vec<_> = members
                .iter()
                .filter(|member| pattern_matches(member, actual))
                .collect();
            !matching_members.is_empty()
                && matching_members.iter().all(|member| {
                    base_bare_nominal_origin_conflict_with(
                        member,
                        actual,
                        pattern_matches,
                        origin_conflicts,
                    )
                })
        }
        _ => false,
    }
}

pub fn base_bare_nominal_origin_conflict(
    param: &super::JuliaType,
    actual: &super::JuliaType,
) -> bool {
    let mut pattern_matches = |pattern: &super::JuliaType, actual: &super::JuliaType| match pattern
    {
        super::JuliaType::Any => true,
        super::JuliaType::Union(members) => members.iter().any(|member| {
            qualified_family_name(member.name().as_ref())
                == qualified_family_name(actual.name().as_ref())
        }),
        _ => {
            nominal_family_name(pattern.name().as_ref())
                == nominal_family_name(actual.name().as_ref())
        }
    };
    base_bare_nominal_origin_conflict_with(param, actual, &mut pattern_matches, &mut |_, _| true)
}

fn unique_nominal_entry<'a>(
    entries: &'a HashMap<String, StructHierarchyEntry>,
    bare: &str,
) -> Option<&'a StructHierarchyEntry> {
    let mut matches = entries
        .iter()
        .filter(|(name, _)| nominal_family_name(name) == bare);
    let first = matches.next().map(|(_, entry)| entry)?;
    matches.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JuliaType;

    #[test]
    fn hierarchy_normalizes_keys_and_parent_lookup() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "Main.Box{Int64}",
            Some("AbstractBox{T}".to_string()),
            vec!["T".to_string()],
        );

        let entry = hierarchy.entry("Box{Float64}").unwrap();
        assert_eq!(entry.parent(), Some("AbstractBox{T}"));
        assert_eq!(entry.type_params(), ["T"]);
        assert!(hierarchy.contains_name("Main.Box{String}"));
        assert!(hierarchy.contains_name("AbstractBox{Int64}"));
        assert_eq!(
            hierarchy.parent_family_for("Box{Float64}"),
            Some(Some("AbstractBox".to_string()))
        );
    }

    #[test]
    fn insert_if_absent_preserves_first_declaration() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert(
            "Box",
            Some("AbstractBox{T}".to_string()),
            vec!["T".to_string()],
        );
        hierarchy.insert_if_absent("Main.Box{Int64}", Some("Any".to_string()), Vec::new());

        let entry = hierarchy.entry("Box").unwrap();
        assert_eq!(entry.parent(), Some("AbstractBox{T}"));
        assert_eq!(entry.type_params(), ["T"]);

        let qualified = hierarchy.entry("Main.Box").unwrap();
        assert_eq!(qualified.parent(), Some("Any"));
    }

    #[test]
    fn qualified_names_do_not_collapse_distinct_module_families() {
        let mut hierarchy = StructHierarchy::new();
        hierarchy.insert("Set", Some("AbstractSet".to_string()), Vec::new());
        hierarchy.insert("AbstractAlgebra.Set", None, Vec::new());

        assert_eq!(
            hierarchy.parent_family_for("Set"),
            Some(Some("AbstractSet".to_string()))
        );
        assert_eq!(
            hierarchy.parent_family_for("AbstractAlgebra.Set"),
            Some(None)
        );
        assert!(hierarchy.entry("Set").is_some());
        assert!(hierarchy.entry("AbstractAlgebra.Set").is_some());
        assert!(base_bare_nominal_origin_conflict(
            &JuliaType::Struct("Partition".to_string()),
            &JuliaType::Struct("MyPkg.Partition".to_string())
        ));
        assert!(!base_bare_nominal_origin_conflict(
            &JuliaType::Struct("Partition".to_string()),
            &JuliaType::Struct("Base.Partition".to_string())
        ));
        assert!(!base_bare_nominal_origin_conflict(
            &JuliaType::AbstractUser("AbstractDisplay".to_string(), None),
            &JuliaType::Struct("MyPkg.AbstractDisplay".to_string())
        ));

        let actual = JuliaType::Struct("MyPkg.KeySet".to_string());
        let mut matches = |pattern: &JuliaType, actual: &JuliaType| {
            crate::inference_core::dispatch_resolver::julia_signature_match_with_bindings(
                std::slice::from_ref(pattern),
                std::slice::from_ref(actual),
                &[],
            )
            .is_some()
        };
        let mut different_origin = |_: &str, _: &str| true;
        assert!(base_bare_nominal_origin_conflict_with(
            &JuliaType::Union(vec![
                JuliaType::Struct("KeySet".to_string()),
                JuliaType::Struct("ValueIterator".to_string()),
            ]),
            &actual,
            &mut matches,
            &mut different_origin,
        ));

        let mut matches = |pattern: &JuliaType, actual: &JuliaType| {
            crate::inference_core::dispatch_resolver::julia_signature_match_with_bindings(
                std::slice::from_ref(pattern),
                std::slice::from_ref(actual),
                &[],
            )
            .is_some()
        };
        assert!(!base_bare_nominal_origin_conflict_with(
            &JuliaType::Union(vec![
                JuliaType::Struct("KeySet".to_string()),
                JuliaType::Any,
            ]),
            &actual,
            &mut matches,
            &mut different_origin,
        ));
    }
}
