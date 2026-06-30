//! Regression tests for Issue #5065 (systematic Union{} / Bottom handling).
//!
//! `const Bottom = Union{}` lives in `base/essentials.jl`, so a *bare* reference
//! to `Bottom` must resolve to the empty type without the user redefining it.
//! Base-level const type aliases live in the prelude program, which is
//! independent of the precompiled-base bytecode cache; before the fix they were
//! dropped on the cached-base path, so `Bottom` raised `UndefVarError` even
//! though `Union{}` worked everywhere. The compiler now also registers the
//! prelude's type aliases, so bare `Bottom` resolves as a DataType value and
//! carries the full Bottom semantics (subtype, typeintersect zero element).

use subset_julia_vm::compile_and_run_value;
use subset_julia_vm::vm::Value;

fn run(src: &str) -> Value {
    compile_and_run_value(src, 0).unwrap_or_else(|e| panic!("run failed for {src:?}: {e}"))
}

fn run_bool(src: &str) -> bool {
    match run(src) {
        Value::Bool(b) => b,
        other => panic!("expected Bool for {src:?}, got {other:?}"),
    }
}

#[test]
fn bare_bottom_resolves_without_redefinition() {
    // Previously: UndefVarError: `Bottom` not defined.
    assert!(run_bool("Bottom === Union{}"));
}

#[test]
fn bottom_is_subtype_of_every_type() {
    assert!(run_bool("Bottom <: Int"));
    assert!(run_bool("Bottom <: Number"));
    assert!(run_bool("Bottom <: String"));
    assert!(run_bool("Bottom <: Any"));
    assert!(run_bool("Bottom <: Bottom"));
}

#[test]
fn only_bottom_is_subtype_of_bottom() {
    assert!(!run_bool("Int <: Bottom"));
    assert!(!run_bool("Any <: Bottom"));
}

#[test]
fn bottom_is_typeintersect_zero_element() {
    assert!(run_bool("typeintersect(Int, String) === Bottom"));
    assert!(run_bool("typeintersect(Int, Bottom) === Bottom"));
    assert!(run_bool("typeintersect(Bottom, Number) === Bottom"));
}

#[test]
fn bottom_collapses_in_union_normalization() {
    assert!(run_bool("Union{Bottom, Int} === Int"));
    assert!(run_bool("Union{Bottom} === Bottom"));
}

#[test]
fn user_const_overrides_base_bottom() {
    // A user `const Bottom = Int` shadows the base binding (later definition
    // wins), matching upstream.
    assert!(run_bool("const Bottom = Int\nBottom === Int"));
}
