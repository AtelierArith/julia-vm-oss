//! Subtype checking, specificity, and type matching for JuliaType.

use super::parsing::{is_type_variable_name, parse_parametric_name, unbounded_vararg_element};
use super::{strip_base_type_prefix, JuliaType};
use crate::types::{StructHierarchy, TypeParam};

impl JuliaType {
    /// Check if `self` is a subtype of `other` (`self <: other`).
    ///
    /// This is the **compile-time** (enum-based) counterpart of
    /// `Vm::check_subtype()` in `vm/type_ops.rs` (runtime, string-based). Both
    /// delegate the built-in type hierarchy (numeric, range, container) to the
    /// shared `CoreSubtypeEngine`, so there is a single source of truth — new
    /// types belong in `inference_core`, not here (Issue #2494 / #5921). The
    /// agreement is locked by `test_check_subtype_parity_with_julia_type`.
    ///
    /// # Examples
    /// ```
    /// use subset_julia_vm::types::JuliaType;
    ///
    /// assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Integer));
    /// assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Number));
    /// assert!(JuliaType::Int64.is_subtype_of(&JuliaType::Any));
    /// assert!(!JuliaType::Int64.is_subtype_of(&JuliaType::Float64));
    /// ```
    pub fn is_subtype_of(&self, other: &JuliaType) -> bool {
        self.is_subtype_of_with_lookup(other, None)
    }

    /// Hierarchy-aware counterpart of [`is_subtype_of`].
    ///
    /// `is_subtype_of` runs at the enum level with no struct hierarchy, so a
    /// `Type{<:Bound}` whose bound name `JuliaType::from_name` cannot resolve
    /// (user abstracts, bare user structs, and parametric bounds spelled with
    /// method `where` parameters such as `Pairs{K,V,I,A}`) is *permissively*
    /// accepted — the enum check has nothing to consult, so it stays accepting
    /// to protect user-abstract / `Pairs`-family dispatch. When a caller does
    /// have the program's [`StructHierarchy`] (e.g. the dispatch matcher in
    /// `compile/method_table.rs`), it should use this variant: the bound is then
    /// decided by `CoreSubtypeEngine::with_hierarchy`, so the same relations the
    /// hierarchy-aware runtime path (`Vm::check_subtype`) gets right are decided
    /// here too, instead of being permissively accepted (Issue #6596).
    pub fn is_subtype_of_in(&self, other: &JuliaType, hierarchy: &StructHierarchy) -> bool {
        self.is_subtype_of_with_lookup(other, Some(hierarchy))
    }

    fn is_subtype_of_with_lookup(
        &self,
        other: &JuliaType,
        hierarchy: Option<&StructHierarchy>,
    ) -> bool {
        if self == other {
            return true;
        }
        // Bottom is a subtype of everything
        if matches!(self, JuliaType::Bottom) {
            return true;
        }
        // Union decomposition (`Union{...} <: U` is ∀-members, `T <:
        // Union{...}` is ∃-member) is decided by the CoreSubtypeEngine below
        // (Issue #5915): `CoreType` has its own Union arms, so the former
        // local decomposition early-returns were redundant and are deleted.
        //
        // When a hierarchy is supplied (Issue #6596) the engine consults it via
        // `with_hierarchy`, so user-abstract and parametric bounds inside
        // `Type{<:Bound}` / bounded typevars are decided exactly instead of
        // permissively accepted by the local residue below.
        let core_subtype = match hierarchy {
            Some(h) => crate::inference_core::CoreSubtypeEngine::with_hierarchy(h),
            None => crate::inference_core::CoreSubtypeEngine::new(),
        };
        if core_subtype.is_subtype(
            &crate::inference_core::CoreType::from(self),
            &crate::inference_core::CoreType::from(other),
        ) {
            return true;
        }
        match other {
            JuliaType::Any => true,
            JuliaType::Bottom => false,
            JuliaType::TypeOf(inner) => {
                // `Type{T}` invariance (`Type{A} <: Type{B}` ⇔ `A === B`) and
                // the covariant `Type{<:B}` spelling (a `TypeOf(TypeVar(_,
                // <:B))`, Issue #5068) are decided by the CoreSubtypeEngine
                // call above (Issue #5915): its `(TypeOf, TypeOf)` arm checks
                // mutual subtyping for invariance and the bound for the
                // covariant spelling.
                //
                // The local residue is the fallback for `Type{<:Bound}` bound
                // names `JuliaType::from_name` cannot resolve (user structs /
                // user abstracts / parametric bounds like `Pairs{K,V,I,A}`).
                let (JuliaType::TypeOf(self_inner), JuliaType::TypeVar(_, Some(bn))) =
                    (self, inner.as_ref())
                else {
                    return false;
                };
                if JuliaType::from_name(bn).is_some() {
                    // A resolvable bound was already decided by the engine above.
                    return false;
                }
                // Without a hierarchy the enum check has nothing to consult, so
                // an unresolvable bound stays accepting (preserving
                // user-abstract / `Pairs`-family dispatch). WITH a hierarchy
                // (Issue #6596) the bound is decided through the supplied
                // `StructHierarchy`.
                self_inner.unresolved_bound_is_satisfied(bn, hierarchy)
            }
            // The built-in numeric hierarchy (Number/Real/Integer/Signed/
            // Unsigned/AbstractFloat as the supertype) is decided entirely by
            // the CoreSubtypeEngine delegation above (Issue #5921); pairs the
            // engine rejects fall through to `_ => false` below.
            //
            // Issue #5157: Complex/Rational subtyping of Number/Real is also
            // not hardcoded here. A struct's declared supertype is honored via
            // the struct-hierarchy paths (runtime `type_ancestors`, and the
            // dispatch `struct_parents` fallback from Issue #5363), exactly as
            // for any user `struct S <: Number`.
            // The remaining local AbstractArray arm keeps enum-only
            // projections (BitArray/range aliases) that do not yet carry a
            // full struct hierarchy into this context.
            JuliaType::AbstractArray => {
                matches!(
                    self,
                    JuliaType::Array
                        | JuliaType::VectorOf(_)
                        | JuliaType::MatrixOf(_)
                        | JuliaType::AbstractArray
                ) || is_array_struct(self)
                    || bitarray_projection(self).is_some()
                    || range_projection(self).is_some()
            }
            JuliaType::Array => {
                matches!(
                    self,
                    JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) | JuliaType::Array
                ) || is_array_struct(self)
            }
            JuliaType::Tuple => matches!(self, JuliaType::TupleOf(_) | JuliaType::Tuple),
            JuliaType::NamedTuple => {
                if matches!(self, JuliaType::NamedTuple) {
                    return true;
                }
                if let JuliaType::Struct(name) = self {
                    // Concrete `@NamedTuple{...}` and the names-only
                    // `NamedTuple{(:a, :b)}` marker (Issue #5063) are both
                    // subtypes of the unparameterized `NamedTuple`.
                    return name.starts_with("@NamedTuple{")
                        || named_tuple_marker_field_names(name).is_some();
                }
                false
            }
            JuliaType::Struct(other_name) => {
                if other_name == "AbstractVector" {
                    return matches!(self, JuliaType::VectorOf(_))
                        || bitarray_projection(self).is_some_and(|(_, dim)| dim == Some(1))
                        || range_projection(self).is_some_and(|(_, dim)| dim == Some(1));
                }
                if other_name == "AbstractMatrix" {
                    return matches!(self, JuliaType::MatrixOf(_))
                        || bitarray_projection(self).is_some_and(|(_, dim)| dim == Some(2));
                }
                if let Some((abstract_elem, abstract_dim)) =
                    abstract_array_struct_projection(other_name)
                {
                    return abstract_array_projection(self).is_some_and(|(self_elem, self_dim)| {
                        array_dims_match(self_dim, abstract_dim) && self_elem == abstract_elem
                    });
                }
                if let (Some((self_elem, self_dim)), Some((other_elem, other_dim))) =
                    (array_projection(self), array_struct_projection(other_name))
                {
                    return array_dims_match(self_dim, other_dim) && self_elem == other_elem;
                }
                // Type-level `NamedTuple{(:a, :b)}` names-only marker (Issue
                // #5063): a concrete named tuple is a subtype iff it has exactly
                // these field names in this order (field types are covariant /
                // unconstrained for the names-only form).
                if let Some(target_names) = named_tuple_marker_field_names(other_name) {
                    if let JuliaType::Struct(self_name) = self {
                        if let Some(self_names) = named_tuple_field_names(self_name) {
                            return self_names == target_names;
                        }
                    }
                    return false;
                }
                if let JuliaType::Struct(self_name) = self {
                    let self_name = strip_base_type_prefix(self_name);
                    let other_name = strip_base_type_prefix(other_name);
                    // Parametric struct: Foo{Int64} <: Foo
                    if let Some(bi) = self_name.find('{') {
                        if &self_name[..bi] == other_name {
                            return true;
                        }
                        if &self_name[..bi] == "@NamedTuple" && other_name == "NamedTuple" {
                            return true;
                        }
                    }
                    // Reverse: Foo <: Foo{Int64} (unparameterized matches parameterized base)
                    if let Some(bi) = other_name.find('{') {
                        if self_name == &other_name[..bi] {
                            return true;
                        }
                    }
                }
                false
            }
            JuliaType::TupleOf(other_types) => {
                if let JuliaType::TupleOf(self_types) = self {
                    self_types.len() == other_types.len()
                        && self_types
                            .iter()
                            .zip(other_types.iter())
                            .all(|(s, o)| s.is_subtype_of_with_lookup(o, hierarchy))
                } else {
                    false
                }
            }
            JuliaType::VectorOf(oe) => {
                if let JuliaType::VectorOf(se) = self {
                    se == oe
                } else {
                    false
                }
            }
            JuliaType::MatrixOf(oe) => {
                if let JuliaType::MatrixOf(se) = self {
                    se == oe
                } else {
                    false
                }
            }
            JuliaType::AbstractUser(abstract_name, parent) => {
                if let JuliaType::AbstractUser(sa, sp) = self {
                    if sa == abstract_name {
                        return true;
                    }
                    if let Some(sp) = sp {
                        if sp == abstract_name {
                            return true;
                        }
                    }
                }
                let abstract_core = crate::inference_core::CoreType::from_julia_name(abstract_name);
                if matches!(abstract_core, crate::inference_core::CoreType::Abstract(_)) {
                    return core_subtype
                        .is_subtype(&crate::inference_core::CoreType::from(self), &abstract_core);
                }

                // Bug #5582: a user abstract's declared parent is a supertype,
                // not a covariant bound for the abstract itself. `Float64 <:
                // Real` must not imply `Float64 <: AbstractIrrational` just
                // because `AbstractIrrational <: Real`. Preserve only the
                // existing Any-rooted built-in family aliases (Issue #4708).
                if parent.as_deref() == Some("Any")
                    && matches!(abstract_core, crate::inference_core::CoreType::Abstract(_))
                {
                    return core_subtype
                        .is_subtype(&crate::inference_core::CoreType::from(self), &abstract_core);
                }
                false
            }
            JuliaType::TypeVar(_, bound) => match bound {
                None => true,
                // A bound name `from_name` resolves is checked structurally.
                // When it does not resolve, the bound is either a user type or a
                // parametric `where`-param spelling: without a hierarchy this
                // stays permissive (the enum has nothing to consult), but with a
                // hierarchy (Issue #6596) the bound is decided by the engine,
                // matching the runtime `Vm::check_subtype` path.
                Some(bn) => match JuliaType::from_name(bn) {
                    Some(bt) => self.is_subtype_of_with_lookup(&bt, hierarchy),
                    None => self.unresolved_bound_is_satisfied(bn, hierarchy),
                },
            },
            JuliaType::UnionAll {
                var: _,
                lower_bound: _,
                bound,
                body,
            } => match bound {
                None => self.is_subtype_of_with_lookup(body, hierarchy),
                Some(bn) => match JuliaType::from_name(bn) {
                    Some(bt) => {
                        self.is_subtype_of_with_lookup(&bt, hierarchy)
                            && self.is_subtype_of_with_lookup(body, hierarchy)
                    }
                    None => {
                        self.unresolved_bound_is_satisfied(bn, hierarchy)
                            && self.is_subtype_of_with_lookup(body, hierarchy)
                    }
                },
            },
            _ => false,
        }
    }

    /// Decide whether `self` satisfies a bound name that `JuliaType::from_name`
    /// cannot resolve (user struct / user abstract / parametric `where`-param
    /// spelling). Without a hierarchy this is the historical permissive accept;
    /// with a hierarchy the engine decides it through the supplied
    /// `StructHierarchy` (Issue #6596).
    fn unresolved_bound_is_satisfied(
        &self,
        bound_name: &str,
        hierarchy: Option<&StructHierarchy>,
    ) -> bool {
        let Some(h) = hierarchy else {
            // No hierarchy: nothing to consult, stay permissive (the pre-#6596
            // behavior that protects user-abstract / `Pairs`-family dispatch).
            return true;
        };
        let engine = crate::inference_core::CoreSubtypeEngine::with_hierarchy(h);

        // A parametric bound spelled with the method's free `where` parameters,
        // e.g. `Pairs{K,V,I,A}`, is upstream `Pairs{K,V,I,A} where {K,V,I,A}`:
        // its parameter slots accept any type, so the relation reduces to the
        // bare family `self <: Pairs`. The engine's invariant `Struct` arm would
        // otherwise demand the parameters match the opaque variable names. Strip
        // the parameters to the bare family name whenever every parameter slot is
        // a free type variable (matches `extract_type_bindings`' `Pairs{K,V,I,A}`
        // handling, Issue #6251).
        let (base, params) = parse_parametric_name(bound_name);
        if !params.is_empty() && params.iter().all(|p| is_type_variable_name(p.trim())) {
            return engine.is_subtype(
                &crate::inference_core::CoreType::from(self),
                &crate::inference_core::CoreType::from(&JuliaType::from_name_or_struct(base)),
            );
        }

        engine.is_subtype(
            &crate::inference_core::CoreType::from(self),
            &crate::inference_core::CoreType::from(&JuliaType::from_name_or_struct(bound_name)),
        )
    }

    /// Get specificity score (higher = more specific).
    pub fn specificity(&self) -> u8 {
        crate::inference_core::CoreType::from(self).specificity()
    }

    /// Check if self is a subtype of other, using type_params context for parametric matching.
    ///
    /// This extends `is_subtype_of` to handle cases like `Complex{Float64}` matching
    /// `Complex{T} where T<:Real` by extracting and checking type parameter bounds.
    ///
    /// Returns true if match, false otherwise.
    pub fn is_subtype_of_parametric(&self, other: &JuliaType, type_params: &[TypeParam]) -> bool {
        // First try normal subtype check
        if self.is_subtype_of(other) {
            return true;
        }

        // When self is Any, allow matching against primitive types for compile-time dispatch.
        // This enables compilation when exact types are unknown at compile time
        // (e.g., calling range(start, stop, length) where length has type Any but
        // the parameter is declared as Int64). Runtime will validate the actual type.
        // NOTE: We do NOT allow Any to match parametric struct types (e.g., Rational{T})
        // because when argument type is Any, we should prefer the generic fallback method.
        // Otherwise, the more specific struct method would be selected at compile time,
        // but at runtime the actual value might be a primitive (not the struct), causing errors.
        if matches!(self, JuliaType::Any)
            && (other.is_primitive() || matches!(other, JuliaType::Any))
        {
            return true;
        }

        // Check if 'other' is a type parameter name
        if let JuliaType::Struct(sn) = other {
            if let Some(tp) = type_params.iter().find(|p| p.name == *sn) {
                if let Some(ub) = tp.get_upper_bound() {
                    if let Some(ubt) = JuliaType::from_name(ub) {
                        if !self.is_subtype_of(&ubt) {
                            return false;
                        }
                    }
                }
                if let Some(lb) = &tp.lower_bound {
                    if let Some(lbt) = JuliaType::from_name(lb) {
                        if !lbt.is_subtype_of(self) {
                            return false;
                        }
                    }
                }
                return true;
            }
        }

        // Check Type{T} where T is a type parameter
        if let JuliaType::TypeOf(inner) = other {
            if matches!(self, JuliaType::DataType) {
                if let JuliaType::TypeVar(_, _) = inner.as_ref() {
                    return true;
                }
                return true;
            }
        }

        // Array{T} / Array{T,N} rank-aware matching. This covers Julia's
        // `Array{T}` UnionAll form and the Vector/Matrix aliases when matching
        // methods such as `f(a::Array{T}) where T`.
        if let (Some((self_elem, self_dim)), Some((other_elem, other_dim))) =
            (array_projection(self), array_projection(other))
        {
            return array_dims_match(self_dim, other_dim)
                && array_elem_matches_parametric(&self_elem, &other_elem, type_params);
        }

        // Check parametric struct matching: Complex{Float64} vs Complex{T}
        if let (JuliaType::Struct(sn), JuliaType::Struct(on)) = (self, other) {
            let (sb, sa) = parse_parametric_name(sn);
            let (ob, oa) = parse_parametric_name(on);

            // Strip module prefix for comparison
            fn strip_mod(n: &str) -> &str {
                n.rfind('.').map_or(n, |i| &n[i + 1..])
            }
            let sb = strip_mod(sb);
            let ob = strip_mod(ob);

            if sb != ob {
                return false;
            }
            if sa.is_empty()
                && !oa.is_empty()
                && array_projection(self).is_none()
                && !matches!(
                    sb,
                    "Array"
                        | "Vector"
                        | "Matrix"
                        | "AbstractArray"
                        | "AbstractVector"
                        | "AbstractMatrix"
                        | "NamedTuple"
                )
                && oa
                    .iter()
                    .all(|param| type_params.iter().any(|tp| tp.name == *param))
            {
                return true;
            }
            // If other has no params but self does, it's a match (e.g., Complex{Float64} <: Complex)
            if oa.is_empty() && !sa.is_empty() {
                return true;
            }
            if sa.len() < oa.len() {
                return false;
            }
            for (s, o) in sa.iter().zip(oa.iter()) {
                if let Some(tp) = type_params.iter().find(|p| p.name == *o) {
                    let st =
                        JuliaType::from_name(s).unwrap_or_else(|| JuliaType::Struct(s.to_string()));
                    if let Some(ub) = tp.get_upper_bound() {
                        if let Some(ubt) = JuliaType::from_name(ub) {
                            if !st.is_subtype_of(&ubt) {
                                return false;
                            }
                        }
                    }
                    if let Some(lb) = &tp.lower_bound {
                        if let Some(lbt) = JuliaType::from_name(lb) {
                            if !lbt.is_subtype_of(&st) {
                                return false;
                            }
                        }
                    }
                } else if !parametric_slot_matches(s, o, type_params) {
                    return false;
                }
            }
            return true;
        }

        // VectorOf parametric matching
        if let (JuliaType::VectorOf(se), JuliaType::VectorOf(oe)) = (self, other) {
            if let JuliaType::TypeVar(name, Some(bn)) = oe.as_ref() {
                if name == "_" {
                    return JuliaType::from_name(bn).is_none_or(|bt| se.is_subtype_of(&bt));
                }
            }
            return se.is_subtype_of_parametric(oe, type_params);
        }

        // Array <-> VectorOf interop
        if matches!(self, JuliaType::Array) && matches!(other, JuliaType::VectorOf(_)) {
            return true;
        }
        if matches!(self, JuliaType::VectorOf(_)) && matches!(other, JuliaType::Array) {
            return true;
        }

        // TupleOf parametric matching
        if let (JuliaType::TupleOf(st), JuliaType::TupleOf(ot)) = (self, other) {
            // Trailing unbounded `Vararg{T}` pattern: `Tuple{A, B, Vararg{T}}`
            // matches any tuple with N >= (leading-count) elements where the
            // leading slots match positionally and every remaining element is a
            // subtype of T (Issue #4857).
            if let Some((lead, vararg_elem)) = split_trailing_vararg(ot) {
                if st.len() < lead.len() {
                    return false;
                }
                let leads_ok = st
                    .iter()
                    .zip(lead.iter())
                    .all(|(s, o)| s.is_subtype_of_parametric(o, type_params));
                if !leads_ok {
                    return false;
                }
                return st[lead.len()..]
                    .iter()
                    .all(|s| s.is_subtype_of_parametric(&vararg_elem, type_params));
            }
            if st.len() != ot.len() {
                return false;
            }
            return st
                .iter()
                .zip(ot.iter())
                .all(|(s, o)| s.is_subtype_of_parametric(o, type_params));
        }

        false
    }

    /// Extract type parameter bindings when matching self against a parametric pattern.
    pub fn extract_type_bindings(
        &self,
        pattern: &JuliaType,
        type_params: &[TypeParam],
    ) -> Option<std::collections::HashMap<String, JuliaType>> {
        use std::collections::HashMap;
        let mut bindings = HashMap::new();

        let self_array_projection = if pattern_uses_abstract_array_projection(pattern) {
            abstract_array_projection(self)
        } else {
            array_projection(self)
        };
        if let (Some((self_elem, self_dim)), Some((pattern_elem, pattern_dim))) =
            (self_array_projection, abstract_array_projection(pattern))
        {
            if !array_dims_match(self_dim, pattern_dim) {
                return None;
            }
            if let Some(tp) = type_param_for_pattern(&pattern_elem, type_params) {
                bindings.insert(tp.name.clone(), self_elem);
                return Some(bindings);
            }
            if let Some(nested) = self_elem.extract_type_bindings(&pattern_elem, type_params) {
                bindings.extend(nested);
                return Some(bindings);
            }
            if self_elem == pattern_elem {
                return Some(bindings);
            }
            return None;
        }

        if let JuliaType::Union(members) = pattern {
            for member in members {
                if let Some(extracted) = self.extract_type_bindings(member, type_params) {
                    return Some(extracted);
                }
            }
            return None;
        }

        // Struct-to-struct matching
        if let (JuliaType::Struct(sn), JuliaType::Struct(pn)) = (self, pattern) {
            let (sb, sa) = parse_parametric_name(sn);
            let (pb, pa) = parse_parametric_name(pn);
            if strip_module_prefix(sb) != strip_module_prefix(pb) || sa.len() < pa.len() {
                return None;
            }
            for (s, p) in sa.iter().zip(pa.iter()) {
                let s = s.trim();
                let p = p.trim();
                if let Some(tp) = type_params.iter().find(|tp| tp.name == *p) {
                    let bt =
                        JuliaType::from_name(s).unwrap_or_else(|| JuliaType::Struct(s.to_string()));
                    if let Some(bn) = &tp.bound {
                        if let Some(b) = JuliaType::from_name(bn) {
                            if !bt.is_subtype_of(&b) {
                                return None;
                            }
                        }
                    }
                    bindings.insert(tp.name.clone(), bt);
                } else if !parametric_slot_matches(s, p, type_params) {
                    return None;
                }
            }
            return Some(bindings);
        }

        // VectorOf element matching
        if let (JuliaType::VectorOf(se), JuliaType::VectorOf(pe)) = (self, pattern) {
            return se.extract_type_bindings(pe, type_params);
        }

        // Trailing unbounded `Vararg{T}` binding: `Tuple{A, Vararg{T}}` binds
        // each non-vararg slot positionally and binds T to the join of the
        // trailing element types (Issue #4857).
        if let (JuliaType::TupleOf(st), JuliaType::TupleOf(pt)) = (self, pattern) {
            if let Some((lead, vararg_elem)) = split_trailing_vararg(pt) {
                if st.len() < lead.len() {
                    return None;
                }
                for (se, pe) in st.iter().zip(lead.iter()) {
                    let eb = se.extract_type_bindings(pe, type_params)?;
                    merge_tuple_bindings(&mut bindings, eb)?;
                }
                let joined = join_types(&st[lead.len()..]);
                if let Some(elem) = joined {
                    let eb = elem.extract_type_bindings(&vararg_elem, type_params)?;
                    merge_tuple_bindings(&mut bindings, eb)?;
                } else {
                    // Zero trailing elements: an unbound `T` in the vararg
                    // element cannot be determined, mirroring upstream's
                    // "T not defined in static parameter matching" error path.
                    if vararg_type_var_unbound(&vararg_elem, type_params) {
                        return None;
                    }
                }
                for (vn, bt) in &bindings {
                    if !satisfies_diagonal_rule(vn, bt, pattern) {
                        return None;
                    }
                }
                return Some(bindings);
            }
            if st.len() != pt.len() {
                return None;
            }
            for (se, pe) in st.iter().zip(pt.iter()) {
                if let Some(eb) = se.extract_type_bindings(pe, type_params) {
                    for (name, bt) in eb {
                        match bindings.entry(name) {
                            std::collections::hash_map::Entry::Occupied(e) => {
                                if e.get() != &bt {
                                    return None;
                                }
                            }
                            std::collections::hash_map::Entry::Vacant(e) => {
                                e.insert(bt);
                            }
                        }
                    }
                } else {
                    return None;
                }
            }
            for (vn, bt) in &bindings {
                if !satisfies_diagonal_rule(vn, bt, pattern) {
                    return None;
                }
            }
            return Some(bindings);
        }

        // Anonymous bounded variables (`<:Pairs{K,V,I,A}` / `<:AbstractArray{...,N}`)
        // do not bind the anonymous slot itself, but their bound may contain
        // method `where` parameters that must be recovered for the method body.
        if let JuliaType::TypeVar(vn, Some(bound)) = pattern {
            if !type_params.iter().any(|p| &p.name == vn) {
                let bound_pattern = JuliaType::from_name_or_struct(bound);
                return self.extract_type_bindings(&bound_pattern, type_params);
            }
        }

        // TypeVar pattern matching
        if let JuliaType::TypeVar(vn, _) = pattern {
            if let Some(tp) = type_params.iter().find(|p| &p.name == vn) {
                bindings.insert(tp.name.clone(), self.clone());
                return Some(bindings);
            }
        }

        // Struct name as type parameter
        if let JuliaType::Struct(sn) = pattern {
            if let Some(tp) = type_params.iter().find(|p| &p.name == sn) {
                bindings.insert(tp.name.clone(), self.clone());
                return Some(bindings);
            }
        }

        // Type{T} pattern matching
        if let JuliaType::TypeOf(inner) = pattern {
            if let JuliaType::TypeVar(vn, _) = inner.as_ref() {
                if let Some(tp) = type_params.iter().find(|p| &p.name == vn) {
                    let JuliaType::TypeOf(arg_inner) = self else {
                        return None;
                    };
                    let binding = arg_inner.as_ref().clone();
                    bindings.insert(tp.name.clone(), binding);
                    return Some(bindings);
                }
            }
            if let JuliaType::TypeOf(arg_inner) = self {
                if let Some(extracted) = arg_inner.extract_type_bindings(inner, type_params) {
                    return Some(extracted);
                }
            }
        }

        if self.is_subtype_of(pattern) {
            Some(bindings)
        } else {
            None
        }
    }

    /// Rank of an array *type* (the `N` in `Array{T,N}`), if this type is a
    /// concrete array projection (Issue #5118).
    ///
    /// Mirrors upstream `ndims(::Type{<:AbstractArray{<:Any,N}}) = N`
    /// (`julia/base/abstractarray.jl:279`): `Vector{T} -> 1`, `Matrix{T} -> 2`,
    /// and `Array{T,N} -> N` for any concrete `N`. Returns `None` for the bare
    /// `Array`/`Array{T}` schema (dimension unspecified), where upstream
    /// `ndims` is a `MethodError`.
    ///
    /// This bridges the gap left by the missing value-parameter binding
    /// machinery (Issue #5062): the pure-Julia `ndims(::Type{Array{T,N}})
    /// where {T,N} = N` cannot bind the value parameter `N` yet, so the
    /// `Ndims` builtin reads the rank from the type directly instead.
    pub fn array_type_ndims(&self) -> Option<usize> {
        let (_, dim) = array_projection(self)?;
        dim
    }

    /// Check Diagonal Rule for function parameters (Issue #2554).
    pub fn check_diagonal_rule_for_params(
        param_types: &[JuliaType],
        bindings: &std::collections::HashMap<String, JuliaType>,
    ) -> bool {
        let pattern = JuliaType::TupleOf(param_types.to_vec());
        bindings
            .iter()
            .all(|(vn, bt)| satisfies_diagonal_rule(vn, bt, &pattern))
    }
}

/// Split a tuple-pattern element list into (leading fixed slots, vararg element)
/// when the last element is an unbounded one-arg `Vararg{T}` marker (Issue #4857).
/// Returns `None` when there is no trailing vararg.
fn split_trailing_vararg(pattern_elems: &[JuliaType]) -> Option<(Vec<JuliaType>, JuliaType)> {
    let last = pattern_elems.last()?;
    let vararg_elem = unbounded_vararg_element(last)?;
    let lead = pattern_elems[..pattern_elems.len() - 1].to_vec();
    Some((lead, vararg_elem))
}

/// Merge bindings extracted from one tuple slot into the accumulator, rejecting
/// conflicting bindings for the same type variable.
fn merge_tuple_bindings(
    acc: &mut std::collections::HashMap<String, JuliaType>,
    extracted: std::collections::HashMap<String, JuliaType>,
) -> Option<()> {
    for (name, bt) in extracted {
        match acc.entry(name) {
            std::collections::hash_map::Entry::Occupied(e) => {
                if e.get() != &bt {
                    return None;
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(bt);
            }
        }
    }
    Some(())
}

/// Join a slice of element types into a single representative type. Returns the
/// shared element type when all entries are equal, `Any` when they differ, and
/// `None` for an empty slice (so callers can detect a zero-length vararg).
fn join_types(elems: &[JuliaType]) -> Option<JuliaType> {
    let mut iter = elems.iter();
    let first = iter.next()?.clone();
    if iter.all(|t| *t == first) {
        Some(first)
    } else {
        Some(JuliaType::Any)
    }
}

/// Whether the vararg element type contains a bare type variable that would be
/// left unbound by a zero-length match (e.g. `Tuple{Vararg{T}}` with `()`).
fn vararg_type_var_unbound(vararg_elem: &JuliaType, type_params: &[TypeParam]) -> bool {
    match vararg_elem {
        JuliaType::TypeVar(name, _) | JuliaType::Struct(name) => {
            type_params.iter().any(|tp| tp.name == *name)
        }
        _ => false,
    }
}

/// Analyze how a type variable occurs in a type expression.
/// Returns (covariant_count, invariant_count).
fn analyze_type_var_occurrences(
    ty: &JuliaType,
    var_name: &str,
    inside_invariant: bool,
) -> (u8, u8) {
    let (mut cov, mut inv): (u8, u8) = (0, 0);
    match ty {
        JuliaType::TypeVar(name, _) if name == var_name => {
            if inside_invariant {
                inv = 1;
            } else {
                cov = 1;
            }
        }
        JuliaType::Struct(name) if name == var_name => {
            if inside_invariant {
                inv = 1;
            } else {
                cov = 1;
            }
        }
        JuliaType::TupleOf(types) => {
            for e in types {
                let (c, i) = analyze_type_var_occurrences(e, var_name, inside_invariant);
                cov = cov.saturating_add(c).min(2);
                inv = inv.saturating_add(i).min(2);
            }
        }
        JuliaType::VectorOf(e) | JuliaType::MatrixOf(e) => {
            let (c, i) = analyze_type_var_occurrences(e, var_name, true);
            cov = cov.saturating_add(c).min(2);
            inv = inv.saturating_add(i).min(2);
        }
        JuliaType::TypeOf(inner) => {
            let (c, i) = analyze_type_var_occurrences(inner, var_name, true);
            cov = cov.saturating_add(c).min(2);
            inv = inv.saturating_add(i).min(2);
        }
        JuliaType::Struct(name) => {
            if let Some(bi) = name.find('{') {
                for p in name[bi + 1..name.len() - 1].split(',') {
                    if p.trim() == var_name {
                        inv = inv.saturating_add(1).min(2);
                    }
                }
            }
        }
        _ => {}
    }
    (cov, inv)
}

/// Check if the diagonal rule is satisfied for a type variable binding.
/// The diagonal rule states that if a type variable appears more than once in
/// covariant position and never in invariant position, then the bound type
/// must be concrete.
fn satisfies_diagonal_rule(var_name: &str, bound_type: &JuliaType, pattern: &JuliaType) -> bool {
    let (cov, inv) = analyze_type_var_occurrences(pattern, var_name, false);
    // If the variable appears at most once in covariant position, or appears
    // in any invariant position, the diagonal rule doesn't apply
    if cov <= 1 || inv > 0 {
        return true;
    }

    // When diagonal rule applies, the bound type must be concrete
    bound_type.is_concrete()
}

fn is_array_struct(ty: &JuliaType) -> bool {
    let JuliaType::Struct(name) = ty else {
        return false;
    };
    let base = name.find('{').map_or(name.as_str(), |i| &name[..i]);
    base.rsplit('.').next().unwrap_or(base) == "Array"
}

fn strip_module_prefix(name: &str) -> &str {
    name.rfind('.').map_or(name, |idx| &name[idx + 1..])
}

fn bitarray_projection(ty: &JuliaType) -> Option<(JuliaType, Option<usize>)> {
    let JuliaType::Struct(name) = ty else {
        return None;
    };
    bitarray_name_projection(name)
}

/// Extract the ordered field names from a names-only `NamedTuple{(:a, :b)}`
/// marker (Issue #5063). Returns `None` for any other struct name, including
/// the concrete `@NamedTuple{a::T1, b::T2}` form (handled separately) and the
/// unparameterized `NamedTuple`.
fn named_tuple_marker_field_names(name: &str) -> Option<Vec<String>> {
    let inner = name
        .strip_prefix("NamedTuple{(")?
        .strip_suffix(")}")?
        .trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    // A single-field marker carries a trailing comma (`(:x,)`); skip the empty
    // segments it produces before stripping the `:` sigil.
    inner
        .split(',')
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .map(|raw| raw.strip_prefix(':').map(str::to_string))
        .collect()
}

/// Extract the ordered field names from a concrete named-tuple type name
/// `@NamedTuple{a::T1, b::T2}` (Issue #5063). The field type, if present, is
/// dropped. Returns `None` for non-named-tuple struct names.
fn named_tuple_field_names(name: &str) -> Option<Vec<String>> {
    let inner = name.strip_prefix("@NamedTuple{")?.strip_suffix('}')?.trim();
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(
        split_named_tuple_fields(inner)
            .into_iter()
            .map(|field| {
                field
                    .split_once("::")
                    .map_or(field.trim(), |(n, _)| n.trim())
                    .to_string()
            })
            .collect(),
    )
}

/// Split the comma-separated field declarations of a `@NamedTuple{...}` body,
/// respecting nested `{}` so a field like `c::Tuple{Int, Int}` is not split at
/// its inner comma.
fn split_named_tuple_fields(inner: &str) -> Vec<&str> {
    let mut fields = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                let field = inner[start..i].trim();
                if !field.is_empty() {
                    fields.push(field);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        fields.push(last);
    }
    fields
}

fn array_projection(ty: &JuliaType) -> Option<(JuliaType, Option<usize>)> {
    match ty {
        JuliaType::VectorOf(elem) => Some((*elem.clone(), Some(1))),
        JuliaType::MatrixOf(elem) => Some((*elem.clone(), Some(2))),
        JuliaType::Struct(name) => {
            bitarray_name_projection(name).or_else(|| array_struct_projection(name))
        }
        _ => None,
    }
}

fn abstract_array_projection(ty: &JuliaType) -> Option<(JuliaType, Option<usize>)> {
    array_projection(ty)
        .or_else(|| {
            let JuliaType::Struct(name) = ty else {
                return None;
            };
            abstract_array_struct_projection(name)
        })
        .or_else(|| range_projection(ty))
}

fn pattern_uses_abstract_array_projection(ty: &JuliaType) -> bool {
    matches!(ty, JuliaType::Struct(name) if abstract_array_struct_projection(name).is_some())
}

fn range_projection(ty: &JuliaType) -> Option<(JuliaType, Option<usize>)> {
    match ty {
        JuliaType::AbstractRange | JuliaType::UnitRange | JuliaType::StepRange => {
            Some((JuliaType::Any, Some(1)))
        }
        JuliaType::Struct(name) => range_name_projection(name),
        _ => None,
    }
}

fn range_name_projection(name: &str) -> Option<(JuliaType, Option<usize>)> {
    let (base, params) = parse_parametric_name(name);
    match base.rsplit('.').next().unwrap_or(base) {
        "AbstractRange" | "AbstractUnitRange" | "UnitRange" | "StepRange" | "StepRangeLen"
        | "LinRange" | "OneTo" | "LogRange" => {
            let elem = params
                .first()
                .map(|p| parse_array_projection_param(p))
                .unwrap_or(JuliaType::Any);
            Some((elem, Some(1)))
        }
        _ => None,
    }
}

fn bitarray_name_projection(name: &str) -> Option<(JuliaType, Option<usize>)> {
    let base = name.find('{').map_or(name, |i| &name[..i]);
    match base.rsplit('.').next().unwrap_or(base) {
        "BitVector" => Some((JuliaType::Bool, Some(1))),
        "BitMatrix" => Some((JuliaType::Bool, Some(2))),
        "BitArray" => {
            let dim = name
                .strip_prefix("BitArray{")
                .and_then(|s| s.strip_suffix('}'))
                .and_then(|s| s.trim().parse::<usize>().ok());
            Some((JuliaType::Bool, dim))
        }
        _ => None,
    }
}

fn abstract_array_struct_projection(name: &str) -> Option<(JuliaType, Option<usize>)> {
    let (base, params) = parse_parametric_name(name);
    let rank = match base.rsplit('.').next().unwrap_or(base) {
        "AbstractArray" => params.get(1).and_then(|p| p.trim().parse::<usize>().ok()),
        "AbstractVector" => Some(1),
        "AbstractMatrix" => Some(2),
        _ => return None,
    };
    let elem = params
        .first()
        .map(|p| parse_array_projection_param(p))
        .unwrap_or(JuliaType::Any);
    Some((elem, rank))
}

fn array_struct_projection(name: &str) -> Option<(JuliaType, Option<usize>)> {
    let (base, params) = parse_parametric_name(name);
    if base.rsplit('.').next().unwrap_or(base) != "Array" || params.is_empty() {
        return None;
    }
    let elem = parse_array_projection_param(params[0]);
    let dim = params.get(1).and_then(|p| p.trim().parse::<usize>().ok());
    Some((elem, dim))
}

fn parse_array_projection_param(param: &str) -> JuliaType {
    let param = param.trim();
    if let Some((name, bound)) = param.split_once("<:") {
        let name = name.trim();
        let name = if name.is_empty() { "_" } else { name };
        return JuliaType::TypeVar(name.to_string(), Some(bound.trim().to_string()));
    }
    if is_type_variable_name(param) {
        JuliaType::TypeVar(param.to_string(), None)
    } else {
        JuliaType::from_name_or_struct(param)
    }
}

fn array_dims_match(self_dim: Option<usize>, other_dim: Option<usize>) -> bool {
    other_dim.is_none_or(|expected| self_dim.is_none_or(|actual| actual == expected))
}

fn parametric_slot_matches(actual: &str, pattern: &str, type_params: &[TypeParam]) -> bool {
    let actual = actual.trim();
    let pattern = pattern.trim();
    if actual == pattern {
        return true;
    }
    let actual_ty = JuliaType::from_name_or_struct(actual);
    let pattern_ty = parse_array_projection_param(pattern);
    match &pattern_ty {
        JuliaType::TypeVar(_, Some(bound)) => JuliaType::from_name(bound)
            .is_none_or(|bound_ty| actual_ty.is_subtype_of_parametric(&bound_ty, type_params)),
        _ => actual_ty.is_subtype_of_parametric(&pattern_ty, type_params),
    }
}

fn array_elem_matches_parametric(
    self_elem: &JuliaType,
    pattern_elem: &JuliaType,
    type_params: &[TypeParam],
) -> bool {
    if type_param_for_pattern(pattern_elem, type_params).is_some() {
        return true;
    }
    self_elem == pattern_elem
        || self_elem
            .extract_type_bindings(pattern_elem, type_params)
            .is_some()
        || self_elem.is_subtype_of_parametric(pattern_elem, type_params)
}

fn type_param_for_pattern<'a>(
    pattern: &JuliaType,
    type_params: &'a [TypeParam],
) -> Option<&'a TypeParam> {
    match pattern {
        JuliaType::TypeVar(name, _) | JuliaType::Struct(name) => {
            type_params.iter().find(|p| p.name == *name)
        }
        _ => None,
    }
}
