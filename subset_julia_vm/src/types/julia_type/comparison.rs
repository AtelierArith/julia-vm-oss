//! Subtype checking, specificity, and type matching for JuliaType.

use super::parsing::parse_parametric_name;
use super::JuliaType;
use crate::types::TypeParam;

impl JuliaType {
    /// Check if `self` is a subtype of `other` (`self <: other`).
    ///
    /// This is the **compile-time** (enum-based) counterpart of
    /// `Vm::check_subtype()` in `vm/type_ops.rs` (runtime, string-based).
    /// Both implementations must cover the same type hierarchy. When adding new
    /// types, update both and run `test_check_subtype_parity_with_julia_type`. (Issue #2494)
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
        if self == other {
            return true;
        }
        // Bottom is a subtype of everything
        if matches!(self, JuliaType::Bottom) {
            return true;
        }
        // Union{T1, T2, ...} <: U iff T1 <: U and T2 <: U
        if let JuliaType::Union(self_types) = self {
            return self_types.iter().all(|t| t.is_subtype_of(other));
        }
        // T <: Union{T1, T2, ...} iff T <: T1 or T <: T2 or ...
        if let JuliaType::Union(other_types) = other {
            return other_types.iter().any(|t| self.is_subtype_of(t));
        }
        match other {
            JuliaType::Any => true,
            JuliaType::Bottom => false,
            JuliaType::TypeOf(inner) => {
                if let JuliaType::TypeOf(si) = self {
                    si.is_subtype_of(inner)
                } else {
                    false
                }
            }
            JuliaType::Number => {
                if matches!(
                    self,
                    JuliaType::Int8
                        | JuliaType::Int16
                        | JuliaType::Int32
                        | JuliaType::Int64
                        | JuliaType::Int128
                        | JuliaType::BigInt
                        | JuliaType::UInt8
                        | JuliaType::UInt16
                        | JuliaType::UInt32
                        | JuliaType::UInt64
                        | JuliaType::UInt128
                        | JuliaType::Bool
                        | JuliaType::Float16
                        | JuliaType::Float32
                        | JuliaType::Float64
                        | JuliaType::BigFloat
                        | JuliaType::Integer
                        | JuliaType::Signed
                        | JuliaType::Unsigned
                        | JuliaType::Real
                        | JuliaType::AbstractFloat
                        | JuliaType::Number
                ) {
                    return true;
                }
                if let JuliaType::Struct(name) = self {
                    let base = name.find('{').map_or(name.as_str(), |i| &name[..i]);
                    return base == "Complex" || base == "Rational";
                }
                false
            }
            JuliaType::Real => {
                if matches!(
                    self,
                    JuliaType::Int8
                        | JuliaType::Int16
                        | JuliaType::Int32
                        | JuliaType::Int64
                        | JuliaType::Int128
                        | JuliaType::BigInt
                        | JuliaType::UInt8
                        | JuliaType::UInt16
                        | JuliaType::UInt32
                        | JuliaType::UInt64
                        | JuliaType::UInt128
                        | JuliaType::Bool
                        | JuliaType::Float16
                        | JuliaType::Float32
                        | JuliaType::Float64
                        | JuliaType::BigFloat
                        | JuliaType::Integer
                        | JuliaType::Signed
                        | JuliaType::Unsigned
                        | JuliaType::AbstractFloat
                        | JuliaType::Real
                ) {
                    return true;
                }
                if let JuliaType::Struct(name) = self {
                    let base = name.find('{').map_or(name.as_str(), |i| &name[..i]);
                    return base == "Rational";
                }
                false
            }
            JuliaType::Integer => matches!(
                self,
                JuliaType::Int8
                    | JuliaType::Int16
                    | JuliaType::Int32
                    | JuliaType::Int64
                    | JuliaType::Int128
                    | JuliaType::BigInt
                    | JuliaType::UInt8
                    | JuliaType::UInt16
                    | JuliaType::UInt32
                    | JuliaType::UInt64
                    | JuliaType::UInt128
                    | JuliaType::Bool
                    | JuliaType::Signed
                    | JuliaType::Unsigned
                    | JuliaType::Integer
            ),
            JuliaType::Signed => matches!(
                self,
                JuliaType::Int8
                    | JuliaType::Int16
                    | JuliaType::Int32
                    | JuliaType::Int64
                    | JuliaType::Int128
                    | JuliaType::BigInt
                    | JuliaType::Signed
            ),
            JuliaType::Unsigned => matches!(
                self,
                JuliaType::UInt8
                    | JuliaType::UInt16
                    | JuliaType::UInt32
                    | JuliaType::UInt64
                    | JuliaType::UInt128
                    | JuliaType::Unsigned
            ),
            JuliaType::AbstractFloat => matches!(
                self,
                JuliaType::Float16
                    | JuliaType::Float32
                    | JuliaType::Float64
                    | JuliaType::BigFloat
                    | JuliaType::AbstractFloat
            ),
            JuliaType::AbstractString => {
                matches!(self, JuliaType::String | JuliaType::AbstractString)
            }
            JuliaType::AbstractChar => matches!(self, JuliaType::Char | JuliaType::AbstractChar),
            JuliaType::AbstractRange => matches!(
                self,
                JuliaType::UnitRange | JuliaType::StepRange | JuliaType::AbstractRange
            ),
            JuliaType::IO => matches!(self, JuliaType::IOBuffer | JuliaType::IO),
            JuliaType::Function => matches!(self, JuliaType::Function),
            JuliaType::Type => matches!(
                self,
                JuliaType::DataType | JuliaType::Type | JuliaType::TypeOf(_)
            ),
            JuliaType::AbstractArray => matches!(
                self,
                JuliaType::Array
                    | JuliaType::VectorOf(_)
                    | JuliaType::MatrixOf(_)
                    | JuliaType::AbstractArray
            ),
            JuliaType::Array => matches!(
                self,
                JuliaType::VectorOf(_) | JuliaType::MatrixOf(_) | JuliaType::Array
            ),
            JuliaType::Tuple => matches!(self, JuliaType::TupleOf(_) | JuliaType::Tuple),
            JuliaType::NamedTuple => {
                if matches!(self, JuliaType::NamedTuple) {
                    return true;
                }
                if let JuliaType::Struct(name) = self {
                    return name.starts_with("@NamedTuple{");
                }
                false
            }
            JuliaType::Struct(other_name) => {
                if let JuliaType::Struct(self_name) = self {
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
                            .all(|(s, o)| s.is_subtype_of(o))
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
                // Any struct is a subtype of any abstract user type (conservative)
                if matches!(self, JuliaType::Struct(_)) {
                    return true;
                }
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
                if let Some(pn) = parent {
                    if let Some(pt) = JuliaType::from_name(pn) {
                        return self.is_subtype_of(&pt);
                    }
                }
                false
            }
            JuliaType::TypeVar(_, bound) => match bound {
                None => true,
                Some(bn) => {
                    JuliaType::from_name(bn).is_none_or(|bt| self.is_subtype_of(&bt))
                }
            },
            JuliaType::UnionAll {
                var: _,
                bound,
                body,
            } => match bound {
                None => self.is_subtype_of(body),
                Some(bn) => JuliaType::from_name(bn).map_or_else(
                    || self.is_subtype_of(body),
                    |bt| self.is_subtype_of(&bt) && self.is_subtype_of(body),
                ),
            },
            _ => false,
        }
    }

    /// Get specificity score (higher = more specific).
    pub fn specificity(&self) -> u8 {
        match self {
            JuliaType::Any => 0,
            JuliaType::Number
            | JuliaType::AbstractString
            | JuliaType::AbstractChar
            | JuliaType::AbstractArray
            | JuliaType::AbstractRange
            | JuliaType::Function
            | JuliaType::IO => 1,
            JuliaType::Real => 2,
            JuliaType::Integer | JuliaType::AbstractFloat => 3,
            JuliaType::Signed | JuliaType::Unsigned => 4,
            JuliaType::Int8
            | JuliaType::Int16
            | JuliaType::Int32
            | JuliaType::Int64
            | JuliaType::Int128
            | JuliaType::BigInt
            | JuliaType::UInt8
            | JuliaType::UInt16
            | JuliaType::UInt32
            | JuliaType::UInt64
            | JuliaType::UInt128
            | JuliaType::Bool
            | JuliaType::Float16
            | JuliaType::Float32
            | JuliaType::Float64
            | JuliaType::BigFloat
            | JuliaType::String
            | JuliaType::Char
            | JuliaType::Array
            | JuliaType::Tuple
            | JuliaType::NamedTuple
            | JuliaType::Dict
            | JuliaType::Set
            | JuliaType::UnitRange
            | JuliaType::StepRange
            | JuliaType::Nothing
            | JuliaType::Missing
            | JuliaType::Module
            | JuliaType::Type
            | JuliaType::DataType
            | JuliaType::Symbol
            | JuliaType::Expr
            | JuliaType::QuoteNode
            | JuliaType::LineNumberNode
            | JuliaType::GlobalRef
            | JuliaType::Pairs
            | JuliaType::Generator
            | JuliaType::IOBuffer => 5,
            JuliaType::TupleOf(elems) => {
                if elems.is_empty() {
                    5
                } else {
                    elems.iter().map(|t| t.specificity()).sum::<u8>()
                }
            }
            JuliaType::VectorOf(e) | JuliaType::MatrixOf(e) => e.specificity(),
            JuliaType::Struct(name) => {
                if let Some(start) = name.find('{') {
                    if let Some(end) = name.rfind('}') {
                        let all_concrete = name[start + 1..end].split(',').all(|p| {
                            let p = p.trim();
                            p.is_empty()
                                || !(p.len() <= 2
                                    && p.chars().all(|c| c.is_uppercase() || c.is_numeric()))
                        });
                        if all_concrete {
                            5
                        } else {
                            4
                        }
                    } else {
                        5
                    }
                } else {
                    5
                }
            }
            JuliaType::AbstractUser(_, _) => 1,
            JuliaType::TypeVar(_, _) | JuliaType::Bottom => 0,
            JuliaType::Union(_) => 1,
            JuliaType::TypeOf(inner) => {
                if inner.specificity() == 0 {
                    1
                } else {
                    5
                }
            }
            JuliaType::UnionAll { body, .. } => body.specificity().saturating_sub(1).max(1),
            JuliaType::Enum(_) => 5,
        }
    }

    /// Check if self is a subtype of other, using type_params context for parametric matching.
    ///
    /// This extends `is_subtype_of` to handle cases like `Complex{Float64}` matching
    /// `Complex{T} where T<:Real` by extracting and checking type parameter bounds.
    ///
    /// Returns true if match, false otherwise.
    pub fn is_subtype_of_parametric(
        &self,
        other: &JuliaType,
        type_params: &[TypeParam],
    ) -> bool {
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
            // If other has no params but self does, it's a match (e.g., Complex{Float64} <: Complex)
            if oa.is_empty() && !sa.is_empty() {
                return true;
            }
            if sa.len() != oa.len() {
                return false;
            }
            for (s, o) in sa.iter().zip(oa.iter()) {
                if let Some(tp) = type_params.iter().find(|p| p.name == *o) {
                    let st = JuliaType::from_name(s)
                        .unwrap_or_else(|| JuliaType::Struct(s.to_string()));
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
                } else if s != o {
                    return false;
                }
            }
            return true;
        }

        // VectorOf parametric matching
        if let (JuliaType::VectorOf(se), JuliaType::VectorOf(oe)) = (self, other) {
            if let JuliaType::TypeVar(name, Some(bn)) = oe.as_ref() {
                if name == "_" {
                    return JuliaType::from_name(bn)
                        .is_none_or(|bt| se.is_subtype_of(&bt));
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

        // Struct-to-struct matching
        if let (JuliaType::Struct(sn), JuliaType::Struct(pn)) = (self, pattern) {
            let (sb, sa) = parse_parametric_name(sn);
            let (pb, pa) = parse_parametric_name(pn);
            if sb != pb || sa.len() != pa.len() {
                return None;
            }
            for (s, p) in sa.iter().zip(pa.iter()) {
                if let Some(tp) = type_params.iter().find(|tp| tp.name == *p) {
                    let bt = JuliaType::from_name(s)
                        .unwrap_or_else(|| JuliaType::Struct(s.to_string()));
                    if let Some(bn) = &tp.bound {
                        if let Some(b) = JuliaType::from_name(bn) {
                            if !bt.is_subtype_of(&b) {
                                return None;
                            }
                        }
                    }
                    bindings.insert(tp.name.clone(), bt);
                } else if s != p {
                    return None;
                }
            }
            return Some(bindings);
        }

        // VectorOf element matching
        if let (JuliaType::VectorOf(se), JuliaType::VectorOf(pe)) = (self, pattern) {
            return se.extract_type_bindings(pe, type_params);
        }

        // TupleOf element-wise matching
        if let (JuliaType::TupleOf(st), JuliaType::TupleOf(pt)) = (self, pattern) {
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
                    bindings.insert(tp.name.clone(), self.clone());
                    return Some(bindings);
                }
            }
        }

        if self.is_subtype_of(pattern) {
            Some(bindings)
        } else {
            None
        }
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
fn satisfies_diagonal_rule(
    var_name: &str,
    bound_type: &JuliaType,
    pattern: &JuliaType,
) -> bool {
    let (cov, inv) = analyze_type_var_occurrences(pattern, var_name, false);
    // If the variable appears at most once in covariant position, or appears
    // in any invariant position, the diagonal rule doesn't apply
    if cov <= 1 || inv > 0 {
        return true;
    }

    // When diagonal rule applies, the bound type must be concrete
    bound_type.is_concrete()
}
