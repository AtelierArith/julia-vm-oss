/// Runtime module projection must retain the declaration owner in the callable
/// singleton identity. Presentation under the same field name is not enough:
/// the shared HOF call site must not reuse one module's dispatch for another.
#[test]
fn dynamic_module_functions_keep_distinct_singleton_owners_11685() {
    run_with_large_stack(|| {
        let mut session = new_session();
        let modules = session
            .eval("module OwnerA11685; f() = 1; end; module OwnerB11685; f() = 2; end; true");
        assert!(modules.success, "{:?}", modules.error);

        let values = session.eval(
            "owner_a_11685 = identity(OwnerA11685); owner_b_11685 = identity(OwnerB11685); callable_a_11685 = getfield(owner_a_11685, :f); callable_b_11685 = getfield(owner_b_11685, :f); typeof(callable_a_11685) !== typeof(callable_b_11685)",
        );
        assert!(values.success, "{:?}", values.error);
        assert!(matches!(values.value, Some(Value::Bool(true))));

        let definition = session.eval("call_owner_11685(f) = f()");
        assert!(definition.success, "{:?}", definition.error);
        let calls = session
            .eval("(call_owner_11685(callable_a_11685), call_owner_11685(callable_b_11685))");
        assert!(calls.success, "{:?}", calls.error);
        assert!(
            matches!(
                &calls.value,
                Some(Value::Tuple(tuple))
                    if matches!(tuple.elements.as_slice(), [Value::I64(1), Value::I64(2)])
            ),
            "unexpected dynamic module call results: {:?}",
            calls.value
        );
    });
}
