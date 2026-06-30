use super::{
    abstract_is_subtype_of, base_type_name, core_type_inner_is_datatype, core_typeof_inner_subtype,
    core_typeof_inner_subtype_in, is_universal_vararg_tuple, named_tuple_fields_match,
    named_tuple_fields_match_in, named_tuple_marker_params_match, primitive_is_subtype_of_abstract,
    registered_struct_is_subtype_of_in, resolve_named_word_alias, struct_is_subtype_of_abstract,
    struct_is_subtype_of_abstract_in, struct_params_are_subtype, struct_params_are_subtype_in,
    substitute_typevar_bound, tuple_elements_match, tuple_elements_match_in, tuple_params_match,
    tuple_params_match_in, CoreAbstract, CoreType,
};
use crate::types::StructHierarchy;

impl CoreType {
    /// Julia-style subtype relation for the structured subset represented by
    /// `CoreType`.
    ///
    /// This deliberately covers the semantic cases needed before method
    /// dispatch can migrate away from local scoring: primitive/abstract
    /// hierarchy, unions, tuple covariance, `Type{T}`, and invariant
    /// parametric structs. Unknown `Named` values only match exactly.
    pub fn is_subtype_of(&self, other: &Self) -> bool {
        self.is_subtype_of_with_lookup(other, None)
    }

    pub fn is_subtype_of_with_hierarchy(&self, other: &Self, hierarchy: &StructHierarchy) -> bool {
        self.is_subtype_of_with_lookup(other, Some(hierarchy))
    }

    fn is_subtype_of_with_lookup(&self, other: &Self, hierarchy: Option<&StructHierarchy>) -> bool {
        if self == other {
            return true;
        }
        if matches!(self, Self::Bottom) || matches!(other, Self::Any) {
            return true;
        }
        if matches!(self, Self::Any) || matches!(other, Self::Bottom) {
            return false;
        }

        // Resolve the 64-bit word aliases `Int`/`UInt` that can survive as an
        // opaque `Named` when they appear nested inside a rendered parametric
        // type name. Treating them as their concrete primitive lets bound checks
        // such as `Box{Box{Int}} <: (Box{Box{T}} where T<:Integer)` agree with
        // upstream while leaving type-propagation and dispatch representations
        // unchanged (Issue #5047).
        if let Some(resolved) = resolve_named_word_alias(self) {
            return resolved.is_subtype_of_with_lookup(other, hierarchy);
        }
        if let Some(resolved) = resolve_named_word_alias(other) {
            return self.is_subtype_of_with_lookup(&resolved, hierarchy);
        }

        match (self, other) {
            (Self::Union(types), _) => types
                .iter()
                .all(|t| t.is_subtype_of_with_lookup(other, hierarchy)),
            (_, Self::Union(types)) => types
                .iter()
                .any(|t| self.is_subtype_of_with_lookup(t, hierarchy)),
            (Self::Primitive(p), Self::Abstract(a)) => primitive_is_subtype_of_abstract(p, a),
            (Self::Abstract(a), Self::Abstract(b)) => abstract_is_subtype_of(a, b),
            (
                Self::AbstractUser { name, parent },
                Self::AbstractUser {
                    name: other_name, ..
                },
            ) => {
                name == other_name
                    // A value-parameterized abstract is a subtype of its bare
                    // family (`AbsM{2,2,T} <: AbsM`), but NOT the reverse — this
                    // asymmetry lets the dispatch dominance check rank the
                    // `AbsM{2,2,T}` specialization above the generic `AbsM`
                    // method (Issue #7960). Only the value-parameter spelling
                    // carries `{...}` in an `AbstractUser` name (type-only
                    // parametric abstracts keep their bare family name), so this
                    // never fires for the historical bare-family forms.
                    || (base_type_name(name) == base_type_name(other_name)
                        && !other_name.contains('{'))
                    || parent
                        .as_deref()
                        .is_some_and(|p| p.is_subtype_of_with_lookup(other, hierarchy))
            }
            (
                Self::Struct { name, params },
                Self::Struct {
                    name: on,
                    params: op,
                },
            ) => match hierarchy {
                Some(hierarchy) => struct_params_are_subtype_in(hierarchy, name, params, on, op),
                None => struct_params_are_subtype(name, params, on, op),
            },
            (Self::Struct { name, params }, Self::Abstract(a)) => match hierarchy {
                Some(hierarchy) => struct_is_subtype_of_abstract_in(hierarchy, name, params, a),
                None => struct_is_subtype_of_abstract(name, params, a),
            },
            (Self::Tuple(_), Self::Struct { name, params }) if name == "Tuple" => {
                params.is_empty()
                    || match hierarchy {
                        Some(hierarchy) => tuple_params_match_in(hierarchy, self, params),
                        None => tuple_params_match(self, params),
                    }
            }
            // The bare `Tuple` datatype is definitionally `Tuple{Vararg{Any}}`
            // upstream (`Tuple === Tuple{Vararg{Any}}`), so it is a subtype of
            // the universal vararg tuple `Tuple{Vararg{Any}}` but not of any
            // narrower vararg element type or fixed-arity tuple (Issue #5061).
            (Self::Struct { name, params }, Self::Tuple(other_elements))
                if name == "Tuple" && params.is_empty() =>
            {
                is_universal_vararg_tuple(other_elements)
            }
            (Self::Tuple(elements), Self::Tuple(other_elements)) => match hierarchy {
                Some(hierarchy) => tuple_elements_match_in(hierarchy, elements, other_elements),
                None => tuple_elements_match(elements, other_elements),
            },
            (Self::NamedTuple(fields), Self::NamedTuple(other_fields)) => match hierarchy {
                Some(hierarchy) => named_tuple_fields_match_in(hierarchy, fields, other_fields),
                None => named_tuple_fields_match(fields, other_fields),
            },
            (Self::NamedTuple(fields), Self::Struct { name, params }) if name == "NamedTuple" => {
                named_tuple_marker_params_match(fields, params)
            }
            (Self::Vararg(inner), Self::Vararg(other_inner)) => {
                inner.is_subtype_of_with_lookup(other_inner, hierarchy)
            }
            (Self::TypeOf(inner), Self::TypeOf(other_inner)) => match hierarchy {
                Some(hierarchy) => core_typeof_inner_subtype_in(hierarchy, inner, other_inner),
                None => core_typeof_inner_subtype(inner, other_inner),
            },
            (Self::TypeOf(_), Self::Abstract(CoreAbstract::Type)) => true,
            // `Type{T} <: DataType` exactly when `T` is itself a nominal
            // DataType: a concrete or abstract type, or a fully-applied
            // parametric type. Union, bare parametric, and `Type{<:Bound}` stay
            // false (Issue #5048).
            (Self::TypeOf(inner), Self::Abstract(CoreAbstract::DataType)) => {
                core_type_inner_is_datatype(inner)
            }
            (Self::TypeVar(var), _) => var
                .upper_bound
                .as_deref()
                .is_some_and(|ub| ub.is_subtype_of_with_lookup(other, hierarchy)),
            (Self::Value(_), _) => false,
            (_, Self::TypeVar(var)) => var
                .upper_bound
                .as_deref()
                .is_none_or(|ub| self.is_subtype_of_with_lookup(ub, hierarchy)),
            // Forall-left: decide `(B where V...) <: C` by introducing a fresh
            // rigid variable for the bound var and checking `B[rigid] <: C` for
            // all choices within the declared bounds (Issue #5047).
            (Self::UnionAll { var, body }, _) => {
                substitute_typevar_bound(body, var).is_subtype_of_with_lookup(other, hierarchy)
            }
            (_, Self::UnionAll { .. }) => match hierarchy {
                Some(hierarchy) => self.matches_unionall_pattern_with_hierarchy(other, hierarchy),
                None => self.matches_unionall_pattern(other),
            },
            (Self::Named(name), Self::Module(module_name)) => name == module_name,
            (Self::Module(module_name), Self::Named(name)) => module_name == name,
            (Self::Module(_), Self::Module(_)) => false,
            // Built-in abstract aliases can arrive as `AbstractUser` through
            // method signatures. Resolve those aliases to canonical built-in
            // abstracts before the generic user-abstract arms below (Issue #5926).
            (_, Self::AbstractUser { name, .. })
                if matches!(Self::from_julia_name(name), Self::Abstract(_)) =>
            {
                self.is_subtype_of_with_lookup(&Self::from_julia_name(name), hierarchy)
            }
            (Self::Struct { name, .. }, Self::AbstractUser { name: parent, .. }) => {
                base_type_name(name) == base_type_name(parent)
                    || match hierarchy {
                        Some(hierarchy) => {
                            registered_struct_is_subtype_of_in(hierarchy, name, parent)
                        }
                        None => false,
                    }
            }
            (Self::Named(child), Self::AbstractUser { name: parent, .. }) => {
                base_type_name(child) == base_type_name(parent)
                    || match hierarchy {
                        Some(hierarchy) => {
                            registered_struct_is_subtype_of_in(hierarchy, child, parent)
                        }
                        None => false,
                    }
            }
            // User-defined structs and abstract types that are not built-in
            // families both lower to `Named`. Resolve the relation through the
            // supplied struct hierarchy so bounded typevar methods match
            // declared subtypes (Issue #5383).
            (Self::Struct { name, .. }, Self::Named(parent)) => {
                base_type_name(name) == base_type_name(parent)
                    || match hierarchy {
                        Some(hierarchy) => {
                            registered_struct_is_subtype_of_in(hierarchy, name, parent)
                        }
                        None => false,
                    }
            }
            // Two user/package types that lower to `Named` denote the SAME type
            // when their names differ only by module qualification — e.g. the bare
            // imported alias `Num` (after `using Symbolics`) and the fully-qualified
            // `Symbolics.Num`. Module qualification is not part of the type identity
            // (the reflexive `self == other` shortcut above already accepts the
            // identical-spelling case), so a `{<:Num}` element bound must match a
            // `Matrix{Symbolics.Num}` actual exactly as the qualified `{<:Symbolics.Num}`
            // spelling does (Issue #8019). This mirrors the sibling `(Struct, Named)`
            // and `(Named, AbstractUser)` arms, which already compare the stripped
            // family names before consulting the hierarchy (cf. #7263 / #7265).
            (Self::Named(child), Self::Named(parent)) => {
                base_type_name(child) == base_type_name(parent)
                    || match hierarchy {
                        Some(hierarchy) => {
                            registered_struct_is_subtype_of_in(hierarchy, child, parent)
                        }
                        None => false,
                    }
            }
            // A non-parametric user type (struct or `abstract type`) lowers to
            // `Named`; when its declared-parent chain reaches a BUILT-IN abstract
            // (`struct Money <: Real`, `abstract type Currency <: Number`), the
            // relation is decided by the same `struct_is_subtype_of_abstract`
            // machinery as the parametric `(Struct, Abstract)` arm above — the
            // chain is walked through the supplied hierarchy. Without this arm a
            // bare user name fell through to `_ => false`, so `Money <: Real` was
            // spuriously false and the runtime had to recover it from a separate
            // `type_ancestors` walk (Issue #5915 wave 3).
            (Self::Named(name), Self::Abstract(a)) => match hierarchy {
                Some(hierarchy) => struct_is_subtype_of_abstract_in(hierarchy, name, &[], a),
                None => struct_is_subtype_of_abstract(name, &[], a),
            },
            _ => false,
        }
    }

    /// Strict subtype dominance on method signatures:
    /// `self` is `<:` `other` but `other` is not `<:` `self`.
    ///
    /// This is the pure-function groundwork for eventual `morespecific`
    /// integration (Issue #5072 / #5925). It is not equivalent to upstream
    /// `type_morespecific_`; it is only the unambiguous, subtype-decidable
    /// fragment of that order.
    #[allow(dead_code)]
    pub fn strict_subtype_dominates(&self, other: &Self) -> bool {
        self.is_subtype_of(other) && !other.is_subtype_of(self)
    }

    #[allow(dead_code)]
    pub fn strict_subtype_dominates_with_hierarchy(
        &self,
        other: &Self,
        hierarchy: &StructHierarchy,
    ) -> bool {
        self.is_subtype_of_with_hierarchy(other, hierarchy)
            && !other.is_subtype_of_with_hierarchy(self, hierarchy)
    }
}
