//! Single-authority local typing for the safe-to-migrate expression classes
//! (part of Issue #5922 — "partial pre-scan retirement").
//!
//! # Background
//!
//! The compiler historically derived the types of *local variables* in two
//! independent places:
//!
//! 1. The legacy pre-scan ([`crate::compile::inference::collect_local_types_with_mixed_tracking`])
//!    walks each function body and types every assignment's right-hand side with
//!    its own ad-hoc routines (`infer_value_type` / `infer_value_type_with_structs`).
//! 2. The shared lattice-based abstract-interpretation engine
//!    ([`crate::compile::abstract_interp::engine::InferenceEngine`]) types the same
//!    expressions through the lattice + transfer-function registry.
//!
//! Issue #5922 ("RC4 — collapse the dual inference paths") asks us to route a
//! *proven-equivalent* class of locals from the single shared authority instead
//! of the legacy pre-scan, narrowing the duplicated logic one class at a time.
//!
//! # What this module migrates
//!
//! This module owns the migration of **literal right-hand sides**. A literal's
//! type is the simplest, most clearly-equivalent class: it depends on nothing
//! but the literal itself, so the engine and the pre-scan must agree by
//! construction.
//!
//! [`literal_to_lattice`] is the **single source of truth** for "what
//! `LatticeType` does a literal have"; the engine's `infer_literal` delegates to
//! it, and the pre-scan calls [`literal_assignment_value_type`] (which bridges
//! the lattice result back to a [`ValueType`]).
//!
//! The function deliberately returns `LatticeType::Top` for the literal variants
//! the lattice cannot represent with full fidelity (array / struct / module /
//! regex / enum / quoted-AST literals). For those, [`literal_assignment_value_type`]
//! returns `None`, leaving the pre-scan's richer legacy handling in charge. This
//! keeps the migration narrow and regression-free: only literals whose lattice
//! round-trip is provably identical to the legacy `infer_value_type` result flow
//! through the shared authority.

use crate::compile::lattice::types::{ConcreteType, ConstValue, LatticeType};
use crate::inference_core::{CorePrimitive, CoreType};
use crate::ir::core::Literal;
use crate::runtime_types::ValueType;

/// Map a [`Literal`] to its [`LatticeType`] under the shared inference authority.
///
/// This is the single source of truth shared by the abstract-interpretation
/// engine (`InferenceEngine::infer_literal`) and the compiler pre-scan. Basic
/// scalar literals are returned as `Const` to enable constant propagation in the
/// engine; literals that the lattice cannot represent faithfully return
/// `LatticeType::Top`.
///
/// Keeping this mapping in one place guarantees the two inference paths cannot
/// drift apart for literal right-hand sides (Issue #5922).
pub fn literal_to_lattice(lit: &Literal) -> LatticeType {
    match lit {
        // Return Const for basic types to enable constant propagation / folding.
        Literal::Int(v) => LatticeType::Const(ConstValue::Int64(*v)),
        Literal::Float(v) => LatticeType::Const(ConstValue::Float64(*v)),
        Literal::Bool(v) => LatticeType::Const(ConstValue::Bool(*v)),
        Literal::Str(v) => LatticeType::Const(ConstValue::String(v.clone())),
        Literal::Nothing => LatticeType::Const(ConstValue::Nothing),
        Literal::Symbol(s) => LatticeType::Const(ConstValue::Symbol(s.clone())),
        // Wider / preserved-identity numeric literals (Issue #3530): keep their
        // exact concrete type rather than narrowing to Int64 / Float64.
        Literal::Int128(_) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Int128,
        ))),
        Literal::BigInt(_) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigInt,
        ))),
        Literal::BigFloat(_) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::BigFloat,
        ))),
        Literal::Float32(_) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float32,
        ))),
        Literal::Float16(_) => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Float16,
        ))),
        Literal::Char(_) => {
            LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(CorePrimitive::Char)))
        }
        Literal::Missing => LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
            CorePrimitive::Missing,
        ))),
        Literal::DataType(name) => {
            LatticeType::Concrete(ConcreteType::DataType { name: name.clone() })
        }
        // Everything else (Array / ArrayI64 / ArrayBool / Struct / Module /
        // Regex / Enum / Expr / QuoteNode / LineNumberNode / Undef) has no
        // faithful lattice representation here, so it widens to `Top`. The
        // pre-scan keeps its richer legacy handling for these (see
        // `literal_assignment_value_type`).
        _ => LatticeType::Top,
    }
}

/// Return the [`ValueType`] for a literal *assignment right-hand side* when the
/// shared inference authority ([`literal_to_lattice`] + the lattice→ValueType
/// bridge) is **provably equivalent** to the legacy pre-scan result.
///
/// Returns `Some(vt)` for the migrated literal classes (numeric scalars,
/// `Bool`, `String`, `Char`, `Nothing`, `Missing`, `Symbol`) and `None` for the
/// literal classes the legacy pre-scan still owns (array / struct / module /
/// regex / enum / quoted-AST / required-kwarg-marker literals). For the `None`
/// case the caller must fall back to its legacy literal handling.
///
/// This is the migration seam for Issue #5922: the set of literals returning
/// `Some` here is exactly the set whose lattice round-trip matches
/// `infer_value_type`'s literal arm, verified by the equivalence tests in this
/// module and in `compile::inference`.
pub fn literal_assignment_value_type(lit: &Literal) -> Option<ValueType> {
    match literal_to_lattice(lit) {
        // `Top` is the explicit "not migrated — defer to the legacy pre-scan"
        // signal. We must not let it widen a local to `Any` (a wrong, too-wide
        // local type is a codegen-specialization hazard, e.g. an array literal
        // local mis-typed as `Any`). So return `None` rather than `Any`.
        LatticeType::Top => None,
        lattice => Some(crate::runtime_types::bridge::lattice_to_value_type(
            &lattice,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared-authority literal value type must match the historical
    /// `infer_value_type` literal arm for every migrated literal class. This
    /// pins the proven-equivalent subset (Issue #5922): if either path changes,
    /// this test must be revisited deliberately.
    #[test]
    fn migrated_literals_match_legacy_value_types() {
        let cases: Vec<(Literal, ValueType)> = vec![
            (Literal::Int(7), ValueType::I64),
            (Literal::Int128(7), ValueType::I128),
            (Literal::BigInt("7".to_string()), ValueType::BigInt),
            (Literal::BigFloat("1.0".to_string()), ValueType::BigFloat),
            (Literal::Float(1.5), ValueType::F64),
            (Literal::Float32(1.5), ValueType::F32),
            (Literal::Float16(half::f16::from_f32(1.5)), ValueType::F16),
            (Literal::Bool(true), ValueType::Bool),
            (Literal::Str("hi".to_string()), ValueType::Str),
            (Literal::Char('a'), ValueType::Char),
            (Literal::Nothing, ValueType::Nothing),
            (Literal::Missing, ValueType::Missing),
            (Literal::Symbol("foo".to_string()), ValueType::Symbol),
            (
                Literal::DataType("Float64".to_string()),
                ValueType::DataType,
            ),
        ];

        for (lit, expected) in cases {
            assert_eq!(
                literal_assignment_value_type(&lit),
                Some(expected.clone()),
                "literal {:?} should migrate to {:?}",
                lit,
                expected
            );
        }
    }

    /// Literal classes the lattice cannot represent faithfully must defer to the
    /// legacy pre-scan (return `None`), never silently widen the local to `Any`.
    #[test]
    fn non_migrated_literals_defer_to_legacy() {
        let deferred: Vec<Literal> = vec![
            Literal::Array(vec![1.0], vec![1]),
            Literal::ArrayI64(vec![1], vec![1]),
            Literal::ArrayBool(vec![true], vec![1]),
            Literal::Struct("Foo".to_string(), vec![]),
            Literal::Module("Base".to_string()),
            Literal::Regex {
                pattern: "a".to_string(),
                flags: String::new(),
            },
            Literal::Enum {
                type_name: "Color".to_string(),
                value: 0,
            },
            Literal::Expr {
                head: "call".to_string(),
                args: vec![],
            },
            Literal::QuoteNode(Box::new(Literal::Int(1))),
            Literal::LineNumberNode {
                line: 1,
                file: None,
            },
            Literal::Undef,
        ];

        for lit in deferred {
            assert_eq!(
                literal_assignment_value_type(&lit),
                None,
                "literal {:?} must defer to the legacy pre-scan",
                lit
            );
        }
    }
}
