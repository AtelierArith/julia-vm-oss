// ---------------------------------------------------------------------------
// Issue #4700: ExceptionType::Union — lattice merge of multiple exception
// types into a sorted canonical set. Mirrors upstream Julia's
// `Union{ExcA, ExcB, ...}` exception inference.
// ---------------------------------------------------------------------------

use super::super::*;
use super::*;

#[test]
fn merge_bottom_is_identity() {
    let known = ExceptionType::Known("DomainError");
    assert_eq!(ExceptionType::Bottom.merge(&known), known);
    assert_eq!(known.merge(&ExceptionType::Bottom), known);

    let any = ExceptionType::Any;
    assert_eq!(ExceptionType::Bottom.merge(&any), any);
}

#[test]
fn merge_any_absorbs() {
    let known = ExceptionType::Known("BoundsError");
    let any = ExceptionType::Any;
    assert_eq!(known.merge(&any), ExceptionType::Any);
    assert_eq!(any.merge(&known), ExceptionType::Any);
}

#[test]
fn merge_same_known_stays_known() {
    let a = ExceptionType::Known("DomainError");
    let b = ExceptionType::Known("DomainError");
    assert_eq!(a.merge(&b), ExceptionType::Known("DomainError"));
}

#[test]
fn merge_distinct_known_becomes_union() {
    let a = ExceptionType::Known("DomainError");
    let b = ExceptionType::Known("InexactError");
    let mut expected = BTreeSet::new();
    expected.insert("DomainError");
    expected.insert("InexactError");
    assert_eq!(a.merge(&b), ExceptionType::Union(expected));
}

#[test]
fn merge_union_with_known_extends() {
    let mut s = BTreeSet::new();
    s.insert("DomainError");
    s.insert("InexactError");
    let u = ExceptionType::Union(s);
    let k = ExceptionType::Known("BoundsError");

    let mut expected = BTreeSet::new();
    expected.insert("BoundsError");
    expected.insert("DomainError");
    expected.insert("InexactError");
    assert_eq!(u.merge(&k), ExceptionType::Union(expected));
}

#[test]
fn merge_known_already_in_union_is_idempotent() {
    let mut s = BTreeSet::new();
    s.insert("DomainError");
    s.insert("InexactError");
    let u = ExceptionType::Union(s.clone());
    let k = ExceptionType::Known("DomainError");
    assert_eq!(u.merge(&k), ExceptionType::Union(s));
}

#[test]
fn merge_union_with_union_extends() {
    let mut a_set = BTreeSet::new();
    a_set.insert("DomainError");
    a_set.insert("InexactError");
    let a = ExceptionType::Union(a_set);

    let mut b_set = BTreeSet::new();
    b_set.insert("BoundsError");
    b_set.insert("DomainError");
    let b = ExceptionType::Union(b_set);

    let mut expected = BTreeSet::new();
    expected.insert("BoundsError");
    expected.insert("DomainError");
    expected.insert("InexactError");
    assert_eq!(a.merge(&b), ExceptionType::Union(expected));
}

#[test]
fn union_is_order_independent() {
    // Insertion order should not affect equality — BTreeSet
    // canonicalizes.
    let a = ExceptionType::Known("DomainError").merge(&ExceptionType::Known("InexactError"));
    let b = ExceptionType::Known("InexactError").merge(&ExceptionType::Known("DomainError"));
    assert_eq!(a, b);
}
