use serde::Serialize;
use subset_julia_vm::aot::codegen::wasm;
use subset_julia_vm::aot::AotError;

pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const WASM_ABI_VERSION: i32 = wasm::WASM_ABI_VERSION;

#[derive(Debug, Serialize)]
pub struct CompilerDiagnostic {
    pub code: &'static str,
    pub kind: &'static str,
    pub message: String,
    pub span: Option<CompilerSpan>,
    pub workaround: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CompilerSpan {
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct PhaseTimings {
    pub source_parse_lower_ms: f64,
    pub dead_code_elimination_ms: f64,
    pub type_inference_ms: f64,
    pub ir_conversion_ms: f64,
    pub optimization_ms: f64,
    pub wasm_ir_lowering_ms: f64,
    pub wasm_codegen_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct CompileToWasmResult {
    pub success: bool,
    #[serde(with = "serde_bytes")]
    pub wasm_bytes: Vec<u8>,
    pub diagnostics: Vec<CompilerDiagnostic>,
    pub compiler_version: &'static str,
    pub abi_version: i32,
    pub entry_point: Option<String>,
    pub imports: Vec<ResolvedWasmImport>,
    pub phase_timings: PhaseTimings,
}

#[derive(Debug, Serialize)]
pub struct ResolvedWasmImport {
    pub module: String,
    pub name: String,
    pub function_name: String,
    pub params: Vec<String>,
    pub result: Option<String>,
}

pub fn diagnostic_from_error(error: AotError) -> CompilerDiagnostic {
    let span = error.span().map(CompilerSpan::from);
    let (code, kind, message, workaround) = match error {
        AotError::ParseError { message, .. } => ("parse_error", "parse", message, None),
        AotError::LoweringError { message, .. } => ("lowering_error", "lowering", message, None),
        AotError::UnsupportedInstruction(value) => (
            "unsupported_instruction",
            "unsupported",
            value.message,
            value.workaround,
        ),
        AotError::TypeInferenceError(message) => {
            ("type_inference_error", "type-inference", message, None)
        }
        AotError::CodegenError(message) => ("codegen_error", "codegen", message, None),
        AotError::OptimizationError(message) => {
            ("optimization_error", "optimization", message, None)
        }
        AotError::InvalidIR(message) => ("invalid_ir", "invalid-ir", message, None),
        AotError::InternalError(message) => ("internal_error", "internal", message, None),
        AotError::ConversionError(message) => ("conversion_error", "conversion", message, None),
    };
    CompilerDiagnostic {
        code,
        kind,
        message,
        span,
        workaround,
    }
}

pub fn diagnostic(code: &'static str, kind: &'static str, message: String) -> CompilerDiagnostic {
    CompilerDiagnostic {
        code,
        kind,
        message,
        span: None,
        workaround: None,
    }
}

pub fn failure(code: &'static str, kind: &'static str, message: String) -> CompileToWasmResult {
    failure_with(diagnostic(code, kind, message))
}

pub fn failure_with(diagnostic: CompilerDiagnostic) -> CompileToWasmResult {
    CompileToWasmResult {
        success: false,
        wasm_bytes: Vec::new(),
        diagnostics: vec![diagnostic],
        compiler_version: COMPILER_VERSION,
        abi_version: WASM_ABI_VERSION,
        entry_point: None,
        imports: Vec::new(),
        phase_timings: PhaseTimings::default(),
    }
}

impl From<subset_julia_vm::span::Span> for CompilerSpan {
    fn from(span: subset_julia_vm::span::Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
            start_line: span.start_line,
            start_column: span.start_column,
            end_line: span.end_line,
            end_column: span.end_column,
        }
    }
}

impl PhaseTimings {
    pub fn from_raw(raw: &[(&'static str, std::time::Duration)]) -> Self {
        let mut timings = Self::default();
        for (name, duration) in raw {
            let milliseconds = duration.as_secs_f64() * 1000.0;
            timings.total_ms += milliseconds;
            match *name {
                "source-parse-lower" => timings.source_parse_lower_ms = milliseconds,
                "dead-code-elimination" => timings.dead_code_elimination_ms = milliseconds,
                "type-inference" => timings.type_inference_ms = milliseconds,
                "ir-conversion" => timings.ir_conversion_ms = milliseconds,
                "optimization" => timings.optimization_ms = milliseconds,
                "wasm-ir-lowering" => timings.wasm_ir_lowering_ms = milliseconds,
                "wasm-codegen" => timings.wasm_codegen_ms = milliseconds,
                _ => {}
            }
        }
        timings
    }

    #[cfg(test)]
    pub fn names(&self) -> [&'static str; 7] {
        [
            "source-parse-lower",
            "dead-code-elimination",
            "type-inference",
            "ir-conversion",
            "optimization",
            "wasm-ir-lowering",
            "wasm-codegen",
        ]
    }
}
