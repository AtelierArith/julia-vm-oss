//! Integration tests for bytecode to AoT compilation flow.
//!
//! These tests verify the complete pipeline:
//! 1. Julia source → bytecode file (.sjbc)
//! 2. bytecode file → AoT IR → Rust code

use tempfile::tempdir;

use subset_julia_vm::base;
use subset_julia_vm::bytecode;
use subset_julia_vm::ir::core::Program;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;

/// Helper: parse and lower Julia source to Core IR
fn compile_source_to_ir(source: &str) -> Program {
    let mut parser = Parser::new().expect("create parser");

    // Parse and lower prelude
    let prelude_src = base::get_prelude();
    let prelude_outcome = parser.parse(&prelude_src).expect("parse prelude");
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering
        .lower(prelude_outcome)
        .expect("lower prelude");

    // Parse and lower user source
    let outcome = parser.parse(source).expect("parse source");
    let mut lowering = Lowering::new(source);
    let mut program = lowering.lower(outcome).expect("lower source");

    // Merge prelude with user program
    use std::collections::HashSet;
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    // Merge structs
    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.append(&mut program.structs);
    program.structs = all_structs;

    // Merge abstract types
    let user_abstract_names: HashSet<_> = program
        .abstract_types
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    let mut all_abstract_types: Vec<_> = prelude_program
        .abstract_types
        .into_iter()
        .filter(|a| !user_abstract_names.contains(a.name.as_str()))
        .collect();
    all_abstract_types.append(&mut program.abstract_types);
    program.abstract_types = all_abstract_types;

    // Merge functions
    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_func_names.contains(f.name.as_str()))
        .collect();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;

    program
}

#[test]
fn test_save_and_load_bytecode_file() {
    let dir = tempdir().expect("create temp dir");
    let bytecode_path = dir.path().join("test.sjbc");

    // Compile simple program
    let source = r#"
        function add(x, y)
            x + y
        end
        add(1, 2)
    "#;
    let program = compile_source_to_ir(source);

    // Save to bytecode file
    bytecode::save(&program, &bytecode_path).expect("save bytecode");

    // Verify file exists
    assert!(bytecode_path.exists());

    // Load from bytecode file
    let loaded = bytecode::load(&bytecode_path).expect("load bytecode");

    // Verify loaded program matches
    assert_eq!(program.functions.len(), loaded.functions.len());
    assert_eq!(program.structs.len(), loaded.structs.len());
}

#[test]
fn test_bytecode_to_bytes_roundtrip() {
    let source = r#"
        struct Point
            x::Float64
            y::Float64
        end

        function distance(p1::Point, p2::Point)
            dx = p1.x - p2.x
            dy = p1.y - p2.y
            sqrt(dx * dx + dy * dy)
        end

        p1 = Point(0.0, 0.0)
        p2 = Point(3.0, 4.0)
        distance(p1, p2)
    "#;
    let program = compile_source_to_ir(source);

    // Save to bytes
    let bytes = bytecode::save_to_bytes(&program).expect("save to bytes");

    // Load from bytes
    let loaded = bytecode::load_from_bytes(&bytes).expect("load from bytes");

    // Verify program structure
    assert_eq!(program.structs.len(), loaded.structs.len());
    assert_eq!(program.functions.len(), loaded.functions.len());
}

#[test]
fn test_bytecode_magic_bytes() {
    let source = "1 + 1";
    let program = compile_source_to_ir(source);

    let bytes = bytecode::save_to_bytes(&program).expect("save to bytes");

    // Check magic bytes
    assert_eq!(&bytes[0..4], b"SJBC");
}

#[test]
fn test_bytecode_version() {
    let source = "1 + 1";
    let program = compile_source_to_ir(source);

    let bytes = bytecode::save_to_bytes(&program).expect("save to bytes");

    // Check version (little-endian u32 at offset 4)
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    assert_eq!(version, bytecode::VERSION);
}

#[test]
fn test_complex_program_roundtrip() {
    let source = r#"
        # Multiple functions with different return types
        function square(x)
            x * x
        end

        function cube(x)
            x * x * x
        end

        function is_positive(x)
            x > 0
        end

        # Arrays
        arr = [1, 2, 3, 4, 5]
        squared = zeros(5)
        for i in 1:5
            squared[i] = square(arr[i])
        end

        # Control flow
        result = 0
        for x in arr
            if is_positive(x)
                result = result + cube(x)
            end
        end

        result
    "#;
    let program = compile_source_to_ir(source);

    // Roundtrip through bytes
    let bytes = bytecode::save_to_bytes(&program).expect("save to bytes");
    let loaded = bytecode::load_from_bytes(&bytes).expect("load from bytes");

    // Find user functions
    let user_funcs: Vec<_> = program
        .functions
        .iter()
        .filter(|f| f.name == "square" || f.name == "cube" || f.name == "is_positive")
        .collect();

    let loaded_user_funcs: Vec<_> = loaded
        .functions
        .iter()
        .filter(|f| f.name == "square" || f.name == "cube" || f.name == "is_positive")
        .collect();

    assert_eq!(user_funcs.len(), loaded_user_funcs.len());
}

#[cfg(feature = "aot")]
mod aot_tests {
    use super::*;
    use subset_julia_vm::aot::analyze::{bytecode_file_to_aot_ir, load_bytecode_bytes};
    use subset_julia_vm::aot::codegen::aot_codegen::AotCodeGenerator;
    use subset_julia_vm::aot::codegen::CodegenConfig;

    #[test]
    fn test_bytecode_to_aot_ir() {
        let source = r#"
            function add(x::Int64, y::Int64)
                x + y
            end
            add(1, 2)
        "#;
        let program = compile_source_to_ir(source);

        // Save to bytes
        let bytes = bytecode::save_to_bytes(&program).expect("save to bytes");

        // Load using AoT analyze function
        let loaded = load_bytecode_bytes(&bytes).expect("load bytecode");
        assert_eq!(program.functions.len(), loaded.functions.len());
    }

    #[test]
    fn test_bytecode_file_to_aot_ir() {
        let dir = tempdir().expect("create temp dir");
        let bytecode_path = dir.path().join("test.sjbc");

        let source = r#"
            function multiply(a::Float64, b::Float64)
                a * b
            end
            multiply(2.0, 3.0)
        "#;
        let program = compile_source_to_ir(source);

        // Save to file
        bytecode::save(&program, &bytecode_path).expect("save bytecode");

        // Convert directly to AoT IR
        let aot_program = bytecode_file_to_aot_ir(&bytecode_path).expect("convert to AoT IR");

        // Verify we got some functions
        assert!(!aot_program.functions.is_empty());
    }

    #[test]
    fn test_full_pipeline_source_to_rust() {
        let dir = tempdir().expect("create temp dir");
        let bytecode_path = dir.path().join("test.sjbc");

        let source = r#"
            function factorial(n)
                if n <= 1
                    return 1
                end
                return n * factorial(n - 1)
            end
            factorial(5)
        "#;
        let program = compile_source_to_ir(source);

        // Save to bytecode
        bytecode::save(&program, &bytecode_path).expect("save bytecode");

        // Load and convert to AoT IR
        let aot_program = bytecode_file_to_aot_ir(&bytecode_path).expect("convert to AoT IR");

        // Generate Rust code
        let config = CodegenConfig::default();
        let mut codegen = AotCodeGenerator::new(config);
        let rust_code = codegen
            .generate_program(&aot_program)
            .expect("generate Rust");

        // Verify Rust code was generated
        assert!(!rust_code.is_empty());
        assert!(rust_code.contains("fn ")); // Should have function definitions
    }
}
