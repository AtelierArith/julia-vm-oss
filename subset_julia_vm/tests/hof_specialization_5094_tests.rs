//! Consolidated integration tests (Issue #9671 Phase 1).
//! Each original one-off test binary is preserved verbatim as an inline
//! `mod`, so per-test filtering and behavior are unchanged while the number
//! of linked test binaries (each linking the ~370k-line VM rlib) drops.
#![allow(dead_code)]

mod common;

mod hof_binary_map_specialization_5094_tests {
    //! Bytecode checks for binary HOF map specialization (Issue #5094).

    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn binary_map_vector_uses_concrete_resolved_method_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function map_add_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        map(+, xs, ys)
    end

    function map_div_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        map(/, xs, ys)
    end

    function map_div_f32_5094(xs::Vector{Float32}, ys::Vector{Float32})
        map(/, xs, ys)
    end

    function map_min_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        map(min, xs, ys)
    end

    function map_max_bool_5094(xs::Vector{Bool}, ys::Vector{Bool})
        map(max, xs, ys)
    end

    map_add_i32_5094(Int32[1, 2], Int32[3, 4])
    map_div_i32_5094(Int32[1, 2], Int32[3, 4])
    map_div_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0])
    map_min_i32_5094(Int32[1, 2], Int32[3, 4])
    map_max_bool_5094(Bool[true, false], Bool[false, true])
    "#,
        );

        for (function_name, callable_name, element_type, return_type) in [
            (
                "map_add_i32_5094",
                "+",
                "Vector{Int32}",
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "map_div_i32_5094",
                "/",
                "Vector{Int32}",
                ValueType::ArrayOf(ArrayElementType::F64, None),
            ),
            (
                "map_div_f32_5094",
                "/",
                "Vector{Float32}",
                ValueType::ArrayOf(ArrayElementType::F32, None),
            ),
            (
                "map_min_i32_5094",
                "min",
                "Vector{Int32}",
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "map_max_bool_5094",
                "max",
                "Vector{Bool}",
                ValueType::ArrayOf(ArrayElementType::Bool, None),
            ),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(
                func.return_type,
                return_type,
                "{function_name} body: {:?}",
                function_body(&compiled, func)
            );
            assert!(
                function_body(&compiled, func)
                    .iter()
                    .any(|instr| matches!(instr, Instr::CallResolved(_, 3))),
                "typed map({}, {}, {}) should resolve to a concrete 3-arg method: {:?}",
                callable_name,
                element_type,
                element_type,
                function_body(&compiled, func)
            );
        }
    }
}

mod hof_broadcast_binary_specialization_5094_tests {
    //! Bytecode checks for binary numeric broadcast specialization (Issue #5094).

    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    fn has_resolved_or_typed_broadcast_candidate(
        compiled: &CompiledProgram,
        body: &[Instr],
        callable_name: &str,
        element_type: &str,
    ) -> bool {
        body.iter().any(|instr| {
            matches!(instr, Instr::CallResolved(_, 3))
                || matches!(
                    instr,
                    // Issue #6496: CallTypedDispatch candidates are function
                    // indices; the expected signature is derived from each
                    // candidate's FunctionInfo.
                    Instr::CallTypedDispatch(name, 3, _, candidates)
                        if name == "broadcast"
                            && candidates.iter().any(|idx| {
                                compiled.functions.get(*idx).is_some_and(|target_func| {
                                    target_func
                                        .param_julia_types
                                        .iter()
                                        .map(|ty| ty.to_string())
                                        .eq([
                                            format!("typeof({callable_name})"),
                                            element_type.to_string(),
                                            element_type.to_string(),
                                        ])
                                })
                            })
                )
        })
    }

    #[test]
    fn binary_broadcast_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function broadcast_add_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        broadcast(+, xs, ys)
    end

    function broadcast_sub_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        broadcast(-, xs, ys)
    end

    function broadcast_mul_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        broadcast(*, xs, ys)
    end

    function broadcast_div_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        broadcast(/, xs, ys)
    end

    function broadcast_div_f32_5094(xs::Vector{Float32}, ys::Vector{Float32})
        broadcast(/, xs, ys)
    end

    function broadcast_div_f64_5094(xs::Vector{Float64}, ys::Vector{Float64})
        broadcast(/, xs, ys)
    end

    function broadcast_min_i32_5094(xs::Vector{Int32}, ys::Vector{Int32})
        broadcast(min, xs, ys)
    end

    function broadcast_max_f32_5094(xs::Vector{Float32}, ys::Vector{Float32})
        broadcast(max, xs, ys)
    end

    broadcast_add_i32_5094(Int32[1, 2], Int32[3, 4])
    broadcast_sub_i32_5094(Int32[1, 2], Int32[3, 4])
    broadcast_mul_i32_5094(Int32[1, 2], Int32[3, 4])
    broadcast_div_i32_5094(Int32[1, 2], Int32[3, 4])
    broadcast_div_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0])
    broadcast_div_f64_5094([1.0, 2.0], [3.0, 4.0])
    broadcast_min_i32_5094(Int32[1, 2], Int32[3, 4])
    broadcast_max_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0])
    "#,
        );

        for (function_name, callable_name) in [
            ("broadcast_add_i32_5094", "+"),
            ("broadcast_sub_i32_5094", "-"),
            ("broadcast_mul_i32_5094", "*"),
            ("broadcast_min_i32_5094", "min"),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(
                func.return_type,
                ValueType::ArrayOf(ArrayElementType::I32, None)
            );
            assert!(
                has_resolved_or_typed_broadcast_candidate(
                    &compiled,
                    function_body(&compiled, func),
                    callable_name,
                    "Vector{Int32}",
                ),
                "typed broadcast({}, Vector{{Int32}}, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
                callable_name,
                function_body(&compiled, func)
            );
        }

        let max_f32_func = get_function(&compiled, "broadcast_max_f32_5094");
        assert_eq!(
            max_f32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::F32, None)
        );
        assert!(
            has_resolved_or_typed_broadcast_candidate(
                &compiled,
                function_body(&compiled, max_f32_func),
                "max",
                "Vector{Float32}",
            ),
            "typed broadcast(max, Vector{{Float32}}, Vector{{Float32}}) dispatch should include the concrete specialization: {:?}",
            function_body(&compiled, max_f32_func)
        );

        let div_func = get_function(&compiled, "broadcast_div_i32_5094");
        assert_eq!(
            div_func.return_type,
            ValueType::ArrayOf(ArrayElementType::F64, None)
        );
        assert!(
            has_resolved_or_typed_broadcast_candidate(
                &compiled,
                function_body(&compiled, div_func),
                "/",
                "Vector{Int32}",
            ),
            "typed broadcast(/, Vector{{Int32}}, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
            function_body(&compiled, div_func)
        );

        for (function_name, element_type, return_type) in [
            (
                "broadcast_div_f32_5094",
                "Vector{Float32}",
                ValueType::ArrayOf(ArrayElementType::F32, None),
            ),
            (
                "broadcast_div_f64_5094",
                "Vector{Float64}",
                ValueType::ArrayOf(ArrayElementType::F64, None),
            ),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(func.return_type, return_type);
            assert!(
                has_resolved_or_typed_broadcast_candidate(
                    &compiled,
                    function_body(&compiled, func),
                    "/",
                    element_type,
                ),
                "typed broadcast(/, {0}, {0}) dispatch should include the concrete specialization: {1:?}",
                element_type,
                function_body(&compiled, func)
            );
        }
    }
}

mod hof_broadcast_nary_plus_specialization_5094_tests {
    //! Bytecode checks for n-ary numeric broadcast specialization (Issue #5094).

    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn nary_broadcast_plus_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function broadcast_plus3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
        broadcast(+, xs, ys, zs)
    end

    function broadcast_plus3_f32_5094(xs::Vector{Float32}, ys::Vector{Float32}, zs::Vector{Float32})
        broadcast(+, xs, ys, zs)
    end

    function broadcast_plus4_i32_5094(a::Vector{Int32}, b::Vector{Int32}, c::Vector{Int32}, d::Vector{Int32})
        broadcast(+, a, b, c, d)
    end

    function broadcast_plus5_i32_5094(a::Vector{Int32}, b::Vector{Int32}, c::Vector{Int32}, d::Vector{Int32}, e::Vector{Int32})
        broadcast(+, a, b, c, d, e)
    end

    function broadcast_mul3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
        broadcast(*, xs, ys, zs)
    end

    function broadcast_mul3_bool_5094(xs::Vector{Bool}, ys::Vector{Bool}, zs::Vector{Bool})
        broadcast(*, xs, ys, zs)
    end

    function broadcast_max3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
        broadcast(max, xs, ys, zs)
    end

    function broadcast_min3_f32_5094(xs::Vector{Float32}, ys::Vector{Float32}, zs::Vector{Float32})
        broadcast(min, xs, ys, zs)
    end

    broadcast_plus3_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6])
    broadcast_plus3_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0], Float32[5.0, 6.0])
    broadcast_plus4_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6], Int32[7, 8])
    broadcast_plus5_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6], Int32[7, 8], Int32[9, 10])
    broadcast_mul3_i32_5094(Int32[2, 3], Int32[4, 5], Int32[6, 7])
    broadcast_mul3_bool_5094([true, false], [true, true], [false, true])
    broadcast_max3_i32_5094(Int32[1, 20], Int32[10, -2], Int32[100, 2])
    broadcast_min3_f32_5094(Float32[1.0, 20.0], Float32[10.0, -2.0], Float32[100.0, 2.0])
    "#,
        );

        for (function_name, arg_count, return_type) in [
            (
                "broadcast_plus3_i32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "broadcast_plus3_f32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::F32, None),
            ),
            (
                "broadcast_plus4_i32_5094",
                5,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "broadcast_plus5_i32_5094",
                6,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "broadcast_mul3_i32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "broadcast_mul3_bool_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::Bool, None),
            ),
            (
                "broadcast_max3_i32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "broadcast_min3_f32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::F32, None),
            ),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(
                func.return_type,
                return_type,
                "{function_name} body: {:?}",
                function_body(&compiled, func)
            );
            assert!(
                function_body(&compiled, func)
                    .iter()
                    .any(|instr| matches!(instr, Instr::CallResolved(_, n) if *n == arg_count)),
                "typed n-ary broadcast should resolve to a concrete method: {:?}",
                function_body(&compiled, func)
            );
        }
    }
}

mod hof_broadcast_predicate_specialization_5094_tests {
    //! Bytecode checks for unary predicate broadcast specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn predicate_broadcast_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function broadcast_iszero_i32_5094(xs::Vector{Int32})
        broadcast(iszero, xs)
    end

    function broadcast_signbit_i32_5094(xs::Vector{Int32})
        broadcast(signbit, xs)
    end

    function broadcast_iseven_i32_5094(xs::Vector{Int32})
        broadcast(iseven, xs)
    end

    broadcast_iszero_i32_5094(Int32[-3, 0, 4])
    broadcast_signbit_i32_5094(Int32[-3, 0, 4])
    broadcast_iseven_i32_5094(Int32[-3, 0, 4])
    "#,
        );

        for (function_name, callable_name) in [
            ("broadcast_iszero_i32_5094", "iszero"),
            ("broadcast_signbit_i32_5094", "signbit"),
            ("broadcast_iseven_i32_5094", "iseven"),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(
                func.return_type,
                ValueType::ArrayOf(ArrayElementType::Bool, None)
            );
            let expected_callable = format!("typeof({callable_name})");
            let body = function_body(&compiled, func);
            let has_specialized_broadcast_candidate = has_resolved_or_typed_candidate(
                &compiled,
                body,
                "broadcast",
                2,
                &[&expected_callable, "Vector{Int32}"],
            );
            assert!(
                has_specialized_broadcast_candidate,
                "typed broadcast({}, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
                callable_name,
                body,
                resolved_target_debug(&compiled, body)
            );
        }
    }
}

mod hof_broadcast_unary_specialization_5094_tests {
    //! Bytecode checks for unary numeric broadcast specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn unary_broadcast_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function broadcast_identity_i32_5094(xs::Vector{Int32})
        broadcast(identity, xs)
    end

    function broadcast_abs_i32_5094(xs::Vector{Int32})
        broadcast(abs, xs)
    end

    function broadcast_neg_i32_5094(xs::Vector{Int32})
        broadcast(-, xs)
    end

    broadcast_identity_i32_5094(Int32[-3, 0, 4])
    broadcast_abs_i32_5094(Int32[-3, 0, 4])
    broadcast_neg_i32_5094(Int32[-3, 0, 4])
    "#,
        );

        for (function_name, callable_name) in [
            ("broadcast_identity_i32_5094", "identity"),
            ("broadcast_abs_i32_5094", "abs"),
            ("broadcast_neg_i32_5094", "-"),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(
                func.return_type,
                ValueType::ArrayOf(ArrayElementType::I32, None)
            );
            let expected_callable = format!("typeof({callable_name})");
            let body = function_body(&compiled, func);
            let has_specialized_broadcast_candidate = has_resolved_or_typed_candidate(
                &compiled,
                body,
                "broadcast",
                2,
                &[&expected_callable, "Vector{Int32}"],
            );
            assert!(
                has_specialized_broadcast_candidate,
                "typed broadcast({}, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
                callable_name,
                body,
                resolved_target_debug(&compiled, body)
            );
        }
    }
}

mod hof_foldl_minmax_specialization_5094_tests {
    //! Bytecode checks for `foldl(min/max, ::Vector{T})` specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn foldl_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function foldl_min_i32_5094(xs::Vector{Int32})
        foldl(min, xs)
    end

    function foldl_max_i32_5094(xs::Vector{Int32})
        foldl(max, xs)
    end

    foldl_min_i32_5094(Int32[-3, 0, 4, 2])
    foldl_max_i32_5094(Int32[-3, 0, 4, 2])
    "#,
        );

        let min_i32_func = get_function(&compiled, "foldl_min_i32_5094");
        assert_eq!(min_i32_func.return_type, ValueType::I32);
        let min_body = function_body(&compiled, min_i32_func);
        let has_min_i32_specialized_foldl_candidate = has_resolved_or_typed_candidate(
            &compiled,
            min_body,
            "foldl",
            2,
            &["typeof(min)", "Vector{Int32}"],
        );
        assert!(
            has_min_i32_specialized_foldl_candidate,
            "typed foldl(min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            min_body,
            resolved_target_debug(&compiled, min_body)
        );

        let max_i32_func = get_function(&compiled, "foldl_max_i32_5094");
        assert_eq!(max_i32_func.return_type, ValueType::I32);
        let max_body = function_body(&compiled, max_i32_func);
        let has_max_i32_specialized_foldl_candidate = has_resolved_or_typed_candidate(
            &compiled,
            max_body,
            "foldl",
            2,
            &["typeof(max)", "Vector{Int32}"],
        );
        assert!(
            has_max_i32_specialized_foldl_candidate,
            "typed foldl(max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            max_body,
            resolved_target_debug(&compiled, max_body)
        );
    }
}

mod hof_foldr_minmax_specialization_5094_tests {
    //! Bytecode checks for `foldr(min/max, ::Vector{T})` specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn foldr_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function foldr_min_i32_5094(xs::Vector{Int32})
        foldr(min, xs)
    end

    function foldr_max_i32_5094(xs::Vector{Int32})
        foldr(max, xs)
    end

    foldr_min_i32_5094(Int32[-3, 0, 4, 2])
    foldr_max_i32_5094(Int32[-3, 0, 4, 2])
    "#,
        );

        let min_i32_func = get_function(&compiled, "foldr_min_i32_5094");
        assert_eq!(min_i32_func.return_type, ValueType::I32);
        let min_body = function_body(&compiled, min_i32_func);
        let has_min_i32_specialized_foldr_candidate = has_resolved_or_typed_candidate(
            &compiled,
            min_body,
            "foldr",
            2,
            &["typeof(min)", "Vector{Int32}"],
        );
        assert!(
            has_min_i32_specialized_foldr_candidate,
            "typed foldr(min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            min_body,
            resolved_target_debug(&compiled, min_body)
        );

        let max_i32_func = get_function(&compiled, "foldr_max_i32_5094");
        assert_eq!(max_i32_func.return_type, ValueType::I32);
        let max_body = function_body(&compiled, max_i32_func);
        let has_max_i32_specialized_foldr_candidate = has_resolved_or_typed_candidate(
            &compiled,
            max_body,
            "foldr",
            2,
            &["typeof(max)", "Vector{Int32}"],
        );
        assert!(
            has_max_i32_specialized_foldr_candidate,
            "typed foldr(max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            max_body,
            resolved_target_debug(&compiled, max_body)
        );
    }
}

mod hof_mapfoldl_minmax_specialization_5094_tests {
    //! Bytecode checks for `mapfoldl(identity, min/max, ::Vector{T})` specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn mapfoldl_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function mapfoldl_min_i32_5094(xs::Vector{Int32})
        mapfoldl(identity, min, xs)
    end

    function mapfoldl_max_i32_5094(xs::Vector{Int32})
        mapfoldl(identity, max, xs)
    end

    mapfoldl_min_i32_5094(Int32[-3, 0, 4, 2])
    mapfoldl_max_i32_5094(Int32[-3, 0, 4, 2])
    "#,
        );

        let min_i32_func = get_function(&compiled, "mapfoldl_min_i32_5094");
        assert_eq!(min_i32_func.return_type, ValueType::I32);
        let min_body = function_body(&compiled, min_i32_func);
        let has_min_i32_specialized_mapfoldl_candidate = has_resolved_or_typed_candidate(
            &compiled,
            min_body,
            "mapfoldl",
            3,
            &["typeof(identity)", "typeof(min)", "Vector{Int32}"],
        );
        assert!(
            has_min_i32_specialized_mapfoldl_candidate,
            "typed mapfoldl(identity, min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            min_body,
            resolved_target_debug(&compiled, min_body)
        );

        let max_i32_func = get_function(&compiled, "mapfoldl_max_i32_5094");
        assert_eq!(max_i32_func.return_type, ValueType::I32);
        let max_body = function_body(&compiled, max_i32_func);
        let has_max_i32_specialized_mapfoldl_candidate = has_resolved_or_typed_candidate(
            &compiled,
            max_body,
            "mapfoldl",
            3,
            &["typeof(identity)", "typeof(max)", "Vector{Int32}"],
        );
        assert!(
            has_max_i32_specialized_mapfoldl_candidate,
            "typed mapfoldl(identity, max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            max_body,
            resolved_target_debug(&compiled, max_body)
        );
    }
}

mod hof_mapfoldr_minmax_specialization_5094_tests {
    //! Bytecode checks for `mapfoldr(identity, min/max, ::Vector{T})` specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn mapfoldr_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function mapfoldr_min_i32_5094(xs::Vector{Int32})
        mapfoldr(identity, min, xs)
    end

    function mapfoldr_max_i32_5094(xs::Vector{Int32})
        mapfoldr(identity, max, xs)
    end

    mapfoldr_min_i32_5094(Int32[-3, 0, 4, 2])
    mapfoldr_max_i32_5094(Int32[-3, 0, 4, 2])
    "#,
        );

        let min_i32_func = get_function(&compiled, "mapfoldr_min_i32_5094");
        assert_eq!(min_i32_func.return_type, ValueType::I32);
        let min_body = function_body(&compiled, min_i32_func);
        let has_min_i32_specialized_mapfoldr_candidate = has_resolved_or_typed_candidate(
            &compiled,
            min_body,
            "mapfoldr",
            3,
            &["typeof(identity)", "typeof(min)", "Vector{Int32}"],
        );
        assert!(
            has_min_i32_specialized_mapfoldr_candidate,
            "typed mapfoldr(identity, min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            min_body,
            resolved_target_debug(&compiled, min_body)
        );

        let max_i32_func = get_function(&compiled, "mapfoldr_max_i32_5094");
        assert_eq!(max_i32_func.return_type, ValueType::I32);
        let max_body = function_body(&compiled, max_i32_func);
        let has_max_i32_specialized_mapfoldr_candidate = has_resolved_or_typed_candidate(
            &compiled,
            max_body,
            "mapfoldr",
            3,
            &["typeof(identity)", "typeof(max)", "Vector{Int32}"],
        );
        assert!(
            has_max_i32_specialized_mapfoldr_candidate,
            "typed mapfoldr(identity, max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            max_body,
            resolved_target_debug(&compiled, max_body)
        );
    }
}

mod hof_mapreduce_minmax_specialization_5094_tests {
    //! Bytecode checks for `mapreduce(identity, min/max, ::Vector{T})` specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn mapreduce_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function mapreduce_min_i32_5094(xs::Vector{Int32})
        mapreduce(identity, min, xs)
    end

    function mapreduce_max_i32_5094(xs::Vector{Int32})
        mapreduce(identity, max, xs)
    end

    mapreduce_min_i32_5094(Int32[-3, 0, 4, 2])
    mapreduce_max_i32_5094(Int32[-3, 0, 4, 2])
    "#,
        );

        let min_i32_func = get_function(&compiled, "mapreduce_min_i32_5094");
        assert_eq!(min_i32_func.return_type, ValueType::I32);
        let min_body = function_body(&compiled, min_i32_func);
        let has_min_i32_specialized_mapreduce_candidate = has_resolved_or_typed_candidate(
            &compiled,
            min_body,
            "mapreduce",
            3,
            &["typeof(identity)", "typeof(min)", "Vector{Int32}"],
        );
        assert!(
            has_min_i32_specialized_mapreduce_candidate,
            "typed mapreduce(identity, min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            min_body,
            resolved_target_debug(&compiled, min_body)
        );

        let max_i32_func = get_function(&compiled, "mapreduce_max_i32_5094");
        assert_eq!(max_i32_func.return_type, ValueType::I32);
        let max_body = function_body(&compiled, max_i32_func);
        let has_max_i32_specialized_mapreduce_candidate = has_resolved_or_typed_candidate(
            &compiled,
            max_body,
            "mapreduce",
            3,
            &["typeof(identity)", "typeof(max)", "Vector{Int32}"],
        );
        assert!(
            has_max_i32_specialized_mapreduce_candidate,
            "typed mapreduce(identity, max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            max_body,
            resolved_target_debug(&compiled, max_body)
        );
    }
}

mod hof_nary_map_specialization_5094_tests {
    //! Bytecode checks for n-ary HOF map specialization (Issue #5094).

    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn nary_map_plus_vector_uses_concrete_resolved_method_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function map_plus3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
        map(+, xs, ys, zs)
    end

    function map_plus3_f32_5094(xs::Vector{Float32}, ys::Vector{Float32}, zs::Vector{Float32})
        map(+, xs, ys, zs)
    end

    function map_plus4_i32_5094(a::Vector{Int32}, b::Vector{Int32}, c::Vector{Int32}, d::Vector{Int32})
        map(+, a, b, c, d)
    end

    function map_mul3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
        map(*, xs, ys, zs)
    end

    function map_mul3_bool_5094(xs::Vector{Bool}, ys::Vector{Bool}, zs::Vector{Bool})
        map(*, xs, ys, zs)
    end

    function map_max3_i32_5094(xs::Vector{Int32}, ys::Vector{Int32}, zs::Vector{Int32})
        map(max, xs, ys, zs)
    end

    function map_min3_f32_5094(xs::Vector{Float32}, ys::Vector{Float32}, zs::Vector{Float32})
        map(min, xs, ys, zs)
    end

    function map_max3_bool_5094(xs::Vector{Bool}, ys::Vector{Bool}, zs::Vector{Bool})
        map(max, xs, ys, zs)
    end

    map_plus3_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6])
    map_plus3_f32_5094(Float32[1.0, 2.0], Float32[3.0, 4.0], Float32[5.0, 6.0])
    map_plus4_i32_5094(Int32[1, 2], Int32[3, 4], Int32[5, 6], Int32[7, 8])
    map_mul3_i32_5094(Int32[2, 3], Int32[4, 5], Int32[6, 7])
    map_mul3_bool_5094([true, false], [true, true], [false, true])
    map_max3_i32_5094(Int32[1, 20], Int32[10, -2], Int32[100, 2])
    map_min3_f32_5094(Float32[1.0, 20.0], Float32[10.0, -2.0], Float32[100.0, 2.0])
    map_max3_bool_5094([true, false], [false, false], [true, true])
    "#,
        );

        for (function_name, arg_count, return_type) in [
            (
                "map_plus3_i32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "map_plus3_f32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::F32, None),
            ),
            (
                "map_plus4_i32_5094",
                5,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "map_mul3_i32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "map_mul3_bool_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::Bool, None),
            ),
            (
                "map_max3_i32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::I32, None),
            ),
            (
                "map_min3_f32_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::F32, None),
            ),
            (
                "map_max3_bool_5094",
                4,
                ValueType::ArrayOf(ArrayElementType::Bool, None),
            ),
        ] {
            let func = get_function(&compiled, function_name);
            assert_eq!(
                func.return_type,
                return_type,
                "{function_name} body: {:?}",
                function_body(&compiled, func)
            );
            assert!(
                function_body(&compiled, func)
                    .iter()
                    .any(|instr| matches!(instr, Instr::CallResolved(_, n) if *n == arg_count)),
                "typed n-ary map should resolve to a concrete method: {:?}",
                function_body(&compiled, func)
            );
        }
    }
}

mod hof_predicate_reducer_specialization_5094_tests {
    //! Bytecode checks for predicate reducer specialization (Issue #5094).

    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm::types::JuliaType;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    fn resolved_target_debug(
        compiled: &CompiledProgram,
        body: &[Instr],
    ) -> Vec<(String, Vec<JuliaType>)> {
        body.iter()
            .filter_map(|instr| match instr {
                Instr::CallResolved(target, _) => {
                    let target_func = &compiled.functions[*target];
                    Some((
                        target_func.name.clone(),
                        target_func.param_julia_types.clone(),
                    ))
                }
                _ => None,
            })
            .collect()
    }

    fn has_specialized_candidate(
        compiled: &CompiledProgram,
        body: &[Instr],
        function: &str,
        predicate: &str,
    ) -> bool {
        let expected_predicate = format!("typeof({predicate})");
        body.iter().any(|instr| match instr {
            // Issue #6496: CallTypedDispatch candidates are function indices; the
            // expected signature is derived from each candidate's FunctionInfo.
            Instr::CallTypedDispatch(name, 2, _, candidates) if name == function => {
                candidates.iter().any(|idx| {
                    compiled.functions.get(*idx).is_some_and(|target_func| {
                        let signature: Vec<String> = target_func
                            .param_julia_types
                            .iter()
                            .map(|ty| ty.to_string())
                            .collect();
                        signature.len() == 2
                            && signature[0] == expected_predicate
                            && signature[1] == "Vector{Int32}"
                    })
                })
            }
            Instr::CallResolved(target, 2) => {
                let target_func = &compiled.functions[*target];
                target_func.name == function
                    && matches!(
                        target_func.param_julia_types.as_slice(),
                        [
                            JuliaType::Struct(function_type),
                            JuliaType::VectorOf(element),
                        ] if function_type == &expected_predicate
                            && **element == JuliaType::Int32
                    )
            }
            _ => false,
        })
    }

    #[test]
    fn predicate_reducers_use_typed_specialized_candidates_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function any_iszero_i32_5094(xs::Vector{Int32})
        any(iszero, xs)
    end

    function all_iseven_i32_5094(xs::Vector{Int32})
        all(iseven, xs)
    end

    function count_signbit_i32_5094(xs::Vector{Int32})
        count(signbit, xs)
    end

    function findall_isodd_i32_5094(xs::Vector{Int32})
        findall(isodd, xs)
    end

    any_iszero_i32_5094(Int32[-3, 0, 4])
    all_iseven_i32_5094(Int32[0, 4])
    count_signbit_i32_5094(Int32[-3, 0, 4])
    findall_isodd_i32_5094(Int32[-3, 0, 5])
    "#,
        );

        let any_func = get_function(&compiled, "any_iszero_i32_5094");
        assert_eq!(any_func.return_type, ValueType::Bool);
        assert!(
            has_specialized_candidate(&compiled, function_body(&compiled, any_func), "any", "iszero"),
            "typed any(iszero, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}; resolved targets: {:?}",
            function_body(&compiled, any_func),
            resolved_target_debug(&compiled, function_body(&compiled, any_func))
        );

        let all_func = get_function(&compiled, "all_iseven_i32_5094");
        assert_eq!(all_func.return_type, ValueType::Bool);
        assert!(
            has_specialized_candidate(&compiled, function_body(&compiled, all_func), "all", "iseven"),
            "typed all(iseven, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
            function_body(&compiled, all_func)
        );

        let count_func = get_function(&compiled, "count_signbit_i32_5094");
        assert_eq!(count_func.return_type, ValueType::I64);
        assert!(
            has_specialized_candidate(
                &compiled,
                function_body(&compiled, count_func),
                "count",
                "signbit"
            ),
            "typed count(signbit, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
            function_body(&compiled, count_func)
        );

        let findall_func = get_function(&compiled, "findall_isodd_i32_5094");
        assert!(
            has_specialized_candidate(
                &compiled,
                function_body(&compiled, findall_func),
                "findall",
                "isodd"
            ),
            "typed findall(isodd, Vector{{Int32}}) dispatch should include the concrete specialization: {:?}",
            function_body(&compiled, findall_func)
        );
    }
}

mod hof_reduce_minmax_specialization_5094_tests {
    //! Bytecode checks for `reduce(min/max, ::Vector{T})` specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{CompiledProgram, FunctionInfo, Instr, ValueType};

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    #[test]
    fn reduce_minmax_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function reduce_min_i32_5094(xs::Vector{Int32})
        reduce(min, xs)
    end

    function reduce_max_i32_5094(xs::Vector{Int32})
        reduce(max, xs)
    end

    function reduce_min_init_i32_5094(xs::Vector{Int32})
        reduce(min, xs; init=Int32(-5))
    end

    function reduce_max_init_i32_5094(xs::Vector{Int32})
        reduce(max, xs; init=Int32(9))
    end

    reduce_min_i32_5094(Int32[-3, 0, 4, 2])
    reduce_max_i32_5094(Int32[-3, 0, 4, 2])
    reduce_min_init_i32_5094(Int32[0, 4, 2])
    reduce_max_init_i32_5094(Int32[-3, 0, 2])
    "#,
        );

        let min_i32_func = get_function(&compiled, "reduce_min_i32_5094");
        assert_eq!(min_i32_func.return_type, ValueType::I32);
        let min_body = function_body(&compiled, min_i32_func);
        let has_min_i32_specialized_reduce_candidate = has_resolved_or_typed_candidate(
            &compiled,
            min_body,
            "reduce",
            2,
            &["typeof(min)", "Vector{Int32}"],
        );
        assert!(
            has_min_i32_specialized_reduce_candidate,
            "typed reduce(min, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            min_body,
            resolved_target_debug(&compiled, min_body)
        );

        let max_i32_func = get_function(&compiled, "reduce_max_i32_5094");
        assert_eq!(max_i32_func.return_type, ValueType::I32);
        let max_body = function_body(&compiled, max_i32_func);
        let has_max_i32_specialized_reduce_candidate = has_resolved_or_typed_candidate(
            &compiled,
            max_body,
            "reduce",
            2,
            &["typeof(max)", "Vector{Int32}"],
        );
        assert!(
            has_max_i32_specialized_reduce_candidate,
            "typed reduce(max, Vector{{Int32}}) dispatch should resolve to the concrete specialization: {:?}; resolved targets: {:?}",
            max_body,
            resolved_target_debug(&compiled, max_body)
        );

        let min_init_i32_func = get_function(&compiled, "reduce_min_init_i32_5094");
        assert_eq!(min_init_i32_func.return_type, ValueType::I32);
        let has_min_init_i32_resolved_reduce = function_body(&compiled, min_init_i32_func)
            .iter()
            .any(|instr| matches!(instr, Instr::CallResolved(_, 3)));
        assert!(
            has_min_init_i32_resolved_reduce,
            "keyword-init reduce(min, Vector{{Int32}}; init=Int32) should resolve to a concrete method call: {:?}",
            function_body(&compiled, min_init_i32_func)
        );

        let max_init_i32_func = get_function(&compiled, "reduce_max_init_i32_5094");
        assert_eq!(max_init_i32_func.return_type, ValueType::I32);
        let has_max_init_i32_resolved_reduce = function_body(&compiled, max_init_i32_func)
            .iter()
            .any(|instr| matches!(instr, Instr::CallResolved(_, 3)));
        assert!(
            has_max_init_i32_resolved_reduce,
            "keyword-init reduce(max, Vector{{Int32}}; init=Int32) should resolve to a concrete method call: {:?}",
            function_body(&compiled, max_init_i32_func)
        );
    }
}

mod hof_unary_map_specialization_5094_tests {
    //! Bytecode checks for unary HOF callee/element-type specialization (Issue #5094).

    use crate::common::{has_resolved_or_typed_candidate, resolved_target_debug};
    use subset_julia_vm::base;
    use subset_julia_vm::compile::host_support::compile_core_program;
    use subset_julia_vm::ir::core::Program;
    use subset_julia_vm::lowering::Lowering;
    use subset_julia_vm::parser::Parser;
    use subset_julia_vm_bytecode::{
        ArrayElementType, CompiledProgram, FunctionInfo, Instr, ValueType,
    };

    fn compile_source_with_base(source: &str) -> CompiledProgram {
        let prelude_src = base::get_base();
        let mut parser = Parser::new().expect("create parser");
        let prelude_parsed = parser.parse(&prelude_src).expect("parse base");
        let mut prelude_lowering = Lowering::new(&prelude_src);
        let prelude_program = prelude_lowering.lower(prelude_parsed).expect("lower base");

        let mut parser = Parser::new().expect("create parser");
        let parsed = parser.parse(source).expect("parse source");
        let mut lowering = Lowering::new(source);
        let mut user_program = lowering.lower(parsed).expect("lower source");

        merge_programs(prelude_program, &mut user_program);
        compile_core_program(&user_program).expect("compile failed")
    }

    fn merge_programs(mut prelude: Program, user: &mut Program) {
        prelude.functions.append(&mut user.functions);
        user.functions = prelude.functions;

        prelude.structs.append(&mut user.structs);
        user.structs = prelude.structs;

        prelude.abstract_types.append(&mut user.abstract_types);
        user.abstract_types = prelude.abstract_types;
    }

    fn get_function<'a>(compiled: &'a CompiledProgram, name: &str) -> &'a FunctionInfo {
        compiled
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("function '{}' not found", name))
    }

    fn function_body<'a>(compiled: &'a CompiledProgram, f: &FunctionInfo) -> &'a [Instr] {
        &compiled.code[f.code_start..f.code_end]
    }

    fn assert_specialized_hof_candidate(
        compiled: &CompiledProgram,
        func: &FunctionInfo,
        function: &str,
        expected_signature: &[&str],
        message: &str,
    ) {
        let body = function_body(compiled, func);
        assert!(
            has_resolved_or_typed_candidate(
                compiled,
                body,
                function,
                expected_signature.len(),
                expected_signature
            ),
            "{message}: {:?}; resolved targets: {:?}",
            body,
            resolved_target_debug(compiled, body)
        );
    }

    #[test]
    fn unary_map_abs_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function unary_map_abs_5094(xs::Vector{Int64})
        map(abs, xs)
    end

    function unary_map_abs2_5094(xs::Vector{Int64})
        map(abs2, xs)
    end

    function unary_map_abs_i32_5094(xs::Vector{Int32})
        map(abs, xs)
    end

    function unary_map_identity_i32_5094(xs::Vector{Int32})
        map(identity, xs)
    end

    function unary_map_iszero_i32_5094(xs::Vector{Int32})
        map(iszero, xs)
    end

    function unary_map_isone_i32_5094(xs::Vector{Int32})
        map(isone, xs)
    end

    function unary_map_signbit_i32_5094(xs::Vector{Int32})
        map(signbit, xs)
    end

    function unary_map_iseven_i32_5094(xs::Vector{Int32})
        map(iseven, xs)
    end

    function unary_map_isodd_i32_5094(xs::Vector{Int32})
        map(isodd, xs)
    end

    function unary_map_neg_i32_5094(xs::Vector{Int32})
        map(-, xs)
    end

    unary_map_abs_5094([-3, 0, 4])
    unary_map_abs2_5094([-3, 0, 4])
    unary_map_abs_i32_5094(Int32[-3, 0, 4])
    unary_map_identity_i32_5094(Int32[-3, 0, 4])
    unary_map_iszero_i32_5094(Int32[-3, 0, 4])
    unary_map_isone_i32_5094(Int32[0, 1, 2])
    unary_map_signbit_i32_5094(Int32[-3, 0, 4])
    unary_map_iseven_i32_5094(Int32[-3, 0, 4])
    unary_map_isodd_i32_5094(Int32[-3, 0, 4])
    unary_map_neg_i32_5094(Int32[-3, 0, 4])
    "#,
        );
        let func = get_function(&compiled, "unary_map_abs_5094");

        assert_eq!(
            func.return_type,
            ValueType::ArrayOf(ArrayElementType::I64, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            func,
            "map",
            &["typeof(abs)", "Vector{Int64}"],
            "typed map(abs, Vector{Int64}) dispatch should resolve to the concrete specialization",
        );

        let abs2_func = get_function(&compiled, "unary_map_abs2_5094");
        assert_eq!(
            abs2_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I64, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            abs2_func,
            "map",
            &["typeof(abs2)", "Vector{Int64}"],
            "typed map(abs2, Vector{Int64}) dispatch should resolve to the concrete specialization",
        );

        let abs_i32_func = get_function(&compiled, "unary_map_abs_i32_5094");
        assert_eq!(
            abs_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            abs_i32_func,
            "map",
            &["typeof(abs)", "Vector{Int32}"],
            "typed map(abs, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let identity_i32_func = get_function(&compiled, "unary_map_identity_i32_5094");
        assert_eq!(
            identity_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            identity_i32_func,
            "map",
            &["typeof(identity)", "Vector{Int32}"],
            "typed map(identity, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let iszero_i32_func = get_function(&compiled, "unary_map_iszero_i32_5094");
        assert_eq!(
            iszero_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::Bool, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            iszero_i32_func,
            "map",
            &["typeof(iszero)", "Vector{Int32}"],
            "typed map(iszero, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let isone_i32_func = get_function(&compiled, "unary_map_isone_i32_5094");
        assert_eq!(
            isone_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::Bool, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            isone_i32_func,
            "map",
            &["typeof(isone)", "Vector{Int32}"],
            "typed map(isone, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let signbit_i32_func = get_function(&compiled, "unary_map_signbit_i32_5094");
        assert_eq!(
            signbit_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::Bool, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            signbit_i32_func,
            "map",
            &["typeof(signbit)", "Vector{Int32}"],
            "typed map(signbit, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let iseven_i32_func = get_function(&compiled, "unary_map_iseven_i32_5094");
        assert_eq!(
            iseven_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::Bool, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            iseven_i32_func,
            "map",
            &["typeof(iseven)", "Vector{Int32}"],
            "typed map(iseven, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let isodd_i32_func = get_function(&compiled, "unary_map_isodd_i32_5094");
        assert_eq!(
            isodd_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::Bool, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            isodd_i32_func,
            "map",
            &["typeof(isodd)", "Vector{Int32}"],
            "typed map(isodd, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let neg_i32_func = get_function(&compiled, "unary_map_neg_i32_5094");
        assert_specialized_hof_candidate(
            &compiled,
            neg_i32_func,
            "map",
            &["typeof(-)", "Vector{Int32}"],
            "typed map(-, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );
    }

    #[test]
    fn filter_parity_vector_uses_typed_specialized_candidate_issue_5094() {
        let compiled = compile_source_with_base(
            r#"
    function filter_iseven_i32_5094(xs::Vector{Int32})
        filter(iseven, xs)
    end

    function filter_isodd_i32_5094(xs::Vector{Int32})
        filter(isodd, xs)
    end

    function filter_iszero_i32_5094(xs::Vector{Int32})
        filter(iszero, xs)
    end

    function filter_isone_i32_5094(xs::Vector{Int32})
        filter(isone, xs)
    end

    function filter_signbit_i32_5094(xs::Vector{Int32})
        filter(signbit, xs)
    end

    filter_iseven_i32_5094(Int32[-3, 0, 4, 5])
    filter_isodd_i32_5094(Int32[-3, 0, 4, 5])
    filter_iszero_i32_5094(Int32[-3, 0, 4, 5])
    filter_isone_i32_5094(Int32[-3, 0, 1, 5])
    filter_signbit_i32_5094(Int32[-3, 0, 4, 5])
    "#,
        );

        let iseven_i32_func = get_function(&compiled, "filter_iseven_i32_5094");
        assert_eq!(
            iseven_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            iseven_i32_func,
            "filter",
            &["typeof(iseven)", "Vector{Int32}"],
            "typed filter(iseven, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let isodd_i32_func = get_function(&compiled, "filter_isodd_i32_5094");
        assert_eq!(
            isodd_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            isodd_i32_func,
            "filter",
            &["typeof(isodd)", "Vector{Int32}"],
            "typed filter(isodd, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let iszero_i32_func = get_function(&compiled, "filter_iszero_i32_5094");
        assert_eq!(
            iszero_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            iszero_i32_func,
            "filter",
            &["typeof(iszero)", "Vector{Int32}"],
            "typed filter(iszero, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let isone_i32_func = get_function(&compiled, "filter_isone_i32_5094");
        assert_eq!(
            isone_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            isone_i32_func,
            "filter",
            &["typeof(isone)", "Vector{Int32}"],
            "typed filter(isone, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );

        let signbit_i32_func = get_function(&compiled, "filter_signbit_i32_5094");
        assert_eq!(
            signbit_i32_func.return_type,
            ValueType::ArrayOf(ArrayElementType::I32, None)
        );
        assert_specialized_hof_candidate(
            &compiled,
            signbit_i32_func,
            "filter",
            &["typeof(signbit)", "Vector{Int32}"],
            "typed filter(signbit, Vector{Int32}) dispatch should resolve to the concrete specialization",
        );
    }
}
