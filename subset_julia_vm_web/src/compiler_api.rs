mod result;

use self::result::{
    diagnostic, diagnostic_from_error, failure, failure_with, CompileToWasmResult,
    CompilerDiagnostic, PhaseTimings, ResolvedWasmImport,
};
use serde::Deserialize;
use subset_julia_vm::aot::codegen::CAbiExport;
use subset_julia_vm::aot::optimizer::OptLevel;
use subset_julia_vm::aot::types::StaticType;
use subset_julia_vm::aot::{
    compile_wasm_source, AotBackend, CompileConfig, WasmImport, SCRIPT_ENTRY_NAME,
};
use wasm_bindgen::prelude::*;

pub const MAX_SOURCE_BYTES: usize = 1_048_576;
const MAX_MODULE_BYTES: usize = 16_777_216;

#[derive(Debug, Default, Deserialize)]
pub struct CompileOptions {
    source_name: Option<String>,
    opt_level: Option<u8>,
    entry_mode: Option<String>,
    #[serde(default)]
    exports: Vec<CompileExport>,
    #[serde(default)]
    imports: Vec<CompileImport>,
}

#[derive(Debug, Deserialize)]
struct CompileExport {
    export_name: String,
    function_name: String,
    #[serde(default)]
    arg_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CompileImport {
    module: String,
    name: String,
    function_name: String,
    #[serde(default)]
    params: Vec<String>,
    result: Option<String>,
}

#[wasm_bindgen(typescript_custom_section)]
const TYPESCRIPT_TYPES: &str = r#"
export interface CompilerSpan { start: number; end: number; start_line: number; start_column: number; end_line: number; end_column: number; }
export interface CompilerDiagnostic { code: string; kind: string; message: string; span?: CompilerSpan; workaround?: string; }
export interface CompileExport { export_name: string; function_name: string; arg_types?: string[]; }
export interface CompileImport { module: string; name: string; function_name: string; params?: string[]; result?: string; }
export interface ResolvedWasmImport { module: string; name: string; function_name: string; params: string[]; result?: string; }
export interface CompileOptions { source_name?: string; opt_level?: 0 | 1 | 2 | 3; entry_mode?: "exports" | "script"; exports?: CompileExport[]; imports?: CompileImport[]; }
export interface PhaseTimings { source_parse_lower_ms: number; dead_code_elimination_ms: number; type_inference_ms: number; ir_conversion_ms: number; optimization_ms: number; wasm_ir_lowering_ms: number; wasm_codegen_ms: number; total_ms: number; }
export interface CompileToWasmResult { success: boolean; wasm_bytes: Uint8Array; diagnostics: CompilerDiagnostic[]; compiler_version: string; abi_version: number; entry_point?: string; imports: ResolvedWasmImport[]; phase_timings: PhaseTimings; }
export function compile_to_wasm(source: string, options?: CompileOptions): CompileToWasmResult;
"#;

#[wasm_bindgen(skip_typescript)]
pub fn compile_to_wasm(source: &str, options: Option<JsValue>) -> JsValue {
    let options = match options {
        Some(value) => match serde_wasm_bindgen::from_value(value) {
            Ok(options) => options,
            Err(error) => {
                return serialize_result(failure("invalid_options", "options", error.to_string()))
            }
        },
        None => CompileOptions::default(),
    };
    serialize_result(compile_to_wasm_internal(source, options))
}

pub fn compile_to_wasm_internal(source: &str, options: CompileOptions) -> CompileToWasmResult {
    if source.len() > MAX_SOURCE_BYTES {
        return failure(
            "source_too_large",
            "limit",
            format!(
                "source is {} bytes; maximum is {MAX_SOURCE_BYTES}",
                source.len()
            ),
        );
    }
    let (config, entry_point) = match compile_config(options) {
        Ok(config) => config,
        Err(diagnostic) => return failure_with(diagnostic),
    };
    let imports = config
        .wasm_imports
        .iter()
        .map(|import| ResolvedWasmImport {
            module: import.module.clone(),
            name: import.name.clone(),
            function_name: import.function_name.clone(),
            params: import
                .params
                .iter()
                .map(StaticType::julia_type_name)
                .collect(),
            result: import.result.as_ref().map(StaticType::julia_type_name),
        })
        .collect();
    match compile_wasm_source(source, &config) {
        Ok(output) if output.wasm_bytes.len() <= MAX_MODULE_BYTES => CompileToWasmResult {
            success: true,
            wasm_bytes: output.wasm_bytes,
            diagnostics: Vec::new(),
            compiler_version: result::COMPILER_VERSION,
            abi_version: result::WASM_ABI_VERSION,
            entry_point,
            imports,
            phase_timings: PhaseTimings::from_raw(&output.timings),
        },
        Ok(output) => failure(
            "module_too_large",
            "limit",
            format!(
                "module is {} bytes; maximum is {MAX_MODULE_BYTES}",
                output.wasm_bytes.len()
            ),
        ),
        Err(error) => failure_with(diagnostic_from_error(error)),
    }
}

fn compile_config(
    options: CompileOptions,
) -> Result<(CompileConfig, Option<String>), CompilerDiagnostic> {
    let opt_level = match options.opt_level.unwrap_or(2) {
        0 => OptLevel::O0,
        1 => OptLevel::O1,
        2 => OptLevel::O2,
        3 => OptLevel::O3,
        value => {
            return Err(diagnostic(
                "invalid_opt_level",
                "options",
                format!("invalid optimization level {value}; expected 0 through 3"),
            ))
        }
    };
    let mut exports = Vec::with_capacity(options.exports.len());
    for export in options.exports {
        let mut arg_types = Vec::with_capacity(export.arg_types.len());
        for name in export.arg_types {
            let Some(arg_type) = StaticType::from_julia_name_lossy(&name) else {
                return Err(diagnostic(
                    "invalid_argument_type",
                    "options",
                    format!("unsupported export argument type: {name}"),
                ));
            };
            arg_types.push(arg_type);
        }
        exports.push(CAbiExport::with_arg_types(
            export.export_name,
            export.function_name,
            arg_types,
        ));
    }
    let mut imports = Vec::with_capacity(options.imports.len());
    for import in options.imports {
        if import.module.is_empty() || import.name.is_empty() || import.function_name.is_empty() {
            return Err(diagnostic(
                "invalid_import",
                "options",
                "import module, name, and function_name must be non-empty".to_string(),
            ));
        }
        let mut params = Vec::with_capacity(import.params.len());
        for name in import.params {
            let Some(param) = StaticType::from_julia_name_lossy(&name) else {
                return Err(diagnostic(
                    "invalid_import_type",
                    "options",
                    format!("unsupported import parameter type: {name}"),
                ));
            };
            params.push(param);
        }
        let result = import
            .result
            .map(|name| {
                StaticType::from_julia_name_lossy(&name).ok_or_else(|| {
                    diagnostic(
                        "invalid_import_type",
                        "options",
                        format!("unsupported import result type: {name}"),
                    )
                })
            })
            .transpose()?;
        imports.push(WasmImport {
            module: import.module,
            name: import.name,
            function_name: import.function_name,
            params,
            result,
        });
    }
    let entry_point = match options.entry_mode.as_deref().unwrap_or("exports") {
        "exports" => None,
        "script" => Some(SCRIPT_ENTRY_NAME.to_string()),
        value => {
            return Err(diagnostic(
                "invalid_entry_mode",
                "options",
                format!("invalid entry mode {value}; expected exports or script"),
            ))
        }
    };
    let mut config = CompileConfig {
        source_name: options
            .source_name
            .unwrap_or_else(|| "<browser>".to_string()),
        backend: AotBackend::Wasm,
        opt_level,
        c_abi_exports: exports,
        wasm_imports: imports,
        ..CompileConfig::default()
    };
    if entry_point.is_some() {
        config.enable_script_entry();
    }
    Ok((config, entry_point))
}

fn serialize_result(result: CompileToWasmResult) -> JsValue {
    serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
}

#[cfg(test)]
impl CompileOptions {
    pub fn for_test_export(function_name: &str, arg_types: &[&str]) -> Self {
        Self {
            exports: vec![CompileExport {
                export_name: function_name.to_string(),
                function_name: function_name.to_string(),
                arg_types: arg_types.iter().map(|name| (*name).to_string()).collect(),
            }],
            ..Self::default()
        }
    }

    pub fn for_test_import(
        function_name: &str,
        module: &str,
        name: &str,
        params: &[&str],
        result: Option<&str>,
    ) -> Self {
        Self {
            imports: vec![CompileImport {
                module: module.to_string(),
                name: name.to_string(),
                function_name: function_name.to_string(),
                params: params.iter().map(|value| (*value).to_string()).collect(),
                result: result.map(str::to_string),
            }],
            ..Self::for_test_export("answer", &["Int64"])
        }
    }

    pub fn for_test_script() -> Self {
        Self {
            entry_mode: Some("script".to_string()),
            ..Self::default()
        }
    }

    pub fn for_test_entry_mode(entry_mode: &str) -> Self {
        Self {
            entry_mode: Some(entry_mode.to_string()),
            ..Self::default()
        }
    }

    pub fn for_test_image_script() -> Self {
        Self {
            entry_mode: Some("script".to_string()),
            imports: vec![
                CompileImport {
                    module: "sjulia_host".to_string(),
                    name: "load".to_string(),
                    function_name: "__sjulia_host_load".to_string(),
                    params: vec![
                        "String".to_string(),
                        "Int64".to_string(),
                        "Int64".to_string(),
                    ],
                    result: Some("Int64".to_string()),
                },
                CompileImport {
                    module: "sjulia_host".to_string(),
                    name: "save".to_string(),
                    function_name: "__sjulia_host_save".to_string(),
                    params: vec!["String".to_string(), "Int64".to_string()],
                    result: Some("Int64".to_string()),
                },
            ],
            ..Self::default()
        }
    }
}
