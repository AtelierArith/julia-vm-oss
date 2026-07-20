/// Helper provenance participates in runtime public/private indexing, so it
/// must survive the real sectioned Base-cache codec rather than defaulting
/// to `false` after a cache hit (Issue #9784).
#[test]
fn lowering_helper_provenance_survives_base_cache_roundtrip_9784() {
    use crate::ir::core::{Block, Expr, Function, Literal, Program, Stmt};
    use crate::span::Span;

    let span = Span::new(0, 1, 1, 1, 1, 2);
    let helper = Function {
        name: "cache_private_helper_9784".to_string(),
        params: Vec::new(),
        kwparams: Vec::new(),
        type_params: Vec::new(),
        return_type: None,
        body: Block {
            stmts: vec![Stmt::Return {
                value: Some(Expr::Literal(Literal::Int(42), span)),
                span,
            }],
            span,
        },
        is_base_extension: false,
        is_runtime_eval: false,
        span,
        new_struct_name: None,
    }
    .into_lowering_helper();
    let program = Program {
        abstract_types: Vec::new(),
        primitive_types: Vec::new(),
        type_aliases: Vec::new(),
        structs: Vec::new(),
        functions: vec![std::sync::Arc::new(helper)],
        base_function_count: 0,
        modules: Vec::new(),
        usings: Vec::new(),
        macros: Vec::new(),
        enums: Vec::new(),
        main: Block {
            stmts: Vec::new(),
            span,
        },
    };
    let Ok(compiled) = crate::compile::compile_core_program(&program) else {
        panic!("explicit lowering helper should compile");
    };
    assert!(
        compiled.functions.iter().any(|function| {
            function.name == "cache_private_helper_9784" && function.is_lowering_helper
        }),
        "fresh compile must classify the helper body as private"
    );

    let Ok(bytes) = serialize_base_cache(&compiled, &HashMap::new(), &HashMap::new(), &[]) else {
        panic!("helper-bearing cache should serialize");
    };
    let Ok(restored) = deserialize_base_cache(&bytes) else {
        panic!("helper-bearing cache should load");
    };
    assert!(
        restored.compiled.functions.iter().any(|function| {
            function.name == "cache_private_helper_9784" && function.is_lowering_helper
        }),
        "cache load must preserve private helper provenance"
    );
}
