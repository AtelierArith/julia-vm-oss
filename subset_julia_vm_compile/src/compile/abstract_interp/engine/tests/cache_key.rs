use super::super::*;
use super::*;
use crate::inference_core::{CorePrimitive, CoreType};

// ----- Issue #3510: typed inference cache keys with controlled const specialization -----

/// `f(x) = x` is invoked with two distinct large-integer `Const` arguments;
/// after the first call populates the cache, the second call must hit it
/// instead of re-inferring (the widened cache key should collapse them).
#[test]
fn test_cache_key_widens_large_int_consts_to_same_entry() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "id".to_string(),
        params: vec![TypedParam {
            name: "x".to_string(),
            type_annotation: None,
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("x".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let big1 = LatticeType::Const(ConstValue::Int64(1_000_000));
    let big2 = LatticeType::Const(ConstValue::Int64(2_000_000));

    let r1 = engine.infer_function_with_arg_types(&func, std::slice::from_ref(&big1));
    let r2 = engine.infer_function_with_arg_types(&func, std::slice::from_ref(&big2));

    // Same widened key → must produce equal results.
    assert_eq!(r1, r2);

    // Both lookups should find the same cached entry under the widened key
    // (Concrete(Int64)), not under the original Const slots.
    let cached_widened = engine.get_cached_return_type(
        "id",
        &[LatticeType::Concrete(ConcreteType::Core(
            CoreType::Primitive(CorePrimitive::Int64),
        ))],
    );
    assert!(
        cached_widened.is_some(),
        "expected widened-Int64 cache entry, got cache lookup miss"
    );

    // The entry under the literal large-Const key should NOT exist —
    // it has been collapsed by the new widening policy.
    assert!(
        engine
            .get_cached_return_type("id", std::slice::from_ref(&big1))
            .is_some(),
        "lookup with the original Const argtype should also resolve via the same widened key"
    );
}

/// Bool consts must remain distinct cache entries because they affect
/// branch elimination. `f(true)` and `f(false)` should produce two
/// independent cache entries (they may even infer different return types).
#[test]
fn test_cache_key_keeps_bool_consts_distinct() {
    let mut engine = InferenceEngine::new();

    // function f(b)
    //     if b
    //         return 1
    //     else
    //         return 1.0
    //     end
    // end
    let func = Function {
        name: "branch".to_string(),
        params: vec![TypedParam {
            name: "b".to_string(),
            type_annotation: None,
            is_varargs: false,
            vararg_count: None,
            span: dummy_span(),
        }],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::If {
                condition: Expr::Var("b".to_string().into(), dummy_span()),
                then_branch: Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(Literal::Int(1), dummy_span())),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
                else_branch: Some(Block {
                    stmts: vec![Stmt::Return {
                        value: Some(Expr::Literal(Literal::Float(1.0), dummy_span())),
                        span: dummy_span(),
                    }],
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

    let t = LatticeType::Const(ConstValue::Bool(true));
    let f = LatticeType::Const(ConstValue::Bool(false));

    let _ = engine.infer_function_with_arg_types(&func, std::slice::from_ref(&t));
    let _ = engine.infer_function_with_arg_types(&func, std::slice::from_ref(&f));

    // Distinct cache entries for the two bool values.
    let c_true = engine.get_cached_return_type("branch", std::slice::from_ref(&t));
    let c_false = engine.get_cached_return_type("branch", std::slice::from_ref(&f));
    assert!(c_true.is_some(), "missing cache entry for branch(true)");
    assert!(c_false.is_some(), "missing cache entry for branch(false)");
}

/// Mixed call: the second argument widens (large Int), the first stays
/// `Const(Symbol)`. Calls that differ only in the widened slot must
/// share a cache entry.
#[test]
fn test_cache_key_mixed_const_and_widened_args() {
    let mut engine = InferenceEngine::new();

    let func = Function {
        name: "mixed".to_string(),
        params: vec![
            TypedParam {
                name: "tag".to_string(),
                type_annotation: None,
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
            TypedParam {
                name: "n".to_string(),
                type_annotation: None,
                is_varargs: false,
                vararg_count: None,
                span: dummy_span(),
            },
        ],
        kwparams: vec![],
        type_params: vec![],
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Var("n".to_string().into(), dummy_span())),
                span: dummy_span(),
            }],
            span: dummy_span(),
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span: dummy_span(),
        new_struct_name: None,
    };

    let sym = LatticeType::Const(ConstValue::Symbol("tag_a".to_string()));
    let big1 = LatticeType::Const(ConstValue::Int64(1_000_000));
    let big2 = LatticeType::Const(ConstValue::Int64(2_000_000));

    let r1 = engine.infer_function_with_arg_types(&func, &[sym.clone(), big1.clone()]);
    let r2 = engine.infer_function_with_arg_types(&func, &[sym.clone(), big2.clone()]);
    assert_eq!(
        r1, r2,
        "calls differing only in the widened Int slot should share cache entry"
    );

    // Different symbol → different cache slot.
    let sym_b = LatticeType::Const(ConstValue::Symbol("tag_b".to_string()));
    let _ = engine.infer_function_with_arg_types(&func, &[sym_b.clone(), big1.clone()]);

    let entry_a = engine.get_cached_return_type("mixed", &[sym, big1.clone()]);
    let entry_b = engine.get_cached_return_type("mixed", &[sym_b, big1]);
    assert!(entry_a.is_some());
    assert!(entry_b.is_some());
}
