use super::super::*;
use super::*;

fn datatype_lit(name: &str) -> Expr {
    Expr::Literal(Literal::DataType(name.to_string()), dummy_span())
}

fn imprecise_promote_type_function() -> Function {
    Function {
        name: "promote_type".to_string(),
        params: vec![
            TypedParam::new("x".to_string(), Some(JuliaType::DataType), dummy_span()),
            TypedParam::new("y".to_string(), Some(JuliaType::DataType), dummy_span()),
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(datatype_lit("DataType")),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    }
}

#[test]
fn promote_type_call_prefers_typevalue_tfunc_over_generic_body_9914() {
    let mut function_table = HashMap::new();
    function_table.insert(
        "promote_type".to_string(),
        imprecise_promote_type_function(),
    );
    let mut engine = InferenceEngine::with_tables(HashMap::new(), function_table);

    let expr = Expr::Call {
        function: "promote_type".to_string().into(),
        args: vec![
            Expr::Var("Int64".to_string().into(), dummy_span()),
            Expr::Var("Float64".to_string().into(), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };

    let result = engine.infer_expr(&expr, &TypeEnv::new());

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::DataType {
            name: "Float64".to_string()
        })
    );
}

#[test]
fn promote_type_call_recovers_where_param_datatypes_9914() {
    let func = Function {
        name: "nested_promote_type_9914".to_string(),
        params: vec![
            TypedParam::new(
                "a".to_string(),
                Some(JuliaType::TypeVar(
                    "T".to_string(),
                    Some("Real".to_string()),
                )),
                dummy_span(),
            ),
            TypedParam::new(
                "b".to_string(),
                Some(JuliaType::TypeVar(
                    "S".to_string(),
                    Some("Real".to_string()),
                )),
                dummy_span(),
            ),
        ],
        kwparams: vec![],
        type_params: vec![
            crate::types::TypeParam::with_upper_bound("T".to_string(), "Real".to_string()),
            crate::types::TypeParam::with_upper_bound("S".to_string(), "Real".to_string()),
        ],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Call {
                    function: "promote_type".to_string().into(),
                    args: vec![
                        Expr::Var("T".to_string().into(), dummy_span()),
                        Expr::Var("S".to_string().into(), dummy_span()),
                    ],
                    kwargs: vec![],
                    kwargs_splat_mask: vec![],
                    splat_mask: vec![false, false],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };
    let arg = LatticeType::Concrete(ConcreteType::Core(CoreType::Primitive(
        CorePrimitive::Float64,
    )));
    let mut engine = InferenceEngine::new();

    let result = engine.infer_function_with_arg_types(&func, &[arg.clone(), arg]);

    assert_eq!(
        result,
        LatticeType::Concrete(ConcreteType::DataType {
            name: "Float64".to_string()
        })
    );
}
