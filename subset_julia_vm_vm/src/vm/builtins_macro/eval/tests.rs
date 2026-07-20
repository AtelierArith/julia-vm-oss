// Whole-file test-only (declared `#[cfg(test)] mod tests;` in
// `vm/builtins_macro/eval.rs`); this inner allow overrides that ancestor's
// `#![deny(clippy::unwrap_used)]`/`#![deny(clippy::expect_used)]` cascade
// from `vm/mod.rs` (Issue #10979 Phase 4 of #10869).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::api::compile_and_run_value;

/// Unbounded self-recursive `eval` must fail safely with a Stack overflow
/// VmError rather than crashing the host process by exhausting the native
/// call stack (Issue #5014). `eval_dispatch_call` recurses on the Rust stack
/// for every nested VM call started from the `eval` builtin, so an
/// `eval`-driven self-recursion would otherwise segfault.
#[test]
fn eval_self_recursion_fails_with_stack_overflow() {
    let src = r#"
            f() = eval(Meta.parse("f()"))
            f()
        "#;
    let result = compile_and_run_value(src, 0);
    let err = result.expect_err("unbounded eval self-recursion must return an error");
    assert!(
        err.contains("Stack overflow"),
        "expected a Stack overflow runtime error, got: {err}"
    );
}

/// A bounded `eval`-driven dispatch into a user function must still
/// succeed, so the depth guard does not regress ordinary `eval` usage
/// (`eval(Meta.parse("g(0)"))` recurses one level through the VM call path).
#[test]
fn eval_bounded_dispatch_succeeds() {
    let src = r#"
            function g(n)
                if n <= 0
                    return 42
                end
                return eval(Meta.parse("g(0)")) + n
            end
            g(5)
        "#;
    let result = compile_and_run_value(src, 0);
    let value = result.expect("bounded eval dispatch should succeed");
    assert!(
        matches!(value, crate::vm::Value::I64(47)),
        "g(5) = eval(g(0)) + 5 = 42 + 5 should yield 47, got: {value:?}"
    );
}

/// The classic arithmetic `eval` path (no VM dispatch) must remain
/// unaffected by the depth guard (Issue #5014 regression guard).
#[test]
fn eval_arithmetic_still_works() {
    let result = compile_and_run_value(r#"eval(Meta.parse("1 + 1"))"#, 0);
    let value = result.expect("arithmetic eval should succeed");
    assert!(
        matches!(value, crate::vm::Value::I64(2)),
        "eval of 1 + 1 should yield 2, got: {value:?}"
    );
}

#[test]
fn eval_runtime_struct_definition_is_immediately_constructible_11546() {
    let src = r#"
            eval(:(struct EvalRuntimeStruct11546
                x::Int
            end))
            eval(:(EvalRuntimeStruct11546(42).x))
        "#;
    let result = compile_and_run_value(src, 0);
    assert!(
        matches!(result, Ok(crate::vm::Value::I64(42))),
        "a runtime eval-defined struct must publish immediately: {result:?}"
    );
}

/// `eval(:(f(x) = expr))` — a runtime-constructed short-form method
/// definition — must define a callable method instead of raising
/// "assignment target must be Symbol" (Issue #8647).
#[test]
fn eval_short_form_function_def_defines_callable_method_issue_8647() {
    let src = r#"
            eval(:(doubleit8647(x) = x + x))
            eval(:(doubleit8647(9)))
        "#;
    let result = compile_and_run_value(src, 0);
    let value = result.expect("eval of a short-form function def should succeed");
    assert!(
        matches!(value, crate::vm::Value::I64(18)),
        "doubleit8647(9) = 9 + 9 should yield 18, got: {value:?}"
    );
}

/// Redefining a method purely through `eval` — both the definition and
/// every observing call going through `eval` — must be observed by
/// later calls even after the dispatch decision has been warmed by many
/// prior calls. This is the Issue #8561 call-site/dispatch-cache
/// invalidation contract (`note_method_table_mutation` via
/// `activate_eval_function`) extended from `@eval` to plain runtime
/// `eval`: a stale cached resolution here would silently keep returning
/// the pre-redefinition value.
#[test]
fn eval_redefinition_after_warmup_is_observed_issue_8647() {
    let src = r#"
            eval(:(warmed8647(x) = x + 1))
            total = 0
            for _ in 1:20
                total += eval(:(warmed8647(1)))
            end
            eval(:(warmed8647(x) = x + 100))
            (total, eval(:(warmed8647(1))))
        "#;
    let result = compile_and_run_value(src, 0);
    let value = result.expect("eval redefinition after warmup should succeed");
    let crate::vm::Value::Tuple(tuple) = value else {
        panic!("expected a 2-tuple result, got: {value:?}");
    };
    assert!(
        matches!(
            tuple.elements.as_slice(),
            [crate::vm::Value::I64(40), crate::vm::Value::I64(101)]
        ),
        "20 pre-redefinition calls should sum to 40 (x=1 each), and the \
             post-redefinition call must observe the NEW body (101), not a \
             stale cached 2; got: {:?}",
        tuple.elements
    );
}

/// Typed parameters (`x::Int`) in a runtime-`eval`-defined method are
/// explicitly deferred (Issue #8647): every eval-defined parameter
/// dispatches as `::Any`, so silently accepting a type annotation would
/// misrepresent the method's actual dispatch behavior. This must raise a
/// clear, catchable error rather than mis-dispatching or panicking.
#[test]
fn eval_typed_param_function_def_is_deferred_not_silently_wrong_issue_8647() {
    let result = compile_and_run_value("eval(:(typed8647(x::Int) = x))", 0);
    let err = result.expect_err("typed parameters must be rejected, not silently accepted");
    assert!(
        err.contains("Issue #8647") && err.contains("deferred"),
        "expected a deferred-scope error referencing Issue #8647, got: {err}"
    );
}

/// Keyword parameters in a runtime-`eval`-defined method are deferred
/// for the same reason as typed parameters (Issue #8647).
#[test]
fn eval_kwarg_function_def_is_deferred_not_silently_wrong_issue_8647() {
    let result = compile_and_run_value("eval(:(kwf8647(; k=1) = k))", 0);
    let err = result.expect_err("keyword parameters must be rejected, not silently accepted");
    assert!(
        err.contains("Issue #8647") && err.contains("deferred"),
        "expected a deferred-scope error referencing Issue #8647, got: {err}"
    );
}

/// A qualified method name (`Mod.f(x) = ...`) in a runtime-`eval`
/// definition is deferred (Issue #8647): this must not be silently
/// treated as a bare, module-less `f`.
#[test]
fn eval_qualified_name_function_def_is_deferred_not_silently_wrong_issue_8647() {
    let result = compile_and_run_value("eval(:(Base.sin8647(x) = x))", 0);
    let err = result.expect_err("a qualified method name must be rejected");
    assert!(
        err.contains("Issue #8647"),
        "expected a deferred-scope error referencing Issue #8647, got: {err}"
    );
}

/// A signature annotation naming a type that is NOT bound raises `UndefVarError`
/// at the definition, like upstream's eager signature evaluation (Issue #11146;
/// the two assertions Phase 1a of #10813 handed over from
/// `types/signature_forward_reference_11025.jl`).
///
/// Before this, the whole eval typed-parameter path raised
/// `VmError::NotImplemented`, which had no Julia exception object at all — the
/// caught value was a raw `String`, so `typeof(e)` was not even an `Exception`
/// subtype.
#[test]
fn eval_forward_referenced_annotation_raises_undefvarerror_11146() {
    let result = compile_and_run_value("eval(:(fwd11146(x::NotYetDefined11146) = 1))", 0);
    let err = result.expect_err("a forward-referenced annotation must raise");
    assert!(
        err.contains("UndefVarError") && err.contains("NotYetDefined11146"),
        "expected UndefVarError naming the unbound annotation, got: {err}"
    );
}

/// The `where`-bound form of the same thing: the BOUND is evaluated (upstream
/// constructs the method's TypeVars before the parameter annotations), while the
/// binder itself is not (Issue #11146).
#[test]
fn eval_forward_referenced_where_bound_raises_undefvarerror_11146() {
    let result = compile_and_run_value(
        "eval(:(fwdw11146(x::T) where {T<:NotYetDefined11146} = 1))",
        0,
    );
    let err = result.expect_err("a forward-referenced where bound must raise");
    assert!(
        err.contains("UndefVarError") && err.contains("NotYetDefined11146"),
        "expected UndefVarError naming the unbound bound, got: {err}"
    );
    // The binder `T` itself must NOT be probed -- it is bound BY the `where`.
    assert!(
        !err.contains("`T`"),
        "the where-binder must not be probed as an unbound name, got: {err}"
    );
}

/// The probe must not report a type that plainly EXISTS as unbound. A type
/// declared inside `module M` is registered under its qualified name (`M.Local`)
/// and the VM tracks no "current module" during a plain `eval`, so a naive bare-
/// name lookup called it a forward reference and raised a WRONG `UndefVarError`.
///
/// Caught by an adversarial `codex exec` review of this diff. The definition
/// still fails -- typed parameters remain deferred (Issue #8647) -- but it must
/// fail with the HONEST reason, not a fabricated undefined-variable claim: a
/// wrong exception class is exactly the defect Issue #11146 exists to remove.
#[test]
fn eval_module_local_annotation_is_not_reported_undefined_11146() {
    let source = "
module MEval11146
struct Local11146
    v::Int64
end
function define_it()
    eval(:(fmod11146(x::Local11146) = 1))
end
end
MEval11146.define_it()
";
    let result = compile_and_run_value(source, 0);
    let err = result.expect_err("typed parameters are still deferred (Issue #8647)");
    assert!(
        !err.contains("UndefVarError"),
        "a module-local type that IS defined must not be reported as an undefined \
         forward reference; got: {err}"
    );
    assert!(
        err.contains("deferred") || err.contains("Issue #8647"),
        "expected the honest 'typed parameters are deferred' gap, got: {err}"
    );
}
