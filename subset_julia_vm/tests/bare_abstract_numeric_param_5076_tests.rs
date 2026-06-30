//! Regression tests for Issue #5076.
//!
//! A method annotated with a *bare* abstract numeric type (`x::Real`,
//! `x::Number`, `x::Integer`, `x::Signed`, ...) must preserve the concrete
//! argument type when its body calls a type-generic function (`zero`, `one`,
//! `oneunit`). Before the fix, `type_helpers::julia_type_to_value_type` widened
//! `Real`/`Number` params to `ValueType::F64` (and `Integer` to `ValueType::I64`)
//! in the compiler's `locals`, so `infer_julia_type` reported `Float64`/`Int64`
//! and statically bound `zero(x)` to `zero(::Float64)`. That made
//! `f(x::Real)=zero(x); f(3)` error ("expected I64, got Float64") and
//! `f(Int8(3))` return `0.0::Float64`.
//!
//! The fix makes `infer_julia_type` report `Any` for params already tracked in
//! `abstract_numeric_params` (which already load via `LoadAny`), so type-generic
//! calls dispatch on the concrete runtime value — exactly like the untyped
//! `f(x)=zero(x)` and `where {T<:Real}` forms, and matching upstream Julia.

use subset_julia_vm::compile_and_run_value;
use subset_julia_vm::vm::Value;

fn run(src: &str) -> Value {
    compile_and_run_value(src, 0).unwrap_or_else(|e| panic!("run failed for {src:?}: {e}"))
}

#[test]
fn bare_real_zero_preserves_int64() {
    // Previously errored "expected I64, got Float64".
    assert!(matches!(run("f(x::Real) = zero(x)\nf(3)"), Value::I64(0)));
}

#[test]
fn bare_real_zero_preserves_int8() {
    // Previously returned 0.0::Float64.
    assert!(matches!(
        run("f(x::Real) = zero(x)\nf(Int8(3))"),
        Value::I8(0)
    ));
}

#[test]
fn bare_real_zero_preserves_int32() {
    assert!(matches!(
        run("f(x::Real) = zero(x)\nf(Int32(7))"),
        Value::I32(0)
    ));
}

#[test]
fn bare_real_zero_preserves_float64() {
    match run("f(x::Real) = zero(x)\nf(3.0)") {
        Value::F64(v) => assert_eq!(v, 0.0),
        other => panic!("expected F64(0.0), got {other:?}"),
    }
}

#[test]
fn bare_number_zero_preserves_int8() {
    assert!(matches!(
        run("f(x::Number) = zero(x)\nf(Int8(3))"),
        Value::I8(0)
    ));
}

#[test]
fn bare_integer_zero_preserves_int8() {
    assert!(matches!(
        run("f(x::Integer) = zero(x)\nf(Int8(3))"),
        Value::I8(0)
    ));
}

#[test]
fn bare_signed_zero_preserves_int32() {
    assert!(matches!(
        run("f(x::Signed) = zero(x)\nf(Int32(7))"),
        Value::I32(0)
    ));
}

#[test]
fn bare_real_one_preserves_int8() {
    assert!(matches!(
        run("f(x::Real) = one(x)\nf(Int8(3))"),
        Value::I8(1)
    ));
}

#[test]
fn bare_number_one_preserves_int64() {
    assert!(matches!(run("f(x::Number) = one(x)\nf(3)"), Value::I64(1)));
}

#[test]
fn bare_real_oneunit_preserves_int8() {
    assert!(matches!(
        run("f(x::Real) = oneunit(x)\nf(Int8(3))"),
        Value::I8(1)
    ));
}

#[test]
fn bare_abstract_matches_untyped_form_int() {
    // Both the bare-abstract and untyped forms must agree (and equal Int64 0).
    let bare = run("f(x::Real) = zero(x)\nf(3)");
    let untyped = run("f(x) = zero(x)\nf(3)");
    assert!(matches!(bare, Value::I64(0)));
    assert!(matches!(untyped, Value::I64(0)));
}
