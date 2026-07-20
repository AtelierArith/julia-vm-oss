#[test]
fn persisted_callable_identity_ignores_replaced_keyword_and_generated_metadata_9784() {
    use crate::vm::types::KwParamInfo;

    fn keyword(name: &str, ty: ValueType) -> KwParamInfo {
        KwParamInfo {
            name: name.to_string(),
            default: Value::Nothing,
            default_expr: None,
            ty,
            declared_type: None,
            slot: 1,
            required: false,
            is_varargs: false,
        }
    }

    let mut prior_method = dispatch_test_function("kwremap_9784", vec![JuliaType::Int64], vec![]);
    prior_method.kwparams = vec![keyword("k", ValueType::I64)];
    let mut prior = Vm::new(vec![], StableRng::new(0));
    prior.functions = vec![Rc::new(prior_method)];

    let mut replacement = dispatch_test_function("kwremap_9784", vec![JuliaType::Int64], vec![]);
    replacement.kwparams = vec![keyword("q", ValueType::Str)];
    replacement.is_generated = true;
    let mut rebuilt = Vm::new(vec![], StableRng::new(0));
    rebuilt.functions = vec![
        Rc::new(dispatch_test_function(
            "unrelated_9784",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(replacement),
    ];
    let mut globals = vec![(
        "saved".to_string(),
        Value::Function(FunctionValue::with_candidates("kwremap_9784", vec![0])),
    )];

    let snapshot = prior.persisted_callable_snapshot();
    rebuilt.remap_persisted_callable_candidates_from(&snapshot, &mut globals, &mut []);

    assert!(matches!(
        &globals[0].1,
        Value::Function(function) if function.candidate_indices.as_deref() == Some(&[1][..])
    ));
}

#[test]
fn persisted_callable_carrier_matrix_preserves_owner_and_helper_provenance_11703() {
    use crate::vm::value::GeneratorValue;

    fn helper(name: &str) -> FunctionInfo {
        let mut function = dispatch_test_function(name, vec![JuliaType::Int64], vec![]);
        function.is_lowering_helper = true;
        function
    }

    let mut prior = Vm::new(vec![], StableRng::new(11703));
    prior.functions = vec![
        Rc::new(helper("same_spelling_11703")),
        Rc::new(dispatch_test_function(
            "OwnerA11703.owned_11703",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(helper("same_predicate_11703")),
    ];

    let helper_function = prior.function_value_with_candidates("same_spelling_11703", vec![0]);
    let helper_closure = prior.closure_value_with_candidates(
        "same_spelling_11703",
        vec![("captured".to_string(), Value::I64(1))],
        vec![0],
    );
    let owned_function = prior.function_value_with_candidates("owned_11703", vec![1]);
    let mut globals = vec![
        ("function".to_string(), Value::Function(helper_function)),
        ("closure".to_string(), Value::Closure(helper_closure)),
        (
            "generator".to_string(),
            Value::Generator(Box::new(GeneratorValue::with_result_element_type(
                GeneratorCallable::FunctionIndex(0),
                Value::Nothing,
                None,
            ))),
        ),
        (
            "splat_generator".to_string(),
            Value::Generator(Box::new(GeneratorValue::with_result_element_type(
                GeneratorCallable::TupleSplatFunctionIndex(0),
                Value::Nothing,
                None,
            ))),
        ),
        (
            "filtered_generator".to_string(),
            Value::Generator(Box::new(GeneratorValue::with_result_element_type(
                GeneratorCallable::FilteredFunctionIndex {
                    map_func_index: 0,
                    predicate_func_index: 2,
                },
                Value::Nothing,
                None,
            ))),
        ),
        ("owned".to_string(), Value::Function(owned_function)),
    ];

    // The helper spellings now exist only as Julia-visible source functions.
    // The owner-qualified source function still exists, but at a relocated
    // index after an unrelated row.
    let mut rebuilt = Vm::new(vec![], StableRng::new(11704));
    rebuilt.functions = vec![
        Rc::new(dispatch_test_function(
            "unrelated_11703",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "same_spelling_11703",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "OwnerA11703.owned_11703",
            vec![JuliaType::Int64],
            vec![],
        )),
        Rc::new(dispatch_test_function(
            "same_predicate_11703",
            vec![JuliaType::Int64],
            vec![],
        )),
    ];

    let snapshot = prior.persisted_callable_snapshot();
    rebuilt.remap_persisted_callable_candidates_from(&snapshot, &mut globals, &mut []);

    let assert_failed_closed_helper = |value: &Value| match value {
        Value::Function(function) => {
            assert_eq!(function.candidate_indices.as_deref(), Some(&[][..]));
            assert!(function.singleton_identity().is_lowering_helper());
        }
        Value::Closure(closure) => {
            assert_eq!(closure.candidate_indices.as_deref(), Some(&[][..]));
            assert!(closure.singleton_identity().is_lowering_helper());
        }
        other => panic!("expected persisted callable, got {other:?}"),
    };
    assert_failed_closed_helper(&globals[0].1);
    assert_failed_closed_helper(&globals[1].1);

    assert!(matches!(
        &globals[2].1,
        Value::Generator(generator)
            if matches!(
                &generator.callable,
                GeneratorCallable::RuntimeValue(callable)
                    if matches!(callable.as_ref(), Value::Function(function)
                        if function.candidate_indices.as_deref() == Some(&[][..])
                            && function.singleton_identity().is_lowering_helper())
            )
    ));
    assert!(matches!(
        &globals[3].1,
        Value::Generator(generator)
            if matches!(
                &generator.callable,
                GeneratorCallable::TupleSplatRuntimeValue(callable)
                    if matches!(callable.as_ref(), Value::Function(function)
                        if function.candidate_indices.as_deref() == Some(&[][..])
                            && function.singleton_identity().is_lowering_helper())
            )
    ));
    assert!(matches!(
        &globals[4].1,
        Value::Generator(generator)
            if matches!(
                &generator.callable,
                GeneratorCallable::FilteredRuntimeValue { map, predicate }
                    if [map.as_ref(), predicate.as_ref()].iter().all(|callable|
                        matches!(callable, Value::Function(function)
                            if function.candidate_indices.as_deref() == Some(&[][..])
                                && function.singleton_identity().is_lowering_helper()))
            )
    ));

    let Value::Function(owned) = &globals[5].1 else {
        panic!("owner-qualified callable changed carrier")
    };
    assert_eq!(owned.candidate_indices.as_deref(), Some(&[2][..]));
    assert_eq!(
        owned.singleton_identity().owner_names(),
        &["OwnerA11703.owned_11703".to_string()]
    );

    // Deep copy must preserve the semantic authority rather than deriving a
    // fresh source identity from the display spelling.
    let copied = rebuilt.deep_copy_value(&globals[0].1).unwrap();
    assert_failed_closed_helper(&copied);
}
