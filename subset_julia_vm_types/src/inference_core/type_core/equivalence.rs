use super::{base_type_name, CoreType, CoreTypeVar};
use crate::types::struct_owners_compatible;

#[derive(Clone, Copy)]
enum NominalOwnerMode {
    ShortName,
    Compatible,
    Exact,
}

impl CoreType {
    /// Structural type equality with alpha-renaming for `UnionAll` binders.
    ///
    /// Unlike mutual subtyping, this preserves an explicit `UnionAll` wrapper.
    /// Free runtime `TypeVar`s remain rigid and compare by object identity.
    pub fn is_semantically_equal(&self, other: &Self) -> bool {
        semantically_equal(self, other, &mut Vec::new(), NominalOwnerMode::ShortName)
    }

    /// Alpha-equivalent structural equality that accepts a bare/qualified
    /// spelling pair while rejecting two distinct qualified owners.
    pub fn is_semantically_equal_with_compatible_nominals(&self, other: &Self) -> bool {
        semantically_equal(self, other, &mut Vec::new(), NominalOwnerMode::Compatible)
    }

    /// Alpha-equivalent structural equality that preserves fully-qualified
    /// nominal owners. Constructor method identity uses this mode because
    /// `A.Bound` and `B.Bound` are distinct constraints even though ordinary
    /// module-insensitive type matching compares their short names (Issue
    /// #11019).
    pub fn is_semantically_equal_with_qualified_nominals(&self, other: &Self) -> bool {
        semantically_equal(self, other, &mut Vec::new(), NominalOwnerMode::Exact)
    }
}

fn semantically_equal(
    left: &CoreType,
    right: &CoreType,
    binders: &mut Vec<(CoreTypeVar, CoreTypeVar)>,
    nominal_owner_mode: NominalOwnerMode,
) -> bool {
    let left_binder_leaf = binder_leaf_var(left);
    let right_binder_leaf = binder_leaf_var(right);
    if let Some(left_var) = left_binder_leaf {
        if let Some((_, expected_right)) = binders
            .iter()
            .rev()
            .find(|(bound_left, _)| binder_reference_matches(left_var, bound_left))
        {
            return right_binder_leaf
                .is_some_and(|right_var| binder_reference_matches(right_var, expected_right));
        }
    }
    if right_binder_leaf.is_some_and(|right_var| {
        binders
            .iter()
            .rev()
            .any(|(_, bound_right)| binder_reference_matches(right_var, bound_right))
    }) {
        return false;
    }
    match (left, right) {
        (CoreType::Union(left), CoreType::Union(right)) => {
            if left.len() != right.len() {
                return false;
            }
            let mut matched = vec![false; right.len()];
            left.iter().all(|left_member| {
                let Some(index) = right.iter().enumerate().find_map(|(index, right_member)| {
                    (!matched[index]
                        && semantically_equal(
                            left_member,
                            right_member,
                            binders,
                            nominal_owner_mode,
                        ))
                    .then_some(index)
                }) else {
                    return false;
                };
                matched[index] = true;
                true
            })
        }
        (CoreType::TypeVar(left), CoreType::TypeVar(right)) => {
            typevars_equal(left, right, binders, nominal_owner_mode)
        }
        (
            CoreType::UnionAll {
                var: left_var,
                body: left_body,
            },
            CoreType::UnionAll {
                var: right_var,
                body: right_body,
            },
        ) => {
            binders.push((left_var.clone(), right_var.clone()));
            let equal = optional_types_equal(
                left_var.lower_bound.as_deref(),
                right_var.lower_bound.as_deref(),
                binders,
                nominal_owner_mode,
            ) && optional_types_equal(
                left_var.upper_bound.as_deref(),
                right_var.upper_bound.as_deref(),
                binders,
                nominal_owner_mode,
            ) && semantically_equal(left_body, right_body, binders, nominal_owner_mode);
            binders.pop();
            equal
        }
        (
            CoreType::AbstractUser {
                name: left_name,
                parent: left_parent,
            },
            CoreType::AbstractUser {
                name: right_name,
                parent: right_parent,
            },
        ) => {
            left_name == right_name
                && optional_types_equal(
                    left_parent.as_deref(),
                    right_parent.as_deref(),
                    binders,
                    nominal_owner_mode,
                )
        }
        (
            CoreType::Struct {
                name: left_name,
                params: left_params,
            },
            CoreType::Struct {
                name: right_name,
                params: right_params,
            },
        ) => {
            (match nominal_owner_mode {
                NominalOwnerMode::ShortName => {
                    base_type_name(left_name) == base_type_name(right_name)
                }
                NominalOwnerMode::Compatible => {
                    base_type_name(left_name) == base_type_name(right_name)
                        && struct_owners_compatible(left_name, right_name)
                }
                NominalOwnerMode::Exact => left_name == right_name,
            }) && slices_equal(left_params, right_params, binders, nominal_owner_mode)
        }
        (CoreType::Tuple(left), CoreType::Tuple(right)) => {
            slices_equal(left, right, binders, nominal_owner_mode)
        }
        (CoreType::Vararg(left), CoreType::Vararg(right))
        | (CoreType::TypeOf(left), CoreType::TypeOf(right)) => {
            semantically_equal(left, right, binders, nominal_owner_mode)
        }
        (
            CoreType::VarargLen {
                element: left_element,
                len: left_len,
            },
            CoreType::VarargLen {
                element: right_element,
                len: right_len,
            },
        ) => {
            semantically_equal(left_element, right_element, binders, nominal_owner_mode)
                && semantically_equal(left_len, right_len, binders, nominal_owner_mode)
        }
        (CoreType::NamedTuple(left), CoreType::NamedTuple(right)) => {
            left.len() == right.len()
                && left.iter().zip(right).all(
                    |((left_name, left_type), (right_name, right_type))| {
                        left_name == right_name
                            && semantically_equal(
                                left_type,
                                right_type,
                                binders,
                                nominal_owner_mode,
                            )
                    },
                )
        }
        _ => left == right,
    }
}

fn binder_leaf_var(ty: &CoreType) -> Option<&CoreTypeVar> {
    match ty {
        CoreType::TypeVar(var) if !var.is_rigid() => Some(var),
        _ => None,
    }
}

fn binder_reference_matches(reference: &CoreTypeVar, binder: &CoreTypeVar) -> bool {
    if reference.is_rigid() || binder.is_rigid() {
        return false;
    }
    match (reference.scope_id, binder.scope_id) {
        (left, right)
            if left != CoreTypeVar::UNRESOLVED_SCOPE_ID
                && right != CoreTypeVar::UNRESOLVED_SCOPE_ID =>
        {
            left == right
        }
        _ => reference.name == binder.name,
    }
}

fn typevars_equal(
    left: &CoreTypeVar,
    right: &CoreTypeVar,
    binders: &mut Vec<(CoreTypeVar, CoreTypeVar)>,
    nominal_owner_mode: NominalOwnerMode,
) -> bool {
    match (left.rigid_identity, right.rigid_identity) {
        (Some(left_id), Some(right_id)) => left_id == right_id,
        (Some(_), None) | (None, Some(_)) => false,
        (None, None) => {
            if let Some((_, expected_right)) = binders
                .iter()
                .rev()
                .find(|(bound_left, _)| binder_reference_matches(left, bound_left))
            {
                return binder_reference_matches(right, expected_right);
            }
            if binders
                .iter()
                .rev()
                .any(|(_, bound_right)| binder_reference_matches(right, bound_right))
            {
                return false;
            }
            left.name == right.name
                && optional_types_equal(
                    left.lower_bound.as_deref(),
                    right.lower_bound.as_deref(),
                    binders,
                    nominal_owner_mode,
                )
                && optional_types_equal(
                    left.upper_bound.as_deref(),
                    right.upper_bound.as_deref(),
                    binders,
                    nominal_owner_mode,
                )
        }
    }
}

fn optional_types_equal(
    left: Option<&CoreType>,
    right: Option<&CoreType>,
    binders: &mut Vec<(CoreTypeVar, CoreTypeVar)>,
    nominal_owner_mode: NominalOwnerMode,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => semantically_equal(left, right, binders, nominal_owner_mode),
        (None, None) => true,
        _ => false,
    }
}

fn slices_equal(
    left: &[CoreType],
    right: &[CoreType],
    binders: &mut Vec<(CoreTypeVar, CoreTypeVar)>,
    nominal_owner_mode: NominalOwnerMode,
) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| semantically_equal(left, right, binders, nominal_owner_mode))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(name: &str) -> CoreTypeVar {
        CoreTypeVar::unscoped(name)
    }

    fn wrapper(name: &str) -> CoreType {
        CoreType::UnionAll {
            var: var(name),
            body: Box::new(CoreType::Struct {
                name: "Box".to_string(),
                params: vec![CoreType::TypeVar(var(name))],
            }),
        }
    }

    #[test]
    fn alpha_equivalent_unionalls_are_equal() {
        assert!(wrapper("T").is_semantically_equal(&wrapper("S")));
    }

    #[test]
    fn alpha_equivalence_resolves_shadowed_binder_bounds_by_scope_10460() {
        let left_outer = var("T").with_scope_id(1);
        let left_inner = CoreTypeVar::with_bounds(
            "T",
            None,
            Some(Box::new(CoreType::TypeVar(left_outer.clone()))),
        )
        .with_scope_id(2);
        let left = CoreType::UnionAll {
            var: left_outer,
            body: Box::new(CoreType::UnionAll {
                var: left_inner.clone(),
                body: Box::new(CoreType::Tuple(vec![CoreType::TypeVar(left_inner)])),
            }),
        };

        let right_outer = var("S").with_scope_id(3);
        let right_inner = CoreTypeVar::with_bounds(
            "T",
            None,
            Some(Box::new(CoreType::TypeVar(right_outer.clone()))),
        )
        .with_scope_id(4);
        let right = CoreType::UnionAll {
            var: right_outer,
            body: Box::new(CoreType::UnionAll {
                var: right_inner.clone(),
                body: Box::new(CoreType::Tuple(vec![CoreType::TypeVar(right_inner)])),
            }),
        };

        assert!(left.is_semantically_equal(&right));
    }

    #[test]
    fn runtime_and_source_dependent_unionalls_are_alpha_equivalent_10091() {
        use crate::types::JuliaType;

        let runtime_b = JuliaType::RuntimeTypeVar {
            id: 1,
            name: "B".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(JuliaType::Number),
        };
        let runtime_c = JuliaType::RuntimeTypeVar {
            id: 2,
            name: "C".to_string(),
            lower_bound: Box::new(JuliaType::Bottom),
            upper_bound: Box::new(runtime_b.clone()),
        };
        let runtime = JuliaType::RuntimeUnionAll {
            var: Box::new(runtime_b.clone()),
            body: Box::new(JuliaType::RuntimeUnionAll {
                var: Box::new(runtime_c.clone()),
                body: Box::new(JuliaType::RuntimeParametric {
                    base: "DepTriple10091".to_string(),
                    params: vec![JuliaType::Number, runtime_b, runtime_c],
                }),
            }),
        };
        let source = JuliaType::UnionAll {
            var: "Y".to_string(),
            lower_bound: None,
            bound: Some(Box::new("Number".to_string())),
            body: Box::new(JuliaType::UnionAll {
                var: "Z".to_string(),
                lower_bound: None,
                bound: Some(Box::new("Y".to_string())),
                body: Box::new(JuliaType::Struct("DepTriple10091{Number,Y,Z}".to_string())),
            }),
        };
        let runtime_core = CoreType::from_julia_type_preserving_owner(&runtime);
        let source_core = CoreType::from_julia_type_preserving_owner(&source);

        assert!(
            runtime_core.is_semantically_equal(&source_core),
            "runtime={runtime_core:#?}\nsource={source_core:#?}"
        );
    }

    #[test]
    fn qualified_nominal_equality_mode_preserves_module_owner_11019() {
        let left = CoreType::Struct {
            name: "A.Bound".to_string(),
            params: vec![],
        };
        let right = CoreType::Struct {
            name: "B.Bound".to_string(),
            params: vec![],
        };

        assert!(left.is_semantically_equal(&right));
        assert!(!left.is_semantically_equal_with_compatible_nominals(&right));
        assert!(!left.is_semantically_equal_with_qualified_nominals(&right));

        let bare = CoreType::Struct {
            name: "Bound".to_string(),
            params: vec![],
        };
        assert!(left.is_semantically_equal_with_compatible_nominals(&bare));
    }

    #[test]
    fn unionall_wrapper_is_not_equal_to_its_body() {
        let wrapped = wrapper("T");
        if let CoreType::UnionAll { body, .. } = &wrapped {
            assert!(!wrapped.is_semantically_equal(body));
        } else {
            assert!(matches!(wrapped, CoreType::UnionAll { .. }));
        }
    }

    #[test]
    fn free_rigid_typevars_compare_by_identity() {
        let left = CoreType::TypeVar(var("T").with_rigid_identity(1));
        let same = CoreType::TypeVar(var("Q").with_rigid_identity(1));
        let distinct = CoreType::TypeVar(var("T").with_rigid_identity(2));
        assert!(left.is_semantically_equal(&same));
        assert!(!left.is_semantically_equal(&distinct));
    }

    #[test]
    fn free_rigid_typevar_never_equals_same_named_nominal_leaf_10613() {
        for (name, nominal) in [
            (
                "Int64",
                CoreType::Primitive(crate::inference_core::CorePrimitive::Int64),
            ),
            (
                "Real",
                CoreType::Abstract(crate::inference_core::CoreAbstract::Real),
            ),
            (
                "String",
                CoreType::Primitive(crate::inference_core::CorePrimitive::String),
            ),
            ("Module", CoreType::Module("Module".to_string())),
        ] {
            let runtime = CoreType::TypeVar(var(name).with_rigid_identity(7));
            assert!(!nominal.is_semantically_equal(&runtime), "{name}");
            assert!(!runtime.is_semantically_equal(&nominal), "{name}");
        }
    }
}
