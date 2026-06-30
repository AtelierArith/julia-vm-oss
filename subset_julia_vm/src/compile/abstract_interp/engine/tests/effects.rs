use super::super::*;
use super::*;

#[test]
fn test_issue_3714_inferred_expr_tracks_call_exception_effects() {
    let mut engine = InferenceEngine::new();
    let env = TypeEnv::new();

    let pure_call = Expr::Call {
        function: "+".to_string(),
        args: vec![
            Expr::Literal(Literal::Int(1), dummy_span()),
            Expr::Literal(Literal::Int(2), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };
    let pure = engine.infer_expr_result(&pure_call, &env);
    assert!(pure.effects.nothrow);
    assert_eq!(pure.exct, ExceptionType::Bottom);

    let getindex_call = Expr::Call {
        function: "getindex".to_string(),
        args: vec![
            Expr::ArrayLiteral {
                elements: vec![Expr::Literal(Literal::Int(1), dummy_span())],
                shape: vec![1],
                span: dummy_span(),
            },
            Expr::Literal(Literal::Int(1), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };
    let getindex = engine.infer_expr_result(&getindex_call, &env);
    assert!(!getindex.effects.nothrow);
    assert_eq!(getindex.exct, ExceptionType::Known("BoundsError"));

    let div_call = Expr::Call {
        function: "div".to_string(),
        args: vec![
            Expr::Literal(Literal::Int(1), dummy_span()),
            Expr::Literal(Literal::Int(2), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };
    let div = engine.infer_expr_result(&div_call, &env);
    assert!(!div.effects.nothrow);
    assert_eq!(div.exct, ExceptionType::Known("DivideError"));

    let float_div_call = Expr::Call {
        function: "div".to_string(),
        args: vec![
            Expr::Literal(Literal::Float(1.0), dummy_span()),
            Expr::Literal(Literal::Float(2.0), dummy_span()),
        ],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false, false],
        span: dummy_span(),
    };
    let float_div = engine.infer_expr_result(&float_div_call, &env);
    assert!(float_div.effects.nothrow);
    assert_eq!(float_div.exct, ExceptionType::Bottom);

    // Issue #4274: sqrt of a known negative constant keeps the
    // conservative DomainError exception type.
    let sqrt_neg_call = Expr::Call {
        function: "sqrt".to_string(),
        args: vec![Expr::Literal(Literal::Int(-1), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };
    let sqrt_neg = engine.infer_expr_result(&sqrt_neg_call, &env);
    assert!(!sqrt_neg.effects.nothrow);
    assert_eq!(sqrt_neg.exct, ExceptionType::Known("DomainError"));

    // Issue #4274: sqrt of a known non-negative constant is nothrow
    // (DomainError cannot fire for non-negative real inputs).
    let sqrt_pos_call = Expr::Call {
        function: "sqrt".to_string(),
        args: vec![Expr::Literal(Literal::Int(4), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };
    let sqrt_pos = engine.infer_expr_result(&sqrt_pos_call, &env);
    assert!(sqrt_pos.effects.nothrow);
    assert_eq!(sqrt_pos.exct, ExceptionType::Bottom);

    // Float constants are refined identically.
    let sqrt_float_pos = Expr::Call {
        function: "sqrt".to_string(),
        args: vec![Expr::Literal(Literal::Float(2.5), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };
    let result = engine.infer_expr_result(&sqrt_float_pos, &env);
    assert!(result.effects.nothrow);
    assert_eq!(result.exct, ExceptionType::Bottom);

    // log/log10/log2 share the const-refinement path.
    for fname in ["log", "log10", "log2"] {
        let log_pos_call = Expr::Call {
            function: fname.to_string(),
            args: vec![Expr::Literal(Literal::Int(2), dummy_span())],
            kwargs: vec![],
            kwargs_splat_mask: vec![],
            splat_mask: vec![false],
            span: dummy_span(),
        };
        let log_pos = engine.infer_expr_result(&log_pos_call, &env);
        assert!(log_pos.effects.nothrow, "{fname}(2) should be nothrow");
        assert_eq!(log_pos.exct, ExceptionType::Bottom, "{fname}(2) exct");

        let log_neg_call = Expr::Call {
            function: fname.to_string(),
            args: vec![Expr::Literal(Literal::Int(-1), dummy_span())],
            kwargs: vec![],
            kwargs_splat_mask: vec![],
            splat_mask: vec![false],
            span: dummy_span(),
        };
        let log_neg = engine.infer_expr_result(&log_neg_call, &env);
        assert!(
            !log_neg.effects.nothrow,
            "{fname}(-1) should not be nothrow"
        );
        // Issue #4700: log family widens to
        // `Union{DomainError, InexactError}` (matches upstream
        // `Base.infer_exception_type(() -> log(-1))`). PR #4699
        // previously had to pick `Known("DomainError")` because the
        // Union variant did not exist; PR #4838 added it.
        let mut expected_set = BTreeSet::new();
        expected_set.insert("DomainError");
        expected_set.insert("InexactError");
        assert_eq!(
            log_neg.exct,
            ExceptionType::Union(expected_set),
            "{fname}(-1) exct"
        );
    }

    let unknown_call = Expr::Call {
        function: "mystery".to_string(),
        args: vec![Expr::Literal(Literal::Int(1), dummy_span())],
        kwargs: vec![],
        kwargs_splat_mask: vec![],
        splat_mask: vec![false],
        span: dummy_span(),
    };
    let unknown = engine.infer_expr_result(&unknown_call, &env);
    assert_eq!(unknown.ty, LatticeType::Top);
    assert!(!unknown.effects.nothrow);
    assert_eq!(unknown.exct, ExceptionType::Any);
}
