use super::compiler_api::{compile_to_wasm_internal, CompileOptions};

#[test]
fn compile_to_wasm_returns_module_bytes_and_exact_phase_timings() {
    // Given: a supported typed arithmetic function and its explicit host export.
    let source = "add_scale(x::Int64, y::Int64) = (x + y) * 2";
    let options = CompileOptions::for_test_export("add_scale", &["Int64", "Int64"]);

    // When: the browser compiler boundary invokes the canonical Wasm backend.
    let result = compile_to_wasm_internal(source, options);

    // Then: it returns a standalone module and every canonical phase timing.
    assert!(
        result.success,
        "unexpected diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(&result.wasm_bytes[..4], b"\0asm");
    assert_eq!(
        result.phase_timings.names(),
        [
            "source-parse-lower",
            "dead-code-elimination",
            "type-inference",
            "ir-conversion",
            "optimization",
            "wasm-ir-lowering",
            "wasm-codegen",
        ]
    );
}

#[test]
fn compile_to_wasm_reports_source_limit_without_panicking() {
    // Given: source one byte larger than the public compiler limit.
    let source = "x".repeat(super::compiler_api::MAX_SOURCE_BYTES + 1);

    // When: compilation is requested.
    let result = compile_to_wasm_internal(&source, CompileOptions::default());

    // Then: a typed limit diagnostic is returned without compiler execution.
    assert!(!result.success);
    assert!(result.wasm_bytes.is_empty());
    assert_eq!(result.diagnostics[0].code, "source_too_large");
    assert_eq!(result.diagnostics[0].kind, "limit");
}

#[test]
fn compile_to_wasm_reports_source_located_parse_diagnostic() {
    // Given: invalid Julia syntax on the second line.
    let source = "x = 1\ny = )\n";

    // When: compilation is requested.
    let result = compile_to_wasm_internal(source, CompileOptions::default());

    // Then: the parser failure remains typed and source-located.
    assert!(!result.success);
    assert_eq!(result.diagnostics[0].kind, "parse");
    let Some(span) = result.diagnostics[0].span.as_ref() else {
        assert!(false, "parse diagnostic should retain a source span");
        return;
    };
    assert_eq!(span.start_line, 2);
    assert!(span.start_column > 0);
}

#[test]
fn compile_to_wasm_supports_string_views() {
    // Given: a statically typed String identity exported through the Wasm ABI.
    let source = "string_identity(value::String)::String = value";
    let options = CompileOptions::for_test_export("string_identity", &["String"]);

    // When: browser compilation lowers the immutable String view.
    let result = compile_to_wasm_internal(source, options);

    // Then: the generated module is valid instead of reporting String as unsupported.
    assert!(
        result.success,
        "unexpected diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(&result.wasm_bytes[..4], b"\0asm");
}

#[test]
fn compile_to_wasm_rejects_dynamic_string_interpolation() {
    // Given: interpolation that requires unsupported dynamic String construction.
    let source = "interpolate(value::Int64)::String = \"value = $value\"";
    let options = CompileOptions::for_test_export("interpolate", &["Int64"]);

    // When: browser compilation reaches dynamic String lowering.
    let result = compile_to_wasm_internal(source, options);

    // Then: the rejection remains a typed diagnostic, not a panic or fallback module.
    assert!(!result.success);
    assert!(result.wasm_bytes.is_empty());
    assert_eq!(result.diagnostics[0].kind, "unsupported");
}

#[test]
fn compile_to_wasm_returns_resolved_import_metadata() {
    let source = r#"
host_scale(value::Int64)::Int64 = value
answer(value::Int64)::Int64 = host_scale(value) + 2
"#;
    let options = CompileOptions::for_test_import(
        "host_scale",
        "sjulia_host",
        "scale",
        &["Int64"],
        Some("Int64"),
    );
    let result = compile_to_wasm_internal(source, options);
    assert!(
        result.success,
        "unexpected diagnostics: {:?}",
        result.diagnostics
    );
    assert_eq!(result.imports.len(), 1);
    assert_eq!(result.imports[0].module, "sjulia_host");
    assert_eq!(result.imports[0].name, "scale");
    assert_eq!(result.imports[0].function_name, "host_scale");
    assert_eq!(result.imports[0].params, ["Int64"]);
    assert_eq!(result.imports[0].result.as_deref(), Some("Int64"));
}
