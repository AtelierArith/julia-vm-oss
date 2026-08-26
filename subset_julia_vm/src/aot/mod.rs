//! AoT (Ahead-of-Time) Compiler Module
//!
//! This module provides the AoT compilation infrastructure for SubsetJuliaVM.
//! It transpiles lowered Core IR into Rust code for native execution.
//!
//! # Architecture
//!
//! ```text
//! Core IR → Analyze → IR → Optimize → Codegen → Rust Code
//! ```
//!
//! # Compilation Levels
//!
//! The compiler supports different type inference levels:
//! - Level 0: Fully static types
//! - Level 1: Inferred types with guards
//! - Level 2: Conditional dispatch
//! - Level 3: Dynamic dispatch (fallback to runtime)

use crate::span::Span;
use std::fmt;
use thiserror::Error;

#[cfg(not(target_arch = "wasm32"))]
struct AotTimer(std::time::Instant);

#[cfg(target_arch = "wasm32")]
struct AotTimer(f64);

#[cfg(target_arch = "wasm32")]
fn wasm_now_ms() -> f64 {
    use wasm_bindgen::{JsCast, JsValue};

    let global = js_sys::global();
    let Ok(performance) = js_sys::Reflect::get(&global, &JsValue::from_str("performance")) else {
        return js_sys::Date::now();
    };
    let Ok(now) = js_sys::Reflect::get(&performance, &JsValue::from_str("now")) else {
        return js_sys::Date::now();
    };
    let Some(now) = now.dyn_ref::<js_sys::Function>() else {
        return js_sys::Date::now();
    };
    now.call0(&performance)
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or_else(js_sys::Date::now)
}

impl AotTimer {
    fn start() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self(std::time::Instant::now())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self(wasm_now_ms())
        }
    }

    fn elapsed(&self) -> std::time::Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.0.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            std::time::Duration::from_secs_f64(((wasm_now_ms() - self.0) / 1000.0).max(0.0))
        }
    }
}

pub mod abi;
pub mod analyze;
pub mod call_graph;
pub mod codegen;
pub mod inference;
pub mod ir;
pub mod linker;
pub mod native_calls;
pub mod optimizer;
pub mod pass_pipeline;
pub mod rooting;
mod script_entry;
pub mod specialization;
pub mod types;

pub use script_entry::SCRIPT_ENTRY_NAME;

/// Code-generation backend selected for the AoT pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AotBackend {
    /// Generate Rust source code through the high-level AoT Rust backend.
    #[default]
    Rust,
    /// Compile through the experimental Cranelift backend when the crate is
    /// built with the `cranelift` feature.
    Cranelift,
    /// Emit a standalone core WebAssembly module when built with `aot-wasm`.
    Wasm,
}

/// Explicit host function imported by a generated WebAssembly module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmImport {
    pub module: String,
    pub name: String,
    pub function_name: String,
    pub params: Vec<types::StaticType>,
    pub result: Option<types::StaticType>,
}

/// User-facing detail for unsupported AoT instructions or semantic boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedInstructionDiagnostic {
    /// Short explanation of what AoT cannot compile.
    pub message: String,
    /// Source span when the unsupported construct still has one attached.
    pub span: Option<Span>,
    /// Suggested workaround or next action.
    pub workaround: Option<String>,
}

impl UnsupportedInstructionDiagnostic {
    /// Create a diagnostic without span/workaround context.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            workaround: None,
        }
    }

    /// Attach source span context.
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a suggested workaround.
    pub fn with_workaround(mut self, workaround: impl Into<String>) -> Self {
        self.workaround = Some(workaround.into());
        self
    }
}

impl fmt::Display for UnsupportedInstructionDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(span) = self.span {
            if span.start_line > 0 {
                write!(
                    f,
                    " at line {}, column {}",
                    span.start_line, span.start_column
                )?;
            } else {
                write!(f, " at byte {}..{}", span.start, span.end)?;
            }
        }
        if let Some(workaround) = &self.workaround {
            write!(f, "\nWorkaround: {}", workaround)?;
        }
        Ok(())
    }
}

/// AoT compilation error
#[derive(Debug, Error)]
pub enum AotError {
    /// Source could not be parsed
    #[error("Parse error: {message}")]
    ParseError { message: String, span: Option<Span> },

    /// Lowering to Core IR failed
    #[error("Lowering error: {message}")]
    LoweringError { message: String, span: Option<Span> },

    /// Type inference failed
    #[error("Type inference failed: {0}")]
    TypeInferenceError(String),

    /// Unsupported bytecode instruction
    #[error("Unsupported instruction: {0}")]
    UnsupportedInstruction(UnsupportedInstructionDiagnostic),

    /// Code generation error
    #[error("Code generation error: {0}")]
    CodegenError(String),

    /// Optimization error
    #[error("Optimization error: {0}")]
    OptimizationError(String),

    /// Invalid IR
    #[error("Invalid IR: {0}")]
    InvalidIR(String),

    /// Internal compiler error
    #[error("Internal compiler error: {0}")]
    InternalError(String),

    /// IR conversion error
    #[error("IR conversion error: {0}")]
    ConversionError(String),
}

/// Result type for AoT operations
pub type AotResult<T> = Result<T, AotError>;

impl AotError {
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::ParseError { span, .. } | Self::LoweringError { span, .. } => *span,
            Self::UnsupportedInstruction(diagnostic) => diagnostic.span,
            Self::TypeInferenceError(_)
            | Self::CodegenError(_)
            | Self::OptimizationError(_)
            | Self::InvalidIR(_)
            | Self::InternalError(_)
            | Self::ConversionError(_) => None,
        }
    }
}

/// Statistics collected during AoT compilation
#[derive(Debug, Default, Clone)]
pub struct AotStats {
    /// Number of functions compiled
    pub functions_compiled: usize,
    /// Total number of functions in program (before DCE)
    pub functions_total: usize,
    /// Number of functions eliminated by DCE
    pub functions_eliminated: usize,
    /// Number of instructions processed
    pub instructions_processed: usize,
    /// Number of type inferences performed
    pub type_inferences: usize,
    /// Number of dynamic dispatch fallbacks
    pub dynamic_fallbacks: usize,
    /// Number of optimizations applied
    pub optimizations_applied: usize,
}

impl AotStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge statistics from another compilation
    pub fn merge(&mut self, other: &AotStats) {
        self.functions_compiled += other.functions_compiled;
        self.functions_total += other.functions_total;
        self.functions_eliminated += other.functions_eliminated;
        self.instructions_processed += other.instructions_processed;
        self.type_inferences += other.type_inferences;
        self.dynamic_fallbacks += other.dynamic_fallbacks;
        self.optimizations_applied += other.optimizations_applied;
    }
}

/// Output from AoT compilation
#[derive(Debug)]
pub struct AotOutput {
    /// Generated Rust code
    pub rust_code: String,
    /// Compilation statistics
    pub stats: AotStats,
    /// Warnings generated during compilation
    pub warnings: Vec<String>,
    /// Human-readable descriptions of each residual dynamic-dispatch site,
    /// surfaced by `--stats`/`--check` so users can see what blocked
    /// full static compilation.
    pub dynamic_op_descriptions: Vec<String>,
}

impl AotOutput {
    /// Create a new AoT output
    pub fn new(rust_code: String, stats: AotStats) -> Self {
        Self {
            rust_code,
            stats,
            warnings: Vec::new(),
            dynamic_op_descriptions: Vec::new(),
        }
    }

    /// Add a warning
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Number of lines in the generated Rust source.
    pub fn generated_loc(&self) -> usize {
        self.rust_code.lines().count()
    }

    /// Estimated size of the generated Rust source in bytes.
    pub fn estimated_bytes(&self) -> usize {
        self.rust_code.len()
    }
}

/// Configuration for the canonical Core IR → selected backend pipeline.
#[derive(Debug, Clone)]
pub struct CompileConfig {
    /// Name used for the `// Source:` header comment.
    pub source_name: String,
    /// Code-generation backend.
    pub backend: AotBackend,
    /// Emit debug comments (and the source header) in generated code.
    pub emit_comments: bool,
    /// Emit native debug information where the selected backend supports it.
    pub debug_info: bool,
    /// Require fully static, standalone Rust (no runtime dependency).
    pub pure_rust: bool,
    /// Optimization level applied to the AoT IR.
    pub opt_level: optimizer::OptLevel,
    /// Optional `--dump-aot-stage` selection (stage name or `all`).
    pub dump_stage: Option<String>,
    /// Explicit C ABI entry points to export from generated Rust.
    pub c_abi_exports: Vec<codegen::CAbiExport>,
    /// Explicit function replacements imported by the Wasm backend.
    pub wasm_imports: Vec<WasmImport>,
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            source_name: "<ir>".to_string(),
            backend: AotBackend::Rust,
            emit_comments: false,
            debug_info: false,
            pure_rust: false,
            opt_level: optimizer::OptLevel::default(),
            dump_stage: None,
            c_abi_exports: Vec::new(),
            wasm_imports: Vec::new(),
        }
    }
}

/// Result of [`compile_program`]: the generated output plus any stage dumps
/// requested via [`CompileConfig::dump_stage`].
#[derive(Debug)]
pub struct CompileResult {
    /// The generated Rust and its statistics.
    pub output: AotOutput,
    /// Rendered AoT IR stage dumps (empty unless `dump_stage` was set).
    pub dumps: String,
    /// Per-stage wall-clock timings, in pipeline order (for `--time-passes`).
    pub timings: Vec<(&'static str, std::time::Duration)>,
}

/// Standalone WebAssembly output from the shared AoT preparation pipeline.
#[derive(Debug)]
pub struct WasmCompileResult {
    /// Encoded core WebAssembly module bytes.
    pub wasm_bytes: Vec<u8>,
    /// Statistics gathered by parse-independent AoT preparation.
    pub stats: AotStats,
    /// Rendered AoT IR stage dumps requested by the compile configuration.
    pub dumps: String,
    /// Per-stage wall-clock timings, including Wasm lowering and codegen.
    pub timings: Vec<(&'static str, std::time::Duration)>,
}

struct PreparedAotProgram {
    aot_program: ir::AotProgram,
    stats: AotStats,
    diagnostics: pass_pipeline::AotPassDiagnostics,
    timings: Vec<(&'static str, std::time::Duration)>,
    dynamic_count: usize,
    dynamic_diagnostics: Vec<String>,
}

/// Canonical AoT pipeline: lowered Core IR → DCE → inference → AoT IR →
/// optimize → Rust codegen. Shared by the `juliars` CLI and
/// [`compile_from_ir_bytes`] so every entry point behaves identically.
pub fn compile_program(
    program: crate::ir::core::Program,
    config: &CompileConfig,
) -> AotResult<CompileResult> {
    let version = env!("CARGO_PKG_VERSION");
    let mut prepared = prepare_aot_program(program, config)?;

    // Codegen
    let t = AotTimer::start();
    prepared.diagnostics.verify_and_record(
        pass_pipeline::AotPassStage::BeforeBackendCodegen,
        &prepared.aot_program,
    )?;
    let mut generated_code = generate_backend_output(&prepared.aot_program, config)?;
    prepared.timings.push(("codegen", t.elapsed()));

    if config.pure_rust {
        let residual = pure_rust_runtime_references(&generated_code);
        if !residual.is_empty() {
            return Err(AotError::CodegenError(format!(
                "Pure Rust output still references subset_julia_vm_runtime:\n{}",
                residual
                    .iter()
                    .map(|line| format!("  - {}", line))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }
    }

    if config.emit_comments {
        generated_code = format!(
            "// Source: {}\n// Generated by SubsetJuliaVM AoT Compiler v{}\n\n{}",
            config.source_name, version, generated_code
        );
    }

    prepared.stats.dynamic_fallbacks = prepared.dynamic_count;
    let mut output = AotOutput::new(generated_code, prepared.stats);
    output.dynamic_op_descriptions = prepared.dynamic_diagnostics;
    if prepared.dynamic_count > 0 {
        output.add_warning(format!(
            "{} function calls will use dynamic dispatch at runtime",
            prepared.dynamic_count
        ));
    }

    Ok(CompileResult {
        output,
        dumps: prepared.diagnostics.render_dumps(),
        timings: prepared.timings,
    })
}

fn prepare_aot_program(
    mut program: crate::ir::core::Program,
    config: &CompileConfig,
) -> AotResult<PreparedAotProgram> {
    use crate::aot::analyze::program_to_aot_ir;
    use crate::aot::call_graph::CallGraph;
    use crate::aot::inference::TypeInferenceEngine;
    use crate::aot::optimizer::optimize_aot_program_at_level_with_options;
    use crate::aot::pass_pipeline::{AotDumpSelection, AotPassDiagnostics, AotPassStage};

    if config.requests_script_entry() {
        script_entry::lift_script_entry(&mut program)?;
    }

    let mut stats = AotStats::new();
    let mut timings: Vec<(&'static str, std::time::Duration)> = Vec::new();
    let selection =
        AotDumpSelection::parse(config.dump_stage.as_deref()).map_err(AotError::InternalError)?;
    let mut diagnostics = AotPassDiagnostics::new(selection);

    // Dead Code Elimination
    let t = AotTimer::start();
    stats.functions_total = program.functions.len();
    let program = if config.c_abi_exports.is_empty() {
        let call_graph = CallGraph::from_program(&program);
        call_graph.filter_program(&program)
    } else {
        // C ABI exports are resolved after inference/AoT conversion and may name
        // generated method symbols such as `add_i64_i64`; keep all functions so
        // DCE cannot delete export candidates before codegen validates them.
        program
    };
    stats.functions_eliminated = stats.functions_total - program.functions.len();
    timings.push(("dead-code-elimination", t.elapsed()));

    // Reverse the #9103 generator-body lift before inference so both inference
    // and IR conversion see the inline generator (Issue #9179). The lift wraps a
    // non-trivial generator body in an expression-position `let` block that AoT
    // inference cannot see through; without this the surrounding binding widens
    // to `Any` and the block trips the #7014 diagnostic during conversion.
    let mut program = program;
    crate::aot::analyze::reverse_generator_lifts_in_program(&mut program);

    // Type inference
    let t = AotTimer::start();
    let mut type_engine = TypeInferenceEngine::new();
    let mut typed_program = type_engine.analyze_program(&program)?;
    stats.functions_compiled = program.functions.len();
    stats.type_inferences = typed_program.function_count();
    timings.push(("type-inference", t.elapsed()));

    apply_wasm_import_declarations(&mut program, &mut typed_program, &config.wasm_imports)?;

    // Convert Core IR to AoT IR
    let t = AotTimer::start();
    let mut aot_program = program_to_aot_ir(&program, &typed_program)?;
    diagnostics.verify_and_record(AotPassStage::AfterAotIrConversion, &aot_program)?;
    stats.instructions_processed = aot_program.instruction_count();
    timings.push(("ir-conversion", t.elapsed()));

    if !config.c_abi_exports.is_empty() {
        let export_roots = c_abi_export_root_names(&aot_program, &config.c_abi_exports);
        aot_program.prune_unreachable_functions_with_roots(&export_roots);
    }

    // Optimize
    let t = AotTimer::start();
    stats.optimizations_applied = optimize_aot_program_at_level_with_options(
        &mut aot_program,
        config.opt_level,
        !config.c_abi_exports.is_empty(),
    );
    diagnostics.verify_and_record(AotPassStage::AfterOptimization, &aot_program)?;
    timings.push(("optimization", t.elapsed()));

    // Residual dynamic dispatch
    let dynamic_count = aot_program.count_dynamic_calls();
    let dynamic_diagnostics: Vec<String> = aot_program
        .diagnose_dynamic_operations()
        .iter()
        .map(|d| d.to_string())
        .collect();

    if config.pure_rust && dynamic_count > 0 {
        return Err(generate_pure_rust_error(&aot_program, dynamic_count));
    }

    Ok(PreparedAotProgram {
        aot_program,
        stats,
        diagnostics,
        timings,
        dynamic_count,
        dynamic_diagnostics,
    })
}

fn apply_wasm_import_declarations(
    program: &mut crate::ir::core::Program,
    typed: &mut inference::TypedProgram,
    imports: &[WasmImport],
) -> AotResult<()> {
    for import in imports {
        let matches: Vec<_> = program
            .functions
            .iter()
            .enumerate()
            .filter_map(|(index, function)| {
                (function.name == import.function_name).then_some(index)
            })
            .collect();
        let [index] = matches.as_slice() else {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "Wasm import `{}.{}` must resolve to exactly one top-level function `{}`; found {}",
                    import.module,
                    import.name,
                    import.function_name,
                    matches.len()
                )),
            ));
        };
        let typed_functions = typed
            .functions
            .get_mut(&import.function_name)
            .ok_or_else(|| {
                AotError::InternalError(format!(
                    "missing inferred signature for Wasm import `{}`",
                    import.function_name
                ))
            })?;
        let [typed_function] = typed_functions.as_mut_slice() else {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "Wasm import `{}.{}` requires one inferred signature for `{}`; found {}",
                    import.module,
                    import.name,
                    import.function_name,
                    typed_functions.len()
                )),
            ));
        };
        if typed_function.signature.param_names.len() != import.params.len() {
            return Err(AotError::UnsupportedInstruction(
                UnsupportedInstructionDiagnostic::new(format!(
                    "Wasm import `{}.{}` parameter count does not match `{}`",
                    import.module, import.name, import.function_name
                )),
            ));
        }
        typed_function.signature = inference::FunctionSignature::new(
            import.function_name.clone(),
            typed_function.signature.param_names.clone(),
            import.params.clone(),
            import.result.clone().unwrap_or(types::StaticType::Nothing),
        );
        let function = std::sync::Arc::make_mut(&mut program.functions[*index]);
        function.body.stmts = vec![crate::ir::core::Stmt::Meta {
            annotation: crate::ir::core::MetaAnnotation {
                name: "noinline".to_string(),
                args: Vec::new(),
            },
            span: function.span,
        }];
    }
    Ok(())
}

fn codegen_config_from_compile_config(config: &CompileConfig) -> codegen::CodegenConfig {
    codegen::CodegenConfig {
        emit_comments: config.emit_comments,
        debug_info: config.debug_info,
        source_name: config.source_name.clone(),
        pure_rust: config.pure_rust,
        opt_level: config.opt_level,
        c_abi_exports: config.c_abi_exports.clone(),
        ..codegen::CodegenConfig::default()
    }
}

fn generate_backend_output(
    aot_program: &ir::AotProgram,
    config: &CompileConfig,
) -> AotResult<String> {
    use crate::aot::codegen::aot_codegen::AotCodeGenerator;

    let codegen_config = codegen_config_from_compile_config(config);
    match config.backend {
        AotBackend::Rust => {
            let mut codegen = AotCodeGenerator::new(codegen_config);
            codegen.generate_program(aot_program)
        }
        AotBackend::Cranelift => generate_cranelift_output(aot_program, codegen_config),
        AotBackend::Wasm => Err(AotError::CodegenError(
            "Wasm output is binary; call compile_wasm instead of compile_program".to_string(),
        )),
    }
}

/// Compile lowered Core IR to a standalone WebAssembly module.
#[cfg(feature = "aot-wasm")]
pub fn compile_wasm(
    program: crate::ir::core::Program,
    config: &CompileConfig,
) -> AotResult<WasmCompileResult> {
    if config.backend != AotBackend::Wasm {
        return Err(AotError::CodegenError(
            "compile_wasm requires CompileConfig.backend = AotBackend::Wasm".to_string(),
        ));
    }
    let mut prepared = prepare_aot_program(program, config)?;
    prepared.diagnostics.verify_and_record(
        pass_pipeline::AotPassStage::BeforeBackendCodegen,
        &prepared.aot_program,
    )?;
    let started = AotTimer::start();
    let module =
        codegen::wasm::lower_program_with_imports(&prepared.aot_program, &config.wasm_imports)?;
    prepared
        .timings
        .push(("wasm-ir-lowering", started.elapsed()));
    let started = AotTimer::start();
    let wasm_bytes =
        codegen::wasm::emit_module(&module, &config.c_abi_exports, &config.wasm_imports)?;
    prepared.timings.push(("wasm-codegen", started.elapsed()));
    Ok(WasmCompileResult {
        wasm_bytes,
        stats: prepared.stats,
        dumps: prepared.diagnostics.render_dumps(),
        timings: prepared.timings,
    })
}

/// Compile Julia source through the canonical parser/lowering pipeline to Wasm.
#[cfg(feature = "aot-wasm")]
pub fn compile_wasm_source(source: &str, config: &CompileConfig) -> AotResult<WasmCompileResult> {
    let started = AotTimer::start();
    let program = crate::pipeline::parse_source(source).map_err(|error| match error {
        crate::pipeline::PipelineError::Parse(error) => {
            let span = match &error {
                crate::error::SyntaxError::ParseFailed(_) => None,
                crate::error::SyntaxError::ErrorNodes(issues) => {
                    issues.first().map(|issue| issue.span)
                }
            };
            AotError::ParseError {
                message: error.to_string(),
                span,
            }
        }
        crate::pipeline::PipelineError::Lower(error) => AotError::LoweringError {
            message: error.to_string(),
            span: Some(error.span),
        },
        crate::pipeline::PipelineError::Load(error) => AotError::InternalError(error.to_string()),
    })?;
    let source_parse_lower = started.elapsed();
    let mut result = compile_wasm(program, config)?;
    result
        .timings
        .insert(0, ("source-parse-lower", source_parse_lower));
    Ok(result)
}

/// Report that source-to-Wasm compilation is unavailable without `aot-wasm`.
#[cfg(not(feature = "aot-wasm"))]
pub fn compile_wasm_source(_source: &str, _config: &CompileConfig) -> AotResult<WasmCompileResult> {
    Err(AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(
            "Wasm AoT output requires the `aot-wasm` Cargo feature",
        )
        .with_workaround("rebuild subset_julia_vm with `--features aot-wasm`"),
    ))
}

/// Report that the Wasm backend was not compiled into this crate.
#[cfg(not(feature = "aot-wasm"))]
pub fn compile_wasm(
    _program: crate::ir::core::Program,
    _config: &CompileConfig,
) -> AotResult<WasmCompileResult> {
    Err(AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(
            "Wasm AoT output requires the `aot-wasm` Cargo feature",
        )
        .with_workaround("rebuild subset_julia_vm with `--features aot-wasm`"),
    ))
}

fn c_abi_export_root_names(
    program: &ir::AotProgram,
    exports: &[codegen::CAbiExport],
) -> Vec<String> {
    let mut roots = std::collections::HashSet::new();
    for request in exports {
        for func in &program.functions {
            let signature_matches = request
                .arg_types
                .as_ref()
                .is_none_or(|arg_types| func.params.iter().map(|(_, ty)| ty).eq(arg_types.iter()));
            if !signature_matches {
                continue;
            }

            let sanitized_name = ir::AotFunction::sanitize_function_name(&func.name);
            if func.name == request.function_name
                || func.mangled_name() == request.function_name
                || sanitized_name == request.function_name
            {
                roots.insert(func.name.clone());
            }
        }
    }
    roots.into_iter().collect()
}

#[cfg(feature = "cranelift")]
#[derive(Debug, Clone)]
struct ResolvedCraneliftCAbiExport {
    export_name: String,
    target_name: String,
    params: Vec<(String, types::StaticType)>,
    return_type: types::StaticType,
}

#[cfg(feature = "cranelift")]
#[derive(Debug, Clone, Default)]
struct CraneliftDebugLines {
    main_line: Option<u32>,
    functions: std::collections::HashMap<String, u32>,
}

#[cfg(feature = "cranelift")]
impl CraneliftDebugLines {
    fn function_line(&self, function_name: &str) -> Option<u32> {
        self.functions.get(function_name).copied().or_else(|| {
            self.functions.iter().find_map(|(source_name, line)| {
                function_name
                    .strip_prefix(source_name)
                    .filter(|suffix| suffix.starts_with('_'))
                    .map(|_| *line)
            })
        })
    }
}

#[cfg(feature = "cranelift")]
fn collect_cranelift_debug_lines(program: &crate::ir::core::Program) -> CraneliftDebugLines {
    let mut lines = CraneliftDebugLines {
        main_line: debug_line_from_span(program.main.span),
        functions: std::collections::HashMap::new(),
    };
    for func in &program.functions {
        if let Some(line) = debug_line_from_span(func.span) {
            lines.functions.insert(func.name.clone(), line);
        }
    }
    lines
}

#[cfg(feature = "cranelift")]
fn debug_line_from_span(span: crate::span::Span) -> Option<u32> {
    u32::try_from(span.start_line).ok().filter(|line| *line > 0)
}

#[cfg(feature = "cranelift")]
fn append_cranelift_c_abi_export_wrappers(
    module: &mut ir::IrModule,
    program: &ir::AotProgram,
    exports: &[codegen::CAbiExport],
) -> AotResult<()> {
    if exports.is_empty() {
        return Ok(());
    }

    let resolved = resolve_cranelift_c_abi_exports(program, exports)?;
    let existing_names: std::collections::HashSet<_> = module
        .functions
        .iter()
        .map(|func| func.name.clone())
        .collect();

    for export in resolved {
        if export.export_name == export.target_name {
            continue;
        }
        if existing_names.contains(&export.export_name) {
            return Err(AotError::CodegenError(format!(
                "C ABI export symbol `{}` conflicts with an existing Cranelift function; use a distinct export name",
                export.export_name
            )));
        }
        module.add_function(cranelift_c_abi_wrapper_function(export)?);
    }

    Ok(())
}

#[cfg(feature = "cranelift")]
fn resolve_cranelift_c_abi_exports(
    program: &ir::AotProgram,
    exports: &[codegen::CAbiExport],
) -> AotResult<Vec<ResolvedCraneliftCAbiExport>> {
    let mut resolved = Vec::new();
    let mut seen_export_names = std::collections::HashSet::new();

    for request in exports {
        if !is_c_symbol_name(&request.export_name) {
            return Err(AotError::CodegenError(format!(
                "C ABI export symbol `{}` is not a valid C symbol name",
                request.export_name
            )));
        }
        if !seen_export_names.insert(request.export_name.clone()) {
            return Err(AotError::CodegenError(format!(
                "duplicate C ABI export symbol `{}`",
                request.export_name
            )));
        }

        let candidates: Vec<_> = program
            .functions
            .iter()
            .filter(|func| {
                let sanitized_name = ir::AotFunction::sanitize_function_name(&func.name);
                let name_matches = func.name == request.function_name
                    || func.mangled_name() == request.function_name
                    || sanitized_name == request.function_name;
                let signature_matches = request.arg_types.as_ref().is_none_or(|arg_types| {
                    func.params.iter().map(|(_, ty)| ty).eq(arg_types.iter())
                });
                name_matches && signature_matches
            })
            .collect::<Vec<_>>();

        let [func] = candidates.as_slice() else {
            return Err(AotError::CodegenError(c_abi_resolution_error(
                request,
                candidates.len(),
            )));
        };

        validate_cranelift_c_abi_export(request, func)?;
        resolved.push(ResolvedCraneliftCAbiExport {
            export_name: request.export_name.clone(),
            target_name: func.name.clone(),
            params: func.params.clone(),
            return_type: func.return_type.clone(),
        });
    }

    Ok(resolved)
}

#[cfg(feature = "cranelift")]
fn c_abi_resolution_error(request: &codegen::CAbiExport, candidate_count: usize) -> String {
    if candidate_count == 0 {
        format!(
            "C ABI export `{}` could not find function `{}`",
            request.export_name, request.function_name
        )
    } else {
        format!(
            "C ABI export `{}` is ambiguous for function `{}`; use `symbol=function(Int64,Float64)` or a generated method name such as `name_i64_i64`",
            request.export_name, request.function_name
        )
    }
}

#[cfg(feature = "cranelift")]
fn validate_cranelift_c_abi_export(
    request: &codegen::CAbiExport,
    func: &ir::AotFunction,
) -> AotResult<()> {
    for (idx, (_, ty)) in func.params.iter().enumerate() {
        if !cranelift_c_abi_type_stable(ty, false) {
            return Err(AotError::CodegenError(format!(
                "C ABI export `{}` for `{}` has non-C-stable parameter {} of type `{}`",
                request.export_name,
                func.name,
                idx + 1,
                ty
            )));
        }
    }

    if !cranelift_c_abi_type_stable(&func.return_type, true) {
        return Err(AotError::CodegenError(format!(
            "C ABI export `{}` for `{}` has non-C-stable return type `{}`",
            request.export_name, func.name, func.return_type
        )));
    }

    Ok(())
}

#[cfg(feature = "cranelift")]
fn cranelift_c_abi_type_stable(ty: &types::StaticType, allow_nothing: bool) -> bool {
    matches!(
        ty,
        types::StaticType::I64
            | types::StaticType::I32
            | types::StaticType::I16
            | types::StaticType::I8
            | types::StaticType::U64
            | types::StaticType::U32
            | types::StaticType::U16
            | types::StaticType::U8
            | types::StaticType::F64
            | types::StaticType::F32
            | types::StaticType::Bool
    ) || (allow_nothing && matches!(ty, types::StaticType::Nothing))
}

#[cfg(feature = "cranelift")]
fn is_c_symbol_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(feature = "cranelift")]
fn cranelift_c_abi_wrapper_function(
    export: ResolvedCraneliftCAbiExport,
) -> AotResult<ir::IrFunction> {
    let mut wrapper = ir::IrFunction::new(
        export.export_name,
        export.params.clone(),
        export.return_type.clone(),
    );
    let args = export
        .params
        .iter()
        .map(|(name, ty)| ir::VarRef::new(name.clone(), ty.clone()))
        .collect::<Vec<_>>();

    // INTERNAL: `IrFunction::new` always creates its entry block (Issue
    // #10907 — previously four repeated panicking `entry_block_mut` lookups
    // relying on this same invariant).
    let entry = wrapper.entry_block_mut().ok_or_else(|| {
        AotError::InternalError(
            "cranelift_c_abi_wrapper_function: IrFunction::new always creates its entry block"
                .to_string(),
        )
    })?;

    if export.return_type == types::StaticType::Nothing {
        entry.push(ir::Instruction::Call {
            dest: None,
            func: export.target_name,
            args,
        });
        entry.set_terminator(ir::Terminator::Return(None));
    } else {
        let result = ir::VarRef::new("__sjulia_cabi_ret".to_string(), export.return_type);
        entry.push(ir::Instruction::Call {
            dest: Some(result.clone()),
            func: export.target_name,
            args,
        });
        entry.set_terminator(ir::Terminator::Return(Some(result)));
    }

    Ok(wrapper)
}

#[cfg(not(feature = "cranelift"))]
fn generate_cranelift_output(
    _aot_program: &ir::AotProgram,
    _codegen_config: codegen::CodegenConfig,
) -> AotResult<String> {
    Err(AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(
            "Cranelift backend was selected, but this `juliars` binary was built without the `cranelift` feature (Issue #6927)",
        )
        .with_workaround(
            "rebuild with `cargo build --release -p subset_julia_vm --features cranelift --bin juliars`, or use `--backend rust`",
        ),
    ))
}

#[cfg(feature = "cranelift")]
fn generate_cranelift_output(
    aot_program: &ir::AotProgram,
    codegen_config: codegen::CodegenConfig,
) -> AotResult<String> {
    use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
    use crate::aot::codegen::CodeGenerator;

    let module = lower_aot_program_for_cranelift(aot_program, None)?;
    let mut codegen = CraneliftCodeGenerator::with_config(codegen_config)
        .map_err(|e| AotError::CodegenError(e.to_string()))?;
    codegen.generate_module(&module)
}

/// Result of an explicit desktop Cranelift JIT run.
#[derive(Debug)]
pub struct CraneliftJitRunResult {
    /// Statistics gathered by the shared AoT preparation pipeline.
    pub stats: AotStats,
    /// Rendered AoT IR stage dumps requested through [`CompileConfig::dump_stage`].
    pub dumps: String,
    /// Per-stage wall-clock timings, including the final JIT main call.
    pub timings: Vec<(&'static str, std::time::Duration)>,
}

/// Result of explicit Cranelift relocatable object emission.
#[derive(Debug)]
pub struct CraneliftObjectResult {
    /// Relocatable object file bytes emitted by `cranelift-object`.
    pub object_bytes: Vec<u8>,
    /// Statistics gathered by the shared AoT preparation pipeline.
    pub stats: AotStats,
    /// Rendered AoT IR stage dumps requested through [`CompileConfig::dump_stage`].
    pub dumps: String,
    /// Per-stage wall-clock timings, including object codegen.
    pub timings: Vec<(&'static str, std::time::Duration)>,
}

#[cfg(not(feature = "cranelift"))]
pub fn compile_cranelift_object(
    _program: crate::ir::core::Program,
    _config: &CompileConfig,
) -> AotResult<CraneliftObjectResult> {
    Err(AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(
            "Cranelift object output was requested, but this `juliars` binary was built without the `cranelift` feature (Issue #7082)",
        )
        .with_workaround(
            "rebuild with `cargo build --release -p subset_julia_vm --features cranelift --bin juliars`, or use the Rust backend",
        ),
    ))
}

#[cfg(not(feature = "cranelift"))]
pub fn compile_cranelift_object_for_target(
    program: crate::ir::core::Program,
    config: &CompileConfig,
    _target: Option<&str>,
) -> AotResult<CraneliftObjectResult> {
    compile_cranelift_object(program, config)
}

#[cfg(feature = "cranelift")]
pub fn compile_cranelift_object(
    program: crate::ir::core::Program,
    config: &CompileConfig,
) -> AotResult<CraneliftObjectResult> {
    compile_cranelift_object_for_target(program, config, None)
}

#[cfg(feature = "cranelift")]
pub fn compile_cranelift_object_for_target(
    program: crate::ir::core::Program,
    config: &CompileConfig,
    target: Option<&str>,
) -> AotResult<CraneliftObjectResult> {
    use crate::aot::codegen::cranelift::CraneliftObjectCodeGenerator;

    let mut object_config = config.clone();
    object_config.backend = AotBackend::Cranelift;
    let debug_lines = object_config
        .debug_info
        .then(|| collect_cranelift_debug_lines(&program));

    let mut prepared = prepare_aot_program(program, &object_config)?;

    let t = AotTimer::start();
    prepared.diagnostics.verify_and_record(
        pass_pipeline::AotPassStage::BeforeBackendCodegen,
        &prepared.aot_program,
    )?;
    let mut module = lower_aot_program_for_cranelift(&prepared.aot_program, debug_lines.as_ref())?;
    append_cranelift_c_abi_export_wrappers(
        &mut module,
        &prepared.aot_program,
        &object_config.c_abi_exports,
    )?;
    let codegen_config = codegen_config_from_compile_config(&object_config);
    let codegen = if let Some(target) = target {
        let target = target.parse().map_err(|e| {
            AotError::CodegenError(format!("invalid Cranelift target triple `{target}`: {e}"))
        })?;
        CraneliftObjectCodeGenerator::with_config_and_target(codegen_config, target)
    } else {
        CraneliftObjectCodeGenerator::with_config(codegen_config)
    }
    .map_err(|e| AotError::CodegenError(e.to_string()))?;
    let object_bytes = codegen
        .generate_object(&module)
        .map_err(|e| AotError::CodegenError(e.to_string()))?;
    prepared.timings.push(("codegen", t.elapsed()));

    prepared.stats.dynamic_fallbacks = prepared.dynamic_count;
    Ok(CraneliftObjectResult {
        object_bytes,
        stats: prepared.stats,
        dumps: prepared.diagnostics.render_dumps(),
        timings: prepared.timings,
    })
}

#[cfg(not(feature = "cranelift"))]
pub fn run_cranelift_jit_main(
    _program: crate::ir::core::Program,
    _config: &CompileConfig,
) -> AotResult<CraneliftJitRunResult> {
    Err(AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(
            "Cranelift JIT run was requested, but this `juliars` binary was built without the `cranelift` feature (Issue #7131)",
        )
        .with_workaround(
            "rebuild with `cargo build --release -p subset_julia_vm --features cranelift --bin juliars`, or use the Rust backend",
        ),
    ))
}

#[cfg(feature = "cranelift")]
pub fn run_cranelift_jit_main(
    program: crate::ir::core::Program,
    config: &CompileConfig,
) -> AotResult<CraneliftJitRunResult> {
    use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
    use crate::aot::codegen::CodeGenerator;

    let mut jit_config = config.clone();
    jit_config.backend = AotBackend::Cranelift;

    let mut prepared = prepare_aot_program(program, &jit_config)?;

    let t = AotTimer::start();
    prepared.diagnostics.verify_and_record(
        pass_pipeline::AotPassStage::BeforeBackendCodegen,
        &prepared.aot_program,
    )?;
    let module = lower_aot_program_for_cranelift(&prepared.aot_program, None)?;
    let mut codegen =
        CraneliftCodeGenerator::with_config(codegen_config_from_compile_config(&jit_config))
            .map_err(|e| AotError::CodegenError(e.to_string()))?;
    codegen.generate_module(&module)?;
    prepared.timings.push(("codegen", t.elapsed()));

    let t = AotTimer::start();
    let main = unsafe {
        codegen
            .get_typed_function::<extern "C" fn()>("__juliars_main")
            .ok_or_else(|| {
                AotError::CodegenError(
                    "Cranelift JIT did not produce `__juliars_main` entry point".to_string(),
                )
            })?
    };
    main();
    prepared.timings.push(("jit-main", t.elapsed()));

    prepared.stats.dynamic_fallbacks = prepared.dynamic_count;
    Ok(CraneliftJitRunResult {
        stats: prepared.stats,
        dumps: prepared.diagnostics.render_dumps(),
        timings: prepared.timings,
    })
}

#[cfg(feature = "cranelift")]
fn cranelift_unsupported(message: impl Into<String>) -> AotError {
    AotError::UnsupportedInstruction(
        UnsupportedInstructionDiagnostic::new(format!("{} (Issue #6927)", message.into()))
            .with_workaround(
                "use `--backend rust` for the full current AoT surface, or restrict Cranelift input to straight-line scalar code",
            ),
    )
}

#[cfg(feature = "cranelift")]
fn cranelift_type_supported(ty: &types::StaticType) -> bool {
    fn scalar_supported(ty: &types::StaticType) -> bool {
        matches!(
            ty,
            types::StaticType::I8
                | types::StaticType::I16
                | types::StaticType::I32
                | types::StaticType::I64
                | types::StaticType::I128
                | types::StaticType::U8
                | types::StaticType::U16
                | types::StaticType::U32
                | types::StaticType::U64
                | types::StaticType::U128
                | types::StaticType::F16
                | types::StaticType::F32
                | types::StaticType::F64
                | types::StaticType::Bool
                | types::StaticType::Char
                | types::StaticType::Nothing
        )
    }

    match ty {
        types::StaticType::Tuple(elements) => elements.iter().all(scalar_supported),
        ty => scalar_supported(ty),
    }
}

#[cfg(feature = "cranelift")]
fn cranelift_local_type_supported(ty: &types::StaticType) -> bool {
    cranelift_type_supported(ty) || matches!(ty, types::StaticType::Str)
}

#[cfg(feature = "cranelift")]
fn cranelift_complex_element_type_from_name(name: &str) -> Option<types::StaticType> {
    if name == "Complex" {
        return Some(types::StaticType::F64);
    }
    match types::StaticType::complex_param_type_from_name(name)? {
        types::StaticType::F32 => Some(types::StaticType::F32),
        types::StaticType::F64 => Some(types::StaticType::F64),
        _ => None,
    }
}

#[cfg(feature = "cranelift")]
fn cranelift_complex_element_type(ty: &types::StaticType) -> Option<types::StaticType> {
    let types::StaticType::Struct { name, .. } = ty else {
        return None;
    };
    cranelift_complex_element_type_from_name(name)
}

#[cfg(feature = "cranelift")]
fn cranelift_complex_types_compatible(
    expected: &types::StaticType,
    actual: &types::StaticType,
) -> bool {
    cranelift_complex_element_type(expected).is_some()
        && cranelift_complex_element_type(expected) == cranelift_complex_element_type(actual)
}

#[cfg(feature = "cranelift")]
#[derive(Debug, Clone)]
struct CraneliftStructField {
    name: String,
    ty: types::StaticType,
    offset: u32,
}

#[cfg(feature = "cranelift")]
#[derive(Debug, Clone)]
struct CraneliftStructLayout {
    fields: Vec<CraneliftStructField>,
    size: u32,
    align: u8,
    is_mutable: bool,
}

#[cfg(feature = "cranelift")]
impl CraneliftStructLayout {
    fn field(&self, name: &str) -> Option<&CraneliftStructField> {
        self.fields.iter().find(|field| field.name == name)
    }
}

#[cfg(feature = "cranelift")]
fn align_to(offset: usize, align: usize) -> usize {
    if align <= 1 {
        offset
    } else {
        offset.div_ceil(align) * align
    }
}

#[cfg(feature = "cranelift")]
fn cranelift_scalar_layout(ty: &types::StaticType) -> Option<(usize, usize)> {
    match ty {
        types::StaticType::Bool | types::StaticType::I8 | types::StaticType::U8 => Some((1, 1)),
        types::StaticType::I16 | types::StaticType::U16 | types::StaticType::F16 => Some((2, 2)),
        types::StaticType::I32
        | types::StaticType::U32
        | types::StaticType::F32
        | types::StaticType::Char => Some((4, 4)),
        types::StaticType::I64 | types::StaticType::U64 | types::StaticType::F64 => Some((8, 8)),
        types::StaticType::I128 | types::StaticType::U128 => Some((16, 16)),
        _ => None,
    }
}

#[cfg(feature = "cranelift")]
fn build_cranelift_struct_layouts(
    structs: &[ir::AotStruct],
) -> AotResult<std::collections::HashMap<String, CraneliftStructLayout>> {
    let mut layouts = std::collections::HashMap::new();
    for s in structs {
        if !s.type_params.is_empty() {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend does not yet lower parametric struct `{}` layout (Issue #7095)",
                s.name
            )));
        }

        let mut offset = 0usize;
        let mut max_align = 1usize;
        let mut fields = Vec::with_capacity(s.fields.len());
        for (name, ty) in &s.fields {
            let Some((size, align)) = cranelift_scalar_layout(ty) else {
                return Err(cranelift_unsupported(format!(
                    "Cranelift backend struct `{}` field `{}` has unsupported type `{}` (Issue #7095)",
                    s.name,
                    name,
                    ty.julia_type_name()
                )));
            };
            offset = align_to(offset, align);
            max_align = max_align.max(align);
            fields.push(CraneliftStructField {
                name: name.clone(),
                ty: ty.clone(),
                offset: offset as u32,
            });
            offset = offset.checked_add(size).ok_or_else(|| {
                AotError::InternalError(format!("struct `{}` layout size overflow", s.name))
            })?;
        }

        let size = align_to(offset, max_align).max(1);
        let size = u32::try_from(size).map_err(|_| {
            AotError::InternalError(format!("struct `{}` layout size exceeds u32", s.name))
        })?;
        layouts.insert(
            s.name.clone(),
            CraneliftStructLayout {
                fields,
                size,
                align: max_align as u8,
                is_mutable: s.is_mutable,
            },
        );
    }
    insert_builtin_cranelift_complex_layouts(&mut layouts);
    Ok(layouts)
}

#[cfg(feature = "cranelift")]
fn insert_builtin_cranelift_complex_layouts(
    layouts: &mut std::collections::HashMap<String, CraneliftStructLayout>,
) {
    // Issue #10907: `size`/`align` used to come from `cranelift_scalar_layout`
    // (a general, `Option`-returning helper covering every `StaticType`),
    // guarded only by a panicking "Complex element type is scalar" assertion
    // that relied on the caller only ever passing F64/F32 below. Since both
    // call sites already know their element's scalar layout literally, take
    // it as a parameter instead — the possibility of "not actually scalar"
    // is removed by construction rather than asserted at runtime.
    fn layout(element_ty: types::StaticType, size: usize, align: usize) -> CraneliftStructLayout {
        CraneliftStructLayout {
            fields: vec![
                CraneliftStructField {
                    name: "re".to_string(),
                    ty: element_ty.clone(),
                    offset: 0,
                },
                CraneliftStructField {
                    name: "im".to_string(),
                    ty: element_ty,
                    offset: size as u32,
                },
            ],
            size: (size * 2) as u32,
            align: align as u8,
            is_mutable: false,
        }
    }

    for name in ["Complex", "ComplexF64", "Complex{Float64}"] {
        layouts
            .entry(name.to_string())
            .or_insert_with(|| layout(types::StaticType::F64, 8, 8));
    }
    for name in ["ComplexF32", "Complex{Float32}"] {
        layouts
            .entry(name.to_string())
            .or_insert_with(|| layout(types::StaticType::F32, 4, 4));
    }
}

#[cfg(feature = "cranelift")]
fn cranelift_global_initializers(
    globals: &[ir::AotGlobal],
) -> AotResult<std::collections::HashMap<String, ir::AotExpr>> {
    let mut initializers = std::collections::HashMap::new();
    for global in globals {
        if !cranelift_type_supported(&global.ty) {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend does not yet lower global `{}` of type `{}` (Issue #7103)",
                global.name,
                global.ty.julia_type_name()
            )));
        }
        let Some(init) = &global.init else {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend requires initialized scalar globals; `{}` has no initializer (Issue #7103)",
                global.name
            )));
        };
        if !cranelift_global_initializer_supported(init) {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend requires a scalar constant initializer for global `{}` (Issue #7103)",
                global.name
            )));
        }
        initializers.insert(global.name.clone(), init.clone());
    }
    Ok(initializers)
}

#[cfg(feature = "cranelift")]
fn cranelift_global_initializer_supported(expr: &ir::AotExpr) -> bool {
    match expr {
        ir::AotExpr::LitI64(_)
        | ir::AotExpr::LitI32(_)
        | ir::AotExpr::LitF64(_)
        | ir::AotExpr::LitF32(_)
        | ir::AotExpr::LitBool(_)
        | ir::AotExpr::LitChar(_)
        | ir::AotExpr::LitNothing
        | ir::AotExpr::Var { .. } => true,
        ir::AotExpr::BinOpStatic {
            left,
            right,
            result_ty,
            ..
        } => {
            cranelift_type_supported(result_ty)
                && cranelift_global_initializer_supported(left)
                && cranelift_global_initializer_supported(right)
        }
        ir::AotExpr::UnaryOp {
            operand, result_ty, ..
        } => cranelift_type_supported(result_ty) && cranelift_global_initializer_supported(operand),
        _ => false,
    }
}

#[cfg(feature = "cranelift")]
fn lower_aot_program_for_cranelift(
    aot_program: &ir::AotProgram,
    debug_lines: Option<&CraneliftDebugLines>,
) -> AotResult<ir::IrModule> {
    let struct_layouts = build_cranelift_struct_layouts(&aot_program.structs)?;
    let globals = cranelift_global_initializers(&aot_program.globals)?;
    let mut module = ir::IrModule::new("juliars_cranelift".to_string());
    for func in &aot_program.functions {
        let debug_line = debug_lines.and_then(|lines| lines.function_line(&func.name));
        module.add_function(
            CraneliftAotLowerer::new(globals.clone(), struct_layouts.clone(), debug_line)
                .lower_function(func)?,
        );
    }
    module.add_function(
        CraneliftAotLowerer::new(
            globals,
            struct_layouts,
            debug_lines.and_then(|lines| lines.main_line),
        )
        .lower_main(&aot_program.main)?,
    );
    module.add_function(cranelift_standalone_main_wrapper()?);
    Ok(module)
}

#[cfg(feature = "cranelift")]
fn cranelift_standalone_main_wrapper() -> AotResult<ir::IrFunction> {
    let mut wrapper = ir::IrFunction::new("main".to_string(), Vec::new(), types::StaticType::I32);
    let exit_code = ir::VarRef::new("__sjulia_exit_code".to_string(), types::StaticType::I32);
    // INTERNAL: `IrFunction::new` always creates its entry block (Issue #10907).
    let entry = wrapper.entry_block_mut().ok_or_else(|| {
        AotError::InternalError(
            "cranelift_standalone_main_wrapper: IrFunction::new always creates its entry block"
                .to_string(),
        )
    })?;
    entry.push(ir::Instruction::Call {
        dest: None,
        func: "__juliars_main".to_string(),
        args: Vec::new(),
    });
    entry.push(ir::Instruction::LoadConst {
        dest: exit_code.clone(),
        value: ir::ConstValue::Int32(0),
    });
    entry.set_terminator(ir::Terminator::Return(Some(exit_code)));
    Ok(wrapper)
}

#[cfg(feature = "cranelift")]
struct CraneliftAotLowerer {
    vars: std::collections::HashMap<String, ir::VarRef>,
    globals: std::collections::HashMap<String, ir::AotExpr>,
    struct_layouts: std::collections::HashMap<String, CraneliftStructLayout>,
    tuple_vars: std::collections::HashMap<String, Vec<ir::VarRef>>,
    temp_index: usize,
    block_index: usize,
    current_block: String,
    debug_line: Option<u32>,
}

#[cfg(feature = "cranelift")]
impl CraneliftAotLowerer {
    fn new(
        globals: std::collections::HashMap<String, ir::AotExpr>,
        struct_layouts: std::collections::HashMap<String, CraneliftStructLayout>,
        debug_line: Option<u32>,
    ) -> Self {
        Self {
            vars: std::collections::HashMap::new(),
            globals,
            struct_layouts,
            tuple_vars: std::collections::HashMap::new(),
            temp_index: 0,
            block_index: 0,
            current_block: "entry".to_string(),
            debug_line,
        }
    }

    fn lower_function(mut self, func: &ir::AotFunction) -> AotResult<ir::IrFunction> {
        for (_, ty) in &func.params {
            if matches!(ty, types::StaticType::Tuple(_)) {
                return Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower tuple parameter type `{}` (Issue #7117)",
                    ty.julia_type_name()
                )));
            }
            if matches!(ty, types::StaticType::Struct { .. }) {
                return Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower struct parameter type `{}` (Issue #7095)",
                    ty.julia_type_name()
                )));
            }
            if !cranelift_type_supported(ty) {
                return Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower parameter type `{}`",
                    ty.julia_type_name()
                )));
            }
        }
        if !cranelift_type_supported(&func.return_type) {
            if matches!(func.return_type, types::StaticType::Struct { .. }) {
                return Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower struct return type `{}` (Issue #7095)",
                    func.return_type.julia_type_name()
                )));
            }
            return Err(cranelift_unsupported(format!(
                "Cranelift backend does not yet lower return type `{}`",
                func.return_type.julia_type_name()
            )));
        }

        let mut lowered = ir::IrFunction::new(
            func.name.clone(),
            func.params.clone(),
            func.return_type.clone(),
        );
        lowered.debug_line = self.debug_line;
        for (name, ty) in &func.params {
            self.vars
                .insert(name.clone(), ir::VarRef::new(name.clone(), ty.clone()));
        }
        self.lower_stmts(&func.body, &mut lowered)?;
        self.ensure_terminator(&mut lowered)?;
        Ok(lowered)
    }

    fn lower_main(mut self, stmts: &[ir::AotStmt]) -> AotResult<ir::IrFunction> {
        let mut lowered = ir::IrFunction::new(
            "__juliars_main".to_string(),
            Vec::new(),
            types::StaticType::Nothing,
        );
        lowered.debug_line = self.debug_line;
        self.lower_stmts(stmts, &mut lowered)?;
        self.ensure_terminator(&mut lowered)?;
        Ok(lowered)
    }

    fn lower_stmts(&mut self, stmts: &[ir::AotStmt], func: &mut ir::IrFunction) -> AotResult<()> {
        for stmt in stmts {
            if self.current_block_terminated(func)? {
                break;
            }
            self.lower_stmt(stmt, func)?;
        }
        Ok(())
    }

    fn lower_stmt(&mut self, stmt: &ir::AotStmt, func: &mut ir::IrFunction) -> AotResult<()> {
        match stmt {
            ir::AotStmt::Let {
                name, ty, value, ..
            } => {
                if let types::StaticType::Tuple(_) = ty {
                    return self.lower_tuple_binding(name, ty, value, func);
                }
                if let types::StaticType::Struct { .. } = ty {
                    return self.lower_struct_binding(name, ty, value, func);
                }
                if !cranelift_local_type_supported(ty) {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower local type `{}`",
                        ty.julia_type_name()
                    )));
                }
                let src = self.lower_expr(value, func)?;
                let dest = ir::VarRef::new(name.clone(), ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::Copy {
                    dest: dest.clone(),
                    src,
                });
                self.vars.insert(name.clone(), dest);
                Ok(())
            }
            ir::AotStmt::Assign { target, value } => {
                if let ir::AotExpr::FieldAccess {
                    object,
                    field,
                    field_ty,
                } = target
                {
                    return self.lower_struct_field_assign(object, field, field_ty, value, func);
                }
                let ir::AotExpr::Var { name, ty } = target else {
                    return Err(cranelift_unsupported(
                        "Cranelift backend only lowers simple variable or struct field assignment",
                    ));
                };
                if let types::StaticType::Tuple(_) = ty {
                    return self.lower_tuple_binding(name, ty, value, func);
                }
                if let types::StaticType::Struct { .. } = ty {
                    return self.lower_struct_binding(name, ty, value, func);
                }
                if !cranelift_local_type_supported(ty) {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower assignment type `{}`",
                        ty.julia_type_name()
                    )));
                }
                let src = self.lower_expr(value, func)?;
                let dest = ir::VarRef::new(name.clone(), ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::Copy {
                    dest: dest.clone(),
                    src,
                });
                self.vars.insert(name.clone(), dest);
                Ok(())
            }
            ir::AotStmt::Expr(expr) => self.lower_expr_for_effect(expr, func),
            ir::AotStmt::ValueCarrier(expr) => self.lower_expr_for_effect(expr, func),
            ir::AotStmt::Return(Some(expr)) => {
                if let types::StaticType::Tuple(elements) = func.return_type.clone() {
                    let values = self.lower_tuple_expr_fields(expr, &elements, func, None)?;
                    self.current_block_mut(func)?
                        .set_terminator(ir::Terminator::ReturnMany(values));
                    return Ok(());
                }
                let value = self.lower_expr(expr, func)?;
                self.current_block_mut(func)?
                    .set_terminator(ir::Terminator::Return(Some(value)));
                Ok(())
            }
            ir::AotStmt::Return(None) => {
                self.current_block_mut(func)?
                    .set_terminator(ir::Terminator::Return(None));
                Ok(())
            }
            _ => Err(cranelift_unsupported(
                "Cranelift backend currently lowers only straight-line scalar statements",
            )),
        }
    }

    fn lower_expr_for_effect(
        &mut self,
        expr: &ir::AotExpr,
        func: &mut ir::IrFunction,
    ) -> AotResult<()> {
        if let ir::AotExpr::CallStatic {
            function,
            args,
            return_ty,
            ..
        } = expr
        {
            if *return_ty == types::StaticType::Nothing {
                let args = self.lower_call_args(args, func)?;
                self.current_block_mut(func)?.push(ir::Instruction::Call {
                    dest: None,
                    func: function.clone(),
                    args,
                });
                return Ok(());
            }
        }
        self.lower_expr(expr, func).map(|_| ())
    }

    fn lower_expr(
        &mut self,
        expr: &ir::AotExpr,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        match expr {
            ir::AotExpr::LitI64(value) => self.lower_const(func, ir::ConstValue::Int64(*value)),
            ir::AotExpr::LitI32(value) => self.lower_const(func, ir::ConstValue::Int32(*value)),
            ir::AotExpr::LitF64(value) => self.lower_const(func, ir::ConstValue::Float64(*value)),
            ir::AotExpr::LitF32(value) => self.lower_const(func, ir::ConstValue::Float32(*value)),
            ir::AotExpr::LitBool(value) => self.lower_const(func, ir::ConstValue::Bool(*value)),
            ir::AotExpr::LitChar(value) => self.lower_const(func, ir::ConstValue::Char(*value)),
            ir::AotExpr::LitStr(value) => {
                self.lower_const(func, ir::ConstValue::String(value.clone()))
            }
            ir::AotExpr::LitNothing => self.lower_const(func, ir::ConstValue::Nothing),
            ir::AotExpr::Var { name, .. } => {
                if let Some(var) = self.vars.get(name) {
                    return Ok(var.clone());
                }
                if let Some(init) = self.globals.get(name).cloned() {
                    return self.lower_expr(&init, func);
                }
                Err(cranelift_unsupported(format!(
                    "Cranelift backend could not resolve variable `{name}`"
                )))
            }
            ir::AotExpr::BinOpStatic {
                op,
                left,
                right,
                result_ty,
            } => {
                if cranelift_complex_element_type(result_ty).is_some() {
                    return self.lower_complex_binop(*op, left, right, result_ty, func);
                }
                if !cranelift_type_supported(result_ty) {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower binary result type `{}`",
                        result_ty.julia_type_name()
                    )));
                }
                if matches!(op, ir::AotBinOp::And | ir::AotBinOp::Or) {
                    return self.lower_short_circuit_bool(*op, left, right, result_ty, func);
                }
                let left = self.lower_expr(left, func)?;
                let right = self.lower_expr(right, func)?;
                let dest = self.temp(result_ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::BinOp {
                    dest: dest.clone(),
                    op: Self::map_binop(*op)?,
                    left,
                    right,
                });
                Ok(dest)
            }
            ir::AotExpr::UnaryOp {
                op,
                operand,
                result_ty,
            } => {
                if matches!(op, ir::AotUnaryOp::Pos) {
                    return self.lower_expr(operand, func);
                }
                if !cranelift_type_supported(result_ty) {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower unary result type `{}`",
                        result_ty.julia_type_name()
                    )));
                }
                let operand = self.lower_expr(operand, func)?;
                let dest = self.temp(result_ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::UnaryOp {
                    dest: dest.clone(),
                    op: Self::map_unaryop(*op)?,
                    operand,
                });
                Ok(dest)
            }
            ir::AotExpr::CallStatic {
                function,
                args,
                return_ty,
                ..
            } => {
                if matches!(return_ty, types::StaticType::Tuple(_)) {
                    return Err(cranelift_unsupported(
                        "Cranelift backend lowers tuple-returning calls only through tuple bindings, field access, or returns (Issue #7117)",
                    ));
                }
                if *return_ty == types::StaticType::Nothing {
                    return Err(cranelift_unsupported(
                        "Cranelift backend cannot use a `Nothing` call as a value",
                    ));
                }
                if !cranelift_type_supported(return_ty) {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower call return type `{}`",
                        return_ty.julia_type_name()
                    )));
                }
                let args = self.lower_call_args(args, func)?;
                let dest = self.temp(return_ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::Call {
                    dest: Some(dest.clone()),
                    func: function.clone(),
                    args,
                });
                Ok(dest)
            }
            ir::AotExpr::Convert { value, target_ty } => {
                let source_ty = value.get_type();
                if &source_ty == target_ty {
                    self.lower_expr(value, func)
                } else {
                    Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower Julia conversion from `{}` to `{}` with Rust-backend parity for range/rounding checks (Issue #7123)",
                        source_ty.julia_type_name(),
                        target_ty.julia_type_name()
                    )))
                }
            }
            ir::AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } if matches!(builtin, ir::AotBuiltinOp::Real | ir::AotBuiltinOp::Imag)
                && args.len() == 1 =>
            {
                let part = if matches!(builtin, ir::AotBuiltinOp::Real) {
                    "re"
                } else {
                    "im"
                };
                self.lower_complex_part(&args[0], part, return_ty, func)
            }
            ir::AotExpr::CallBuiltin {
                builtin: ir::AotBuiltinOp::Abs2,
                args,
                return_ty,
            } if args.len() == 1 => self.lower_complex_abs2(&args[0], return_ty, func),
            ir::AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } if Self::cranelift_math_builtin_name(*builtin).is_some() => {
                if *return_ty == types::StaticType::Nothing {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend cannot use math builtin `{}` as a value",
                        builtin
                    )));
                }
                if !cranelift_type_supported(return_ty) {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend does not yet lower math builtin `{}` return type `{}`",
                        builtin,
                        return_ty.julia_type_name()
                    )));
                }
                let args = self.lower_call_args(args, func)?;
                let dest = self.temp(return_ty.clone());
                // Issue #10907: bind the match guard's `Option` once instead
                // of re-deriving it with a second `Self::cranelift_math_builtin_name`
                // call guarded only by a panicking "math builtin was matched"
                // assertion.
                let Some(math_name) = Self::cranelift_math_builtin_name(*builtin) else {
                    return Err(AotError::InternalError(format!(
                        "cranelift math builtin `{builtin}` matched the arm guard but not the body"
                    )));
                };
                self.current_block_mut(func)?.push(ir::Instruction::Call {
                    dest: Some(dest.clone()),
                    func: math_name.to_string(),
                    args,
                });
                Ok(dest)
            }
            ir::AotExpr::CallBuiltin { builtin, .. }
                if matches!(
                    builtin,
                    ir::AotBuiltinOp::Print | ir::AotBuiltinOp::Println | ir::AotBuiltinOp::StringConcat
                ) =>
            {
                Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower display builtin `{}` through Julia's print/show formatting runtime (Issue #7121)",
                    builtin
                )))
            }
            ir::AotExpr::CallBuiltin {
                builtin,
                args,
                return_ty,
            } if matches!(
                builtin,
                ir::AotBuiltinOp::Length | ir::AotBuiltinOp::StringLength
            ) && args.len() == 1
                && matches!(args[0].get_type(), types::StaticType::Str) =>
            {
                if *return_ty != types::StaticType::I64 {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend expected String length builtin `{}` to return Int64, got `{}`",
                        builtin,
                        return_ty.julia_type_name()
                    )));
                }
                let args = self.lower_call_args(args, func)?;
                let dest = self.temp(return_ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::Call {
                    dest: Some(dest.clone()),
                    func: "__sjulia_string_length".to_string(),
                    args,
                });
                Ok(dest)
            }
            ir::AotExpr::CallBuiltin { builtin, .. }
                if matches!(builtin, ir::AotBuiltinOp::Sitofp | ir::AotBuiltinOp::Fptosi) =>
            {
                Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower conversion builtin `{}` with Julia range/rounding semantics (Issue #7123)",
                    builtin
                )))
            }
            ir::AotExpr::CallBuiltin { builtin, .. }
                if matches!(
                    builtin,
                    ir::AotBuiltinOp::Div
                        | ir::AotBuiltinOp::Rem
                        | ir::AotBuiltinOp::Mod
                        | ir::AotBuiltinOp::Fld
                        | ir::AotBuiltinOp::Cld
                ) =>
            {
                Err(cranelift_unsupported(format!(
                    "Cranelift backend does not yet lower builtin `{}` with Julia division-family semantics (Issue #7119)",
                    builtin
                )))
            }
            ir::AotExpr::Index {
                array,
                indices,
                elem_ty,
                is_tuple: true,
            } => self.lower_tuple_index(array, indices, elem_ty, func),
            ir::AotExpr::StructNew { name, fields } => {
                let ty = types::StaticType::Struct {
                    type_id: 0,
                    name: name.clone(),
                };
                self.lower_struct_new(name, &ty, fields, None, func)
            }
            ir::AotExpr::FieldAccess {
                object,
                field,
                field_ty,
            } => self.lower_struct_field_access(object, field, field_ty, func),
            ir::AotExpr::TupleLit { .. } => Err(cranelift_unsupported(
                "Cranelift backend lowers tuple literals only through constant field access or local tuple bindings (Issue #7097)",
            )),
            _ => Err(cranelift_unsupported(
                "Cranelift backend currently lowers only scalar literals, variables, calls, and simple arithmetic",
            )),
        }
    }

    fn lower_tuple_binding(
        &mut self,
        name: &str,
        ty: &types::StaticType,
        value: &ir::AotExpr,
        func: &mut ir::IrFunction,
    ) -> AotResult<()> {
        let types::StaticType::Tuple(elements) = ty else {
            return Err(AotError::InternalError(
                "tuple binding called for non-tuple type".to_string(),
            ));
        };
        let fields = self.lower_tuple_expr_fields(value, elements, func, Some(name))?;
        self.tuple_vars.insert(name.to_string(), fields);
        Ok(())
    }

    fn lower_struct_binding(
        &mut self,
        name: &str,
        ty: &types::StaticType,
        value: &ir::AotExpr,
        func: &mut ir::IrFunction,
    ) -> AotResult<()> {
        match value {
            ir::AotExpr::StructNew {
                name: struct_name,
                fields,
            } => {
                let dest = self.lower_struct_new(struct_name, ty, fields, Some(name), func)?;
                self.vars.insert(name.to_string(), dest);
                Ok(())
            }
            ir::AotExpr::Var {
                name: src_name,
                ty: src_ty,
            } if src_ty == ty || cranelift_complex_types_compatible(ty, src_ty) => {
                let Some(src) = self.vars.get(src_name).cloned() else {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend could not resolve struct variable `{src_name}` (Issue #7095)"
                    )));
                };
                let dest = ir::VarRef::new(name.to_string(), ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::Copy {
                    dest: dest.clone(),
                    src,
                });
                self.vars.insert(name.to_string(), dest);
                Ok(())
            }
            _ if cranelift_complex_types_compatible(ty, &value.get_type()) => {
                let src = self.lower_expr(value, func)?;
                let dest = ir::VarRef::new(name.to_string(), ty.clone());
                self.current_block_mut(func)?.push(ir::Instruction::Copy {
                    dest: dest.clone(),
                    src,
                });
                self.vars.insert(name.to_string(), dest);
                Ok(())
            }
            _ => Err(cranelift_unsupported(
                "Cranelift backend lowers struct locals only from constructors or same-typed struct variables (Issue #7095)",
            )),
        }
    }

    fn lower_struct_new(
        &mut self,
        struct_name: &str,
        ty: &types::StaticType,
        fields: &[ir::AotExpr],
        binding_name: Option<&str>,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let layout = self.struct_layout(struct_name)?;
        if fields.len() != layout.fields.len() {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend struct `{struct_name}` constructor has {} field(s), expected {} (Issue #7095)",
                fields.len(),
                layout.fields.len()
            )));
        }

        let mut inits = Vec::with_capacity(fields.len());
        for (expr, field) in fields.iter().zip(&layout.fields) {
            let value = self.lower_expr(expr, func)?;
            if value.ty != field.ty {
                return Err(cranelift_unsupported(format!(
                    "Cranelift backend struct `{struct_name}` field `{}` type `{}` does not match expected `{}` (Issue #7095)",
                    field.name,
                    value.ty.julia_type_name(),
                    field.ty.julia_type_name()
                )));
            }
            inits.push(ir::StructFieldInit {
                offset: field.offset as i32,
                value,
            });
        }

        let dest = if let Some(name) = binding_name {
            ir::VarRef::new(name.to_string(), ty.clone())
        } else {
            self.temp(ty.clone())
        };
        self.current_block_mut(func)?
            .push(ir::Instruction::StructNew {
                dest: dest.clone(),
                layout_id: 0,
                size: layout.size,
                align: layout.align,
                fields: inits,
            });
        Ok(dest)
    }

    fn lower_struct_field_from_var(
        &mut self,
        object: ir::VarRef,
        field: &str,
        field_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let layout_field = self.struct_field(&object.ty, field)?;
        if &layout_field.ty != field_ty {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend struct field `{field}` type `{}` does not match expected `{}` (Issue #7095)",
                layout_field.ty.julia_type_name(),
                field_ty.julia_type_name()
            )));
        }
        let dest = self.temp(field_ty.clone());
        self.current_block_mut(func)?
            .push(ir::Instruction::GetFieldOffset {
                dest: dest.clone(),
                object,
                layout_id: 0,
                offset: layout_field.offset as i32,
            });
        Ok(dest)
    }

    fn lower_struct_field_access(
        &mut self,
        object: &ir::AotExpr,
        field: &str,
        field_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let object_ty = object.get_type();
        let layout_field = self.struct_field(&object_ty, field)?;
        if &layout_field.ty != field_ty {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend struct field `{field}` type `{}` does not match expected `{}` (Issue #7095)",
                layout_field.ty.julia_type_name(),
                field_ty.julia_type_name()
            )));
        }
        let object = self.lower_expr(object, func)?;
        let dest = self.temp(field_ty.clone());
        self.current_block_mut(func)?
            .push(ir::Instruction::GetFieldOffset {
                dest: dest.clone(),
                object,
                layout_id: 0,
                offset: layout_field.offset as i32,
            });
        Ok(dest)
    }

    fn lower_binop_vars(
        &mut self,
        op: ir::BinOpKind,
        left: ir::VarRef,
        right: ir::VarRef,
        result_ty: types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let dest = self.temp(result_ty);
        self.current_block_mut(func)?.push(ir::Instruction::BinOp {
            dest: dest.clone(),
            op,
            left,
            right,
        });
        Ok(dest)
    }

    fn lower_complex_part(
        &mut self,
        value: &ir::AotExpr,
        part: &str,
        return_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let Some(element_ty) = cranelift_complex_element_type(&value.get_type()) else {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend expected Complex operand for `{part}` builtin (Issue #7099)"
            )));
        };
        if &element_ty != return_ty {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend Complex `{part}` return type `{}` does not match element type `{}` (Issue #7099)",
                return_ty.julia_type_name(),
                element_ty.julia_type_name()
            )));
        }
        let value = self.lower_expr(value, func)?;
        self.lower_struct_field_from_var(value, part, return_ty, func)
    }

    fn lower_complex_abs2(
        &mut self,
        value: &ir::AotExpr,
        return_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let Some(element_ty) = cranelift_complex_element_type(&value.get_type()) else {
            return Err(cranelift_unsupported(
                "Cranelift backend expected Complex operand for abs2 builtin (Issue #7099)",
            ));
        };
        if &element_ty != return_ty {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend Complex abs2 return type `{}` does not match element type `{}` (Issue #7099)",
                return_ty.julia_type_name(),
                element_ty.julia_type_name()
            )));
        }
        let value = self.lower_expr(value, func)?;
        let re = self.lower_struct_field_from_var(value.clone(), "re", return_ty, func)?;
        let im = self.lower_struct_field_from_var(value, "im", return_ty, func)?;
        let re2 =
            self.lower_binop_vars(ir::BinOpKind::Mul, re.clone(), re, return_ty.clone(), func)?;
        let im2 =
            self.lower_binop_vars(ir::BinOpKind::Mul, im.clone(), im, return_ty.clone(), func)?;
        self.lower_binop_vars(ir::BinOpKind::Add, re2, im2, return_ty.clone(), func)
    }

    fn lower_complex_binop(
        &mut self,
        op: ir::AotBinOp,
        left: &ir::AotExpr,
        right: &ir::AotExpr,
        result_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        let Some(element_ty) = cranelift_complex_element_type(result_ty) else {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend does not yet lower binary result type `{}`",
                result_ty.julia_type_name()
            )));
        };
        if !matches!(
            op,
            ir::AotBinOp::Add | ir::AotBinOp::Sub | ir::AotBinOp::Mul
        ) {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend does not yet lower Complex binary operator `{op}` (Issue #7099)"
            )));
        }
        if cranelift_complex_element_type(&left.get_type()) != Some(element_ty.clone())
            || cranelift_complex_element_type(&right.get_type()) != Some(element_ty.clone())
        {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend Complex binary operands must match result element type `{}` (Issue #7099)",
                element_ty.julia_type_name()
            )));
        }

        let left = self.lower_expr(left, func)?;
        let right = self.lower_expr(right, func)?;
        let l_re = self.lower_struct_field_from_var(left.clone(), "re", &element_ty, func)?;
        let l_im = self.lower_struct_field_from_var(left, "im", &element_ty, func)?;
        let r_re = self.lower_struct_field_from_var(right.clone(), "re", &element_ty, func)?;
        let r_im = self.lower_struct_field_from_var(right, "im", &element_ty, func)?;

        let (re, im) = match op {
            ir::AotBinOp::Add | ir::AotBinOp::Sub => {
                let op = if matches!(op, ir::AotBinOp::Add) {
                    ir::BinOpKind::Add
                } else {
                    ir::BinOpKind::Sub
                };
                (
                    self.lower_binop_vars(op, l_re, r_re, element_ty.clone(), func)?,
                    self.lower_binop_vars(op, l_im, r_im, element_ty.clone(), func)?,
                )
            }
            ir::AotBinOp::Mul => {
                let ac = self.lower_binop_vars(
                    ir::BinOpKind::Mul,
                    l_re.clone(),
                    r_re.clone(),
                    element_ty.clone(),
                    func,
                )?;
                let bd = self.lower_binop_vars(
                    ir::BinOpKind::Mul,
                    l_im.clone(),
                    r_im.clone(),
                    element_ty.clone(),
                    func,
                )?;
                let ad = self.lower_binop_vars(
                    ir::BinOpKind::Mul,
                    l_re,
                    r_im,
                    element_ty.clone(),
                    func,
                )?;
                let bc = self.lower_binop_vars(
                    ir::BinOpKind::Mul,
                    l_im,
                    r_re,
                    element_ty.clone(),
                    func,
                )?;
                (
                    self.lower_binop_vars(ir::BinOpKind::Sub, ac, bd, element_ty.clone(), func)?,
                    self.lower_binop_vars(ir::BinOpKind::Add, ad, bc, element_ty.clone(), func)?,
                )
            }
            _ => unreachable!("Complex operator was checked above"),
        };

        let layout = self.struct_layout(self.struct_type_name(result_ty)?)?;
        // INTERNAL: every Complex layout is built by
        // `insert_builtin_cranelift_complex_layouts`, which always inserts
        // exactly a "re" and "im" field (Issue #10907 — replaces two
        // panicking field-lookup assertions with typed internal errors).
        let re_offset = layout.field("re").ok_or_else(|| {
            AotError::InternalError("Complex layout is missing its `re` field".to_string())
        })?;
        let re_offset = re_offset.offset as i32;
        let im_offset = layout.field("im").ok_or_else(|| {
            AotError::InternalError("Complex layout is missing its `im` field".to_string())
        })?;
        let im_offset = im_offset.offset as i32;
        let dest = self.temp(result_ty.clone());
        self.current_block_mut(func)?
            .push(ir::Instruction::StructNew {
                dest: dest.clone(),
                layout_id: 0,
                size: layout.size,
                align: layout.align,
                fields: vec![
                    ir::StructFieldInit {
                        offset: re_offset,
                        value: re,
                    },
                    ir::StructFieldInit {
                        offset: im_offset,
                        value: im,
                    },
                ],
            });
        Ok(dest)
    }

    fn lower_struct_field_assign(
        &mut self,
        object: &ir::AotExpr,
        field: &str,
        field_ty: &types::StaticType,
        value: &ir::AotExpr,
        func: &mut ir::IrFunction,
    ) -> AotResult<()> {
        let object_ty = object.get_type();
        let struct_name = self.struct_type_name(&object_ty)?;
        let layout = self.struct_layout(struct_name)?;
        if !layout.is_mutable {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend does not lower assignment to immutable struct `{struct_name}` field `{field}` (Issue #7095)",
            )));
        }
        let layout_field = layout.field(field).cloned().ok_or_else(|| {
            cranelift_unsupported(format!(
                "Cranelift backend struct `{struct_name}` has no field `{field}` (Issue #7095)"
            ))
        })?;
        if &layout_field.ty != field_ty {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend struct field `{field}` type `{}` does not match expected `{}` (Issue #7095)",
                layout_field.ty.julia_type_name(),
                field_ty.julia_type_name()
            )));
        }
        let object = self.lower_expr(object, func)?;
        let value = self.lower_expr(value, func)?;
        if value.ty != layout_field.ty {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend assigned value type `{}` does not match struct field `{field}` type `{}` (Issue #7095)",
                value.ty.julia_type_name(),
                layout_field.ty.julia_type_name()
            )));
        }
        self.current_block_mut(func)?
            .push(ir::Instruction::SetFieldOffset {
                object,
                offset: layout_field.offset as i32,
                value,
            });
        Ok(())
    }

    fn struct_type_name<'a>(&self, ty: &'a types::StaticType) -> AotResult<&'a str> {
        let types::StaticType::Struct { name, .. } = ty else {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend expected struct type, got `{}` (Issue #7095)",
                ty.julia_type_name()
            )));
        };
        Ok(name)
    }

    fn struct_layout(&self, name: &str) -> AotResult<CraneliftStructLayout> {
        self.struct_layouts.get(name).cloned().ok_or_else(|| {
            cranelift_unsupported(format!(
                "Cranelift backend has no layout for struct `{name}` (Issue #7095)"
            ))
        })
    }

    fn struct_field(
        &self,
        object_ty: &types::StaticType,
        field: &str,
    ) -> AotResult<CraneliftStructField> {
        let struct_name = self.struct_type_name(object_ty)?;
        self.struct_layout(struct_name)?
            .field(field)
            .cloned()
            .ok_or_else(|| {
                cranelift_unsupported(format!(
                    "Cranelift backend struct `{struct_name}` has no field `{field}` (Issue #7095)"
                ))
            })
    }

    fn lower_tuple_expr_fields(
        &mut self,
        value: &ir::AotExpr,
        expected: &[types::StaticType],
        func: &mut ir::IrFunction,
        binding_name: Option<&str>,
    ) -> AotResult<Vec<ir::VarRef>> {
        match value {
            ir::AotExpr::TupleLit { elements } => {
                if elements.len() != expected.len() {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend tuple literal has {} field(s), expected {} (Issue #7117)",
                        elements.len(),
                        expected.len()
                    )));
                }
                let mut fields = Vec::with_capacity(elements.len());
                for (index, (element, expected_ty)) in elements.iter().zip(expected).enumerate() {
                    let field = self.lower_expr(element, func)?;
                    if &field.ty != expected_ty {
                        return Err(cranelift_unsupported(format!(
                            "Cranelift backend tuple field {} type `{}` does not match expected `{}` (Issue #7117)",
                            index + 1,
                            field.ty.julia_type_name(),
                            expected_ty.julia_type_name()
                        )));
                    }
                    fields.push(self.copy_tuple_field(index, field, expected_ty, func, binding_name)?);
                }
                Ok(fields)
            }
            ir::AotExpr::Var { name, .. } => {
                let fields = self.tuple_vars.get(name).cloned().ok_or_else(|| {
                    cranelift_unsupported(format!(
                        "Cranelift backend could not resolve tuple variable `{name}` (Issue #7117)"
                    ))
                })?;
                self.copy_tuple_fields(fields, expected, func, binding_name)
            }
            ir::AotExpr::CallStatic {
                function,
                args,
                return_ty,
                ..
            } => {
                let types::StaticType::Tuple(actual) = return_ty else {
                    return Err(cranelift_unsupported(
                        "Cranelift backend expected a tuple-returning static call (Issue #7117)",
                    ));
                };
                if actual != expected {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend tuple-returning call `{}` returns `{}`, expected `{}` (Issue #7117)",
                        function,
                        return_ty.julia_type_name(),
                        types::StaticType::Tuple(expected.to_vec()).julia_type_name()
                    )));
                }
                let args = self.lower_call_args(args, func)?;
                let dests = expected
                    .iter()
                    .enumerate()
                    .map(|(index, ty)| self.tuple_field_dest(index, ty, binding_name))
                    .collect::<Vec<_>>();
                self.current_block_mut(func)?.push(ir::Instruction::CallMulti {
                    dests: dests.clone(),
                    func: function.clone(),
                    args,
                });
                Ok(dests)
            }
            _ => Err(cranelift_unsupported(
                "Cranelift backend lowers tuple values only from tuple literals, tuple variables, or tuple-returning static calls (Issue #7117)",
            )),
        }
    }

    fn copy_tuple_fields(
        &mut self,
        fields: Vec<ir::VarRef>,
        expected: &[types::StaticType],
        func: &mut ir::IrFunction,
        binding_name: Option<&str>,
    ) -> AotResult<Vec<ir::VarRef>> {
        if fields.len() != expected.len() {
            return Err(cranelift_unsupported(format!(
                "Cranelift backend tuple has {} field(s), expected {} (Issue #7117)",
                fields.len(),
                expected.len()
            )));
        }
        fields
            .into_iter()
            .zip(expected)
            .enumerate()
            .map(|(index, (field, expected_ty))| {
                if &field.ty != expected_ty {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend tuple field {} type `{}` does not match expected `{}` (Issue #7117)",
                        index + 1,
                        field.ty.julia_type_name(),
                        expected_ty.julia_type_name()
                    )));
                }
                self.copy_tuple_field(index, field, expected_ty, func, binding_name)
            })
            .collect()
    }

    fn copy_tuple_field(
        &mut self,
        index: usize,
        field: ir::VarRef,
        expected_ty: &types::StaticType,
        func: &mut ir::IrFunction,
        binding_name: Option<&str>,
    ) -> AotResult<ir::VarRef> {
        if binding_name.is_none() {
            return Ok(field);
        }
        let dest = self.tuple_field_dest(index, expected_ty, binding_name);
        self.current_block_mut(func)?.push(ir::Instruction::Copy {
            dest: dest.clone(),
            src: field,
        });
        Ok(dest)
    }

    fn tuple_field_dest(
        &mut self,
        index: usize,
        ty: &types::StaticType,
        binding_name: Option<&str>,
    ) -> ir::VarRef {
        if let Some(name) = binding_name {
            ir::VarRef::new(format!("{name}#{}", index + 1), ty.clone())
        } else {
            self.temp(ty.clone())
        }
    }

    fn lower_tuple_index(
        &mut self,
        array: &ir::AotExpr,
        indices: &[ir::AotExpr],
        elem_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        if indices.len() != 1 {
            return Err(cranelift_unsupported(
                "Cranelift backend requires a single constant tuple index (Issue #7097)",
            ));
        }
        let ir::AotExpr::LitI64(index) = &indices[0] else {
            return Err(cranelift_unsupported(
                "Cranelift backend requires a constant tuple index (Issue #7097)",
            ));
        };
        if *index < 1 {
            return Err(cranelift_unsupported(
                "Cranelift backend tuple index must be one-based and positive (Issue #7097)",
            ));
        }
        let zero_based = usize::try_from(*index - 1).map_err(|_| {
            cranelift_unsupported(format!(
                "Cranelift backend tuple index {index} exceeds the host index range (Issue #7097)"
            ))
        })?;
        let field = match array {
            ir::AotExpr::TupleLit { elements } => {
                let Some(element) = elements.get(zero_based) else {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend tuple index {} is out of bounds for tuple length {} (Issue #7097)",
                        index,
                        elements.len()
                    )));
                };
                self.lower_expr(element, func)?
            }
            ir::AotExpr::Var { name, .. } => {
                let Some(fields) = self.tuple_vars.get(name) else {
                    return Err(cranelift_unsupported(format!(
                        "Cranelift backend could not resolve tuple variable `{name}` (Issue #7117)"
                    )));
                };
                fields.get(zero_based).cloned().ok_or_else(|| {
                    cranelift_unsupported(format!(
                        "Cranelift backend tuple index {} is out of bounds for `{name}` (Issue #7117)",
                        index
                    ))
                })?
            }
            _ => match array.get_type() {
                types::StaticType::Tuple(elements) => self
                    .lower_tuple_expr_fields(array, &elements, func, None)?
                    .get(zero_based)
                    .cloned()
                    .ok_or_else(|| {
                        cranelift_unsupported(format!(
                            "Cranelift backend tuple index {} is out of bounds for tuple length {} (Issue #7117)",
                            index,
                            elements.len()
                        ))
                    })?,
                _ => {
                    return Err(cranelift_unsupported(
                        "Cranelift backend lowers tuple indexing only for tuple literals, local tuple variables, or tuple-returning calls (Issue #7117)",
                    ));
                }
            },
        };
        if &field.ty == elem_ty {
            Ok(field)
        } else {
            Err(cranelift_unsupported(format!(
                "Cranelift backend tuple field type `{}` does not match expected `{}` (Issue #7097)",
                field.ty.julia_type_name(),
                elem_ty.julia_type_name()
            )))
        }
    }

    fn cranelift_math_builtin_name(builtin: ir::AotBuiltinOp) -> Option<&'static str> {
        match builtin {
            ir::AotBuiltinOp::Sqrt => Some("sqrt"),
            ir::AotBuiltinOp::Sin => Some("sin"),
            ir::AotBuiltinOp::Cos => Some("cos"),
            ir::AotBuiltinOp::Exp => Some("exp"),
            ir::AotBuiltinOp::Log => Some("log"),
            ir::AotBuiltinOp::Abs => Some("abs"),
            _ => None,
        }
    }

    fn lower_call_args(
        &mut self,
        args: &[ir::AotExpr],
        func: &mut ir::IrFunction,
    ) -> AotResult<Vec<ir::VarRef>> {
        args.iter().map(|arg| self.lower_expr(arg, func)).collect()
    }

    fn lower_short_circuit_bool(
        &mut self,
        op: ir::AotBinOp,
        left: &ir::AotExpr,
        right: &ir::AotExpr,
        result_ty: &types::StaticType,
        func: &mut ir::IrFunction,
    ) -> AotResult<ir::VarRef> {
        if *result_ty != types::StaticType::Bool
            || left.get_type() != types::StaticType::Bool
            || right.get_type() != types::StaticType::Bool
        {
            return Err(cranelift_unsupported(
                "Cranelift backend currently lowers short-circuit `&&` / `||` only for Bool operands and Bool result",
            ));
        }

        let source_label = self.current_block.clone();
        let rhs_label = self.fresh_block_label("short_circuit_rhs");
        let const_label = self.fresh_block_label("short_circuit_const");
        let join_label = self.fresh_block_label("short_circuit_join");

        let left_value = self.lower_expr(left, func)?;
        let (then_block, else_block, const_value) = match op {
            ir::AotBinOp::And => (rhs_label.clone(), const_label.clone(), false),
            ir::AotBinOp::Or => (const_label.clone(), rhs_label.clone(), true),
            _ => unreachable!("short-circuit lowering called with non-logical operator"),
        };
        self.current_block_mut(func)?
            .set_terminator(ir::Terminator::Branch {
                cond: left_value,
                then_block,
                else_block,
            });

        self.add_block(func, rhs_label.clone());
        self.current_block = rhs_label;
        let right_value = self.lower_expr(right, func)?;
        let rhs_end_label = self.current_block.clone();
        self.current_block_mut(func)?
            .set_terminator(ir::Terminator::Jump(join_label.clone()));

        self.add_block(func, const_label.clone());
        self.current_block = const_label.clone();
        let const_value = self.lower_const(func, ir::ConstValue::Bool(const_value))?;
        self.current_block_mut(func)?
            .set_terminator(ir::Terminator::Jump(join_label.clone()));

        let dest = self.temp(types::StaticType::Bool);
        self.add_block(func, join_label.clone());
        self.current_block = join_label;
        self.current_block_mut(func)?.push(ir::Instruction::Phi {
            dest: dest.clone(),
            incoming: vec![(rhs_end_label, right_value), (const_label, const_value)],
        });

        debug_assert_ne!(source_label, self.current_block);
        Ok(dest)
    }

    fn lower_const(
        &mut self,
        func: &mut ir::IrFunction,
        value: ir::ConstValue,
    ) -> AotResult<ir::VarRef> {
        let dest = self.temp(value.get_type());
        self.current_block_mut(func)?
            .push(ir::Instruction::LoadConst {
                dest: dest.clone(),
                value,
            });
        Ok(dest)
    }

    fn temp(&mut self, ty: types::StaticType) -> ir::VarRef {
        let name = format!("__tmp{}", self.temp_index);
        self.temp_index += 1;
        ir::VarRef::new(name, ty)
    }

    fn fresh_block_label(&mut self, prefix: &str) -> String {
        self.block_index += 1;
        format!("{}_{}", prefix, self.block_index)
    }

    fn add_block(&self, func: &mut ir::IrFunction, label: String) {
        func.add_block(ir::BasicBlock::new(label));
    }

    fn current_block_mut<'a>(
        &self,
        func: &'a mut ir::IrFunction,
    ) -> AotResult<&'a mut ir::BasicBlock> {
        let name = func.name.clone();
        let current = self.current_block.clone();
        func.blocks
            .iter_mut()
            .find(|block| block.label == current)
            .ok_or_else(|| {
                AotError::InternalError(format!(
                    "Cranelift lowering lost block `{current}` for `{name}`"
                ))
            })
    }

    fn current_block_terminated(&self, func: &mut ir::IrFunction) -> AotResult<bool> {
        Ok(self.current_block_mut(func)?.terminator.is_some())
    }

    fn ensure_terminator(&self, func: &mut ir::IrFunction) -> AotResult<()> {
        if self.current_block_terminated(func)? {
            return Ok(());
        }
        if func.return_type == types::StaticType::Nothing {
            self.current_block_mut(func)?
                .set_terminator(ir::Terminator::Return(None));
            Ok(())
        } else {
            Err(cranelift_unsupported(format!(
                "Cranelift backend requires explicit return for `{}`",
                func.name
            )))
        }
    }

    fn map_binop(op: ir::AotBinOp) -> AotResult<ir::BinOpKind> {
        match op {
            ir::AotBinOp::Add => Ok(ir::BinOpKind::Add),
            ir::AotBinOp::Sub => Ok(ir::BinOpKind::Sub),
            ir::AotBinOp::Mul => Ok(ir::BinOpKind::Mul),
            ir::AotBinOp::Div | ir::AotBinOp::IntDiv => Ok(ir::BinOpKind::Div),
            ir::AotBinOp::Mod => Ok(ir::BinOpKind::Rem),
            ir::AotBinOp::Pow => Ok(ir::BinOpKind::Pow),
            ir::AotBinOp::Eq | ir::AotBinOp::Egal => Ok(ir::BinOpKind::Eq),
            ir::AotBinOp::Ne | ir::AotBinOp::NotEgal => Ok(ir::BinOpKind::Ne),
            ir::AotBinOp::Lt => Ok(ir::BinOpKind::Lt),
            ir::AotBinOp::Le => Ok(ir::BinOpKind::Le),
            ir::AotBinOp::Gt => Ok(ir::BinOpKind::Gt),
            ir::AotBinOp::Ge => Ok(ir::BinOpKind::Ge),
            ir::AotBinOp::And => Ok(ir::BinOpKind::And),
            ir::AotBinOp::Or => Ok(ir::BinOpKind::Or),
            ir::AotBinOp::BitAnd => Ok(ir::BinOpKind::BitAnd),
            ir::AotBinOp::BitOr => Ok(ir::BinOpKind::BitOr),
            ir::AotBinOp::BitXor => Ok(ir::BinOpKind::BitXor),
            ir::AotBinOp::Shl => Ok(ir::BinOpKind::Shl),
            ir::AotBinOp::Shr => Ok(ir::BinOpKind::Shr),
            ir::AotBinOp::Subtype => Err(cranelift_unsupported(
                "Cranelift backend does not lower subtype comparison",
            )),
        }
    }

    fn map_unaryop(op: ir::AotUnaryOp) -> AotResult<ir::UnaryOpKind> {
        match op {
            ir::AotUnaryOp::Neg => Ok(ir::UnaryOpKind::Neg),
            ir::AotUnaryOp::Not => Ok(ir::UnaryOpKind::Not),
            ir::AotUnaryOp::BitNot => Ok(ir::UnaryOpKind::BitNot),
            ir::AotUnaryOp::Pos => Err(cranelift_unsupported(
                "Cranelift lowering should handle unary plus before codegen",
            )),
        }
    }
}

/// Return the residual `subset_julia_vm_runtime` reference lines (if any) in
/// generated pure-Rust output, so callers can tell the user exactly which
/// lines blocked standalone compilation (Issue #6926).
pub fn pure_rust_runtime_references(rust_code: &str) -> Vec<String> {
    rust_code
        .lines()
        .filter(|line| {
            line.contains("extern crate subset_julia_vm_runtime")
                || line.contains("use subset_julia_vm_runtime::")
                || line.contains("subset_julia_vm_runtime::")
        })
        .map(|line| line.trim().to_string())
        .collect()
}

/// Generate a detailed error message for pure Rust mode failures.
pub fn generate_pure_rust_error(aot_program: &ir::AotProgram, dynamic_count: usize) -> AotError {
    let diagnostics = aot_program.diagnose_dynamic_operations();
    let mut error_msg = format!(
        "Pure Rust mode requires fully static types, but {} dynamic operation(s) were found.\n\n",
        dynamic_count
    );

    if !diagnostics.is_empty() {
        error_msg.push_str("Dynamic operations detected:\n");
        error_msg.push_str(&"─".repeat(60));
        error_msg.push('\n');
        for (i, diag) in diagnostics.iter().enumerate() {
            if i > 0 {
                error_msg.push('\n');
            }
            error_msg.push_str(&format!("{}. {}\n", i + 1, diag));
        }
        error_msg.push_str(&"─".repeat(60));
        error_msg.push_str("\n\n");
    }

    error_msg.push_str("To fix this:\n");
    error_msg.push_str(
        "  1. Add explicit type annotations to all function parameters and return types\n",
    );
    error_msg.push_str("  2. Use typed local variables (e.g., x::Float64 = 1.0)\n");
    error_msg.push_str("  3. Replace broadcasting operators (.+, .*, etc.) with explicit loops\n");
    error_msg.push_str("  4. Avoid operations that require runtime type dispatch\n");

    AotError::CodegenError(error_msg)
}

/// Compile persisted Core IR bytes to Rust code.
///
/// This is the main entry point for AoT compilation from serialized Core IR
/// (the format produced by [`crate::core_ir_file::save_to_bytes`]).
///
/// # Arguments
///
/// * `ir_bytes` - Persisted Core IR bytes
///
/// # Returns
///
/// Returns `AotOutput` containing the generated Rust code and statistics.
pub fn compile_from_ir_bytes(ir_bytes: &[u8]) -> AotResult<AotOutput> {
    let program = crate::core_ir_file::load_from_bytes(ir_bytes)
        .map_err(|e| AotError::InvalidIR(format!("Failed to load Core IR bytes: {}", e)))?;
    let config = CompileConfig::default();
    Ok(compile_program(program, &config)?.output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aot::codegen::CAbiExport;
    use crate::ir::core::{
        BinaryOp, Block, Expr, Function, Literal, MetaAnnotation, Program, Stmt, TypedParam,
    };
    #[cfg(feature = "cranelift")]
    use crate::ir::core::{EnumDef, EnumMember};
    use crate::span::Span;
    use crate::types::JuliaType;
    use std::sync::Arc;

    fn empty_program() -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
            type_aliases: vec![],
            structs: vec![],
            functions: vec![],
            base_function_count: 0,
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: Block {
                stmts: vec![],
                span: Span::new(0, 0, 1, 1, 0, 0),
            },
        }
    }

    fn scalar_bool_main_program() -> Program {
        let mut program = empty_program();
        let span = Span::new(0, 4, 1, 1, 1, 5);
        program.main.stmts.push(Stmt::Expr {
            expr: Expr::Literal(Literal::Bool(true), span),
            span,
        });
        program
    }

    #[test]
    fn script_entry_lifts_main_and_preserves_spans_issue_2() {
        let mut program = scalar_bool_main_program();
        let main_span = program.main.span;
        let statement = program.main.stmts[0].clone();

        script_entry::lift_script_entry(&mut program).expect("script entry should lift main");

        assert!(program.main.stmts.is_empty());
        assert_eq!(program.functions.len(), 1);
        let entry = &program.functions[0];
        assert_eq!(entry.name, SCRIPT_ENTRY_NAME);
        assert!(entry.params.is_empty());
        assert_eq!(entry.body.span, main_span);
        assert_eq!(entry.body.stmts.first(), Some(&statement));
        assert!(matches!(
            entry.body.stmts.last(),
            Some(Stmt::Return { value: None, .. })
        ));
    }

    #[test]
    fn script_entry_rejects_reserved_name_collision_issue_2() {
        let mut program = empty_program();
        let span = Span::new(0, 4, 1, 1, 1, 5);
        program.functions.push(Arc::new(Function {
            name: SCRIPT_ENTRY_NAME.to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            new_struct_name: None,
            span,
        }));

        let error = script_entry::lift_script_entry(&mut program)
            .expect_err("reserved name must be rejected");

        assert!(error.to_string().contains(SCRIPT_ENTRY_NAME));
    }

    #[test]
    fn default_compile_config_does_not_request_script_entry_issue_2() {
        assert!(!CompileConfig::default().requests_script_entry());
    }

    #[cfg(feature = "aot-wasm")]
    #[test]
    fn script_entry_survives_source_to_wasm_pipeline_issue_2() {
        let mut config = CompileConfig {
            backend: AotBackend::Wasm,
            ..CompileConfig::default()
        };
        config.enable_script_entry();

        let output = compile_wasm_source("x = 40\ny = 2\nx + y\n", &config)
            .expect("top-level source should compile through the script entry");

        assert!(output
            .wasm_bytes
            .windows(SCRIPT_ENTRY_NAME.len())
            .any(|window| window == SCRIPT_ENTRY_NAME.as_bytes()));
    }

    #[cfg(feature = "aot-wasm")]
    #[test]
    fn script_entry_eliminates_captured_closure_types_issue_3() {
        let source = r#"
function gamma(exponent::Float64)
    return function(channel::Float64)::Float64
        adjusted = channel ^ exponent
        return adjusted
    end
end
correct = gamma(0.85)
result = correct(0.25)
"#;
        let program = crate::pipeline::parse_source(source).expect("source should lower");
        let mut config = CompileConfig {
            backend: AotBackend::Wasm,
            ..CompileConfig::default()
        };
        config.enable_script_entry();

        let prepared = prepare_aot_program(program, &config).expect("AoT preparation should pass");
        let entry = prepared
            .aot_program
            .functions
            .iter()
            .find(|function| function.name == SCRIPT_ENTRY_NAME)
            .expect("script entry should exist");
        let rendered = format!("{:#?}", entry.body);
        assert!(!rendered.contains("Lambda"), "closure survived: {rendered}");
        assert!(
            !rendered.contains("Function {"),
            "function slot survived: {rendered}"
        );
        let program_rendered = format!("{:#?}", prepared.aot_program.functions);
        assert!(
            !program_rendered.contains("Lambda")
                && !program_rendered.contains("return_type: Function")
                && !program_rendered.contains("ty: Function"),
            "closure helper survived: {program_rendered}"
        );
    }

    #[cfg(feature = "aot-wasm")]
    #[test]
    fn imported_array_call_survives_aot_conversion_issue_5() {
        let source = r#"
load(path::String)::Array{UInt8,3} = Array{UInt8,3}(undef, 0, 0, 0)
image = load("inputs/input.png")
"#;
        let program = crate::pipeline::parse_source(source).expect("source should lower");
        let mut config = CompileConfig {
            backend: AotBackend::Wasm,
            wasm_imports: vec![WasmImport {
                module: "sjulia_host".to_string(),
                name: "load".to_string(),
                function_name: "load".to_string(),
                params: vec![types::StaticType::Str],
                result: Some(types::StaticType::Array {
                    element: Box::new(types::StaticType::U8),
                    ndims: Some(3),
                }),
            }],
            ..CompileConfig::default()
        };
        config.enable_script_entry();

        let prepared = prepare_aot_program(program, &config).expect("AoT preparation should pass");
        let entry = prepared
            .aot_program
            .functions
            .iter()
            .find(|function| function.name == SCRIPT_ENTRY_NAME)
            .expect("script entry should exist");

        assert!(
            !format!("{:?}", entry.body).contains("LitNothing"),
            "script entry contains a placeholder: {:?}",
            entry.body
        );
        assert!(format!("{:?}", entry.body).contains("CallStatic"));
    }

    #[cfg(feature = "aot-wasm")]
    #[test]
    fn wasm_static_payloads_share_one_data_section_issue_6() {
        fn read_leb(bytes: &[u8], cursor: &mut usize) -> usize {
            let mut value = 0_usize;
            let mut shift = 0;
            loop {
                let byte = bytes[*cursor];
                *cursor += 1;
                value |= usize::from(byte & 0x7f) << shift;
                if byte & 0x80 == 0 {
                    return value;
                }
                shift += 7;
            }
        }

        let mut config = CompileConfig {
            backend: AotBackend::Wasm,
            wasm_imports: vec![WasmImport {
                module: "host".to_string(),
                name: "value".to_string(),
                function_name: "host_value".to_string(),
                params: vec![types::StaticType::Str],
                result: Some(types::StaticType::I64),
            }],
            ..CompileConfig::default()
        };
        config
            .c_abi_exports
            .push(codegen::CAbiExport::new("answer", "answer"));
        let source = r#"
host_value(value::String)::Int64 = 0
answer()::Int64 = host_value("static payload")
"#;

        let output = compile_wasm_source(source, &config).expect("module should compile");
        let mut cursor = 8;
        let mut data_sections = 0;
        while cursor < output.wasm_bytes.len() {
            let section = output.wasm_bytes[cursor];
            cursor += 1;
            let size = read_leb(&output.wasm_bytes, &mut cursor);
            if section == 11 {
                data_sections += 1;
            }
            cursor += size;
        }

        assert_eq!(data_sections, 1);
    }

    #[cfg(not(feature = "cranelift"))]
    #[test]
    fn cranelift_backend_requires_feature_issue_6927() {
        let config = CompileConfig {
            backend: AotBackend::Cranelift,
            ..CompileConfig::default()
        };
        let err = match compile_program(scalar_bool_main_program(), &config) {
            Ok(_) => panic!("expected Cranelift feature diagnostic"),
            Err(err) => err,
        };
        let msg = err.to_string();

        assert!(msg.contains("Cranelift backend was selected"));
        assert!(msg.contains("cranelift` feature"));
        assert!(msg.contains("#6927"));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_backend_reaches_generator_issue_6927() {
        let config = CompileConfig {
            backend: AotBackend::Cranelift,
            ..CompileConfig::default()
        };
        let result = compile_program(scalar_bool_main_program(), &config).unwrap();

        assert!(result
            .output
            .rust_code
            .contains("Cranelift: compiled module juliars_cranelift with 2 functions"));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_jit_run_executes_main_entry_issue_7131() {
        let config = CompileConfig {
            backend: AotBackend::Cranelift,
            ..CompileConfig::default()
        };
        let result = run_cranelift_jit_main(scalar_bool_main_program(), &config).unwrap();

        assert!(result.timings.iter().any(|(stage, _)| *stage == "jit-main"));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_object_debug_info_uses_core_spans_issue_7090() {
        use object::{Object, ObjectSection};

        let mut program = scalar_bool_main_program();
        program.main.span = Span::new(0, 4, 9, 1, 9, 5);
        let config = CompileConfig {
            backend: AotBackend::Cranelift,
            debug_info: true,
            source_name: "issue_7090_pipeline.jl".to_string(),
            ..CompileConfig::default()
        };

        let result = compile_cranelift_object_for_target(program, &config, None).unwrap();
        let object_file = object::File::parse(&*result.object_bytes).unwrap();
        for section_name in [".debug_abbrev", ".debug_info", ".debug_line"] {
            let section = object_file
                .section_by_name(section_name)
                .unwrap_or_else(|| panic!("missing {section_name} section"));
            assert!(
                !section.data().unwrap().is_empty(),
                "{section_name} should not be empty"
            );
        }
        assert!(result
            .object_bytes
            .windows(b"issue_7090_pipeline.jl".len())
            .any(|window| window == b"issue_7090_pipeline.jl"));
        assert!(result
            .object_bytes
            .windows(b"__juliars_main".len())
            .any(|window| window == b"__juliars_main"));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_lowering_emits_standalone_main_wrapper_issue_7084() {
        let mut program = ir::AotProgram::new();
        program
            .main
            .push(ir::AotStmt::Expr(ir::AotExpr::LitI64(42)));

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let wrapper = module
            .functions
            .iter()
            .find(|func| func.name == "main")
            .expect("Cranelift module should contain standalone C main wrapper");

        assert!(wrapper.params.is_empty());
        assert_eq!(wrapper.return_type, types::StaticType::I32);
        let entry = wrapper.entry_block().unwrap();
        assert!(matches!(
            entry.instructions.first(),
            Some(ir::Instruction::Call {
                dest: None,
                func,
                args,
            }) if func == "__juliars_main" && args.is_empty()
        ));
        assert!(matches!(
            entry.instructions.get(1),
            Some(ir::Instruction::LoadConst {
                dest,
                value: ir::ConstValue::Int32(0),
            }) if dest.name == "__sjulia_exit_code" && dest.ty == types::StaticType::I32
        ));
        assert!(matches!(
            &entry.terminator,
            Some(ir::Terminator::Return(Some(value)))
                if value.name == "__sjulia_exit_code" && value.ty == types::StaticType::I32
        ));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_char_lowers_as_i32_codepoint_issue_7101() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        let mut program = ir::AotProgram::new();
        let mut id_char = ir::AotFunction::new(
            "id_char".to_string(),
            vec![("value".to_string(), types::StaticType::Char)],
            types::StaticType::Char,
        );
        id_char
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::Var {
                name: "value".to_string(),
                ty: types::StaticType::Char,
            })));
        program.add_function(id_char);
        program.main.push(ir::AotStmt::Let {
            name: "ch".to_string(),
            ty: types::StaticType::Char,
            value: ir::AotExpr::LitChar('λ'),
            is_mutable: false,
        });
        program
            .main
            .push(ir::AotStmt::Expr(ir::AotExpr::CallStatic {
                function: "id_char".to_string(),
                args: vec![ir::AotExpr::Var {
                    name: "ch".to_string(),
                    ty: types::StaticType::Char,
                }],
                return_ty: types::StaticType::Char,
                inline_policy: ir::AotInlinePolicy::Never,
            }));

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let id_char = module
            .functions
            .iter()
            .find(|func| func.name == "id_char")
            .unwrap();
        assert_eq!(
            id_char.params,
            vec![("value".to_string(), types::StaticType::Char)]
        );
        assert_eq!(id_char.return_type, types::StaticType::Char);

        let main = module
            .functions
            .iter()
            .find(|func| func.name == "__juliars_main")
            .unwrap();
        assert!(main.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(
                    inst,
                    ir::Instruction::LoadConst {
                        dest,
                        value: ir::ConstValue::Char('λ'),
                    } if dest.ty == types::StaticType::Char
                )
            })
        }));

        CraneliftCodeGenerator::new()
            .and_then(|mut codegen| {
                codegen.generate_module(&module).map_err(|err| {
                    crate::aot::codegen::cranelift::CraneliftError::FunctionCompilation(
                        err.to_string(),
                    )
                })
            })
            .unwrap();
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_aot_lowerer_accepts_i128_u128_issue_7092() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        let mut program = ir::AotProgram::new();
        let mut add_i128 = ir::AotFunction::new(
            "add_i128".to_string(),
            vec![
                ("x".to_string(), types::StaticType::I128),
                ("y".to_string(), types::StaticType::I128),
            ],
            types::StaticType::I128,
        );
        add_i128
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(ir::AotExpr::Var {
                    name: "x".to_string(),
                    ty: types::StaticType::I128,
                }),
                right: Box::new(ir::AotExpr::Var {
                    name: "y".to_string(),
                    ty: types::StaticType::I128,
                }),
                result_ty: types::StaticType::I128,
            })));
        program.add_function(add_i128);

        let mut id_u128 = ir::AotFunction::new(
            "id_u128".to_string(),
            vec![("value".to_string(), types::StaticType::U128)],
            types::StaticType::U128,
        );
        id_u128
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::Var {
                name: "value".to_string(),
                ty: types::StaticType::U128,
            })));
        program.add_function(id_u128);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let add_i128 = module
            .functions
            .iter()
            .find(|func| func.name == "add_i128")
            .unwrap();
        assert_eq!(
            add_i128.params,
            vec![
                ("x".to_string(), types::StaticType::I128),
                ("y".to_string(), types::StaticType::I128),
            ]
        );
        assert_eq!(add_i128.return_type, types::StaticType::I128);

        let id_u128 = module
            .functions
            .iter()
            .find(|func| func.name == "id_u128")
            .unwrap();
        assert_eq!(
            id_u128.params,
            vec![("value".to_string(), types::StaticType::U128)]
        );
        assert_eq!(id_u128.return_type, types::StaticType::U128);

        CraneliftCodeGenerator::new()
            .and_then(|mut codegen| {
                codegen.generate_module(&module).map_err(|err| {
                    crate::aot::codegen::cranelift::CraneliftError::FunctionCompilation(
                        err.to_string(),
                    )
                })
            })
            .unwrap();
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_aot_lowerer_accepts_f16_issue_7093() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        let mut program = ir::AotProgram::new();
        let mut add_f16 = ir::AotFunction::new(
            "add_f16".to_string(),
            vec![
                ("x".to_string(), types::StaticType::F16),
                ("y".to_string(), types::StaticType::F16),
            ],
            types::StaticType::F16,
        );
        add_f16
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(ir::AotExpr::Var {
                    name: "x".to_string(),
                    ty: types::StaticType::F16,
                }),
                right: Box::new(ir::AotExpr::Var {
                    name: "y".to_string(),
                    ty: types::StaticType::F16,
                }),
                result_ty: types::StaticType::F16,
            })));
        program.add_function(add_f16);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let add_f16 = module
            .functions
            .iter()
            .find(|func| func.name == "add_f16")
            .unwrap();
        assert_eq!(
            add_f16.params,
            vec![
                ("x".to_string(), types::StaticType::F16),
                ("y".to_string(), types::StaticType::F16),
            ]
        );
        assert_eq!(add_f16.return_type, types::StaticType::F16);

        CraneliftCodeGenerator::new()
            .and_then(|mut codegen| {
                codegen.generate_module(&module).map_err(|err| {
                    crate::aot::codegen::cranelift::CraneliftError::FunctionCompilation(
                        err.to_string(),
                    )
                })
            })
            .unwrap();
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_aot_lowerer_preserves_short_circuit_cfg_issue_7115() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        fn bool_var(name: &str) -> ir::AotExpr {
            ir::AotExpr::Var {
                name: name.to_string(),
                ty: types::StaticType::Bool,
            }
        }

        let mut program = ir::AotProgram::new();
        for (name, op) in [
            ("bool_and", ir::AotBinOp::And),
            ("bool_or", ir::AotBinOp::Or),
        ] {
            let mut func = ir::AotFunction::new(
                name.to_string(),
                vec![
                    ("left".to_string(), types::StaticType::Bool),
                    ("right".to_string(), types::StaticType::Bool),
                ],
                types::StaticType::Bool,
            );
            func.body
                .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                    op,
                    left: Box::new(bool_var("left")),
                    right: Box::new(bool_var("right")),
                    result_ty: types::StaticType::Bool,
                })));
            program.add_function(func);
        }

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        for func_name in ["bool_and", "bool_or"] {
            let func = module
                .functions
                .iter()
                .find(|func| func.name == func_name)
                .unwrap();
            assert!(
                func.blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Some(ir::Terminator::Branch { .. }))),
                "{func_name} should branch before evaluating the RHS"
            );
            assert!(
                func.blocks.iter().any(|block| {
                    block.instructions.iter().any(|inst| {
                        matches!(
                            inst,
                            ir::Instruction::Phi {
                                dest,
                                incoming,
                            } if dest.ty == types::StaticType::Bool && incoming.len() == 2
                        )
                    })
                }),
                "{func_name} should join short-circuit values through a Bool phi"
            );
        }

        CraneliftCodeGenerator::new()
            .and_then(|mut codegen| {
                codegen.generate_module(&module).map_err(|err| {
                    crate::aot::codegen::cranelift::CraneliftError::FunctionCompilation(
                        err.to_string(),
                    )
                })
            })
            .unwrap();
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_tuple_local_and_field_access_issue_7097() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        fn i64_var(name: &str) -> ir::AotExpr {
            ir::AotExpr::Var {
                name: name.to_string(),
                ty: types::StaticType::I64,
            }
        }

        let tuple_ty =
            types::StaticType::Tuple(vec![types::StaticType::I64, types::StaticType::I64]);
        let mut program = ir::AotProgram::new();

        let mut second_from_tuple = ir::AotFunction::new(
            "second_from_tuple".to_string(),
            vec![
                ("x".to_string(), types::StaticType::I64),
                ("y".to_string(), types::StaticType::I64),
            ],
            types::StaticType::I64,
        );
        second_from_tuple.body.push(ir::AotStmt::Let {
            name: "pair".to_string(),
            ty: tuple_ty.clone(),
            value: ir::AotExpr::TupleLit {
                elements: vec![i64_var("x"), i64_var("y")],
            },
            is_mutable: false,
        });
        second_from_tuple
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::Index {
                array: Box::new(ir::AotExpr::Var {
                    name: "pair".to_string(),
                    ty: tuple_ty.clone(),
                }),
                indices: vec![ir::AotExpr::LitI64(2)],
                elem_ty: types::StaticType::I64,
                is_tuple: true,
            })));
        program.add_function(second_from_tuple);

        let mut first_from_literal = ir::AotFunction::new(
            "first_from_literal".to_string(),
            vec![
                ("x".to_string(), types::StaticType::I64),
                ("y".to_string(), types::StaticType::I64),
            ],
            types::StaticType::I64,
        );
        first_from_literal
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::Index {
                array: Box::new(ir::AotExpr::TupleLit {
                    elements: vec![i64_var("x"), i64_var("y")],
                }),
                indices: vec![ir::AotExpr::LitI64(1)],
                elem_ty: types::StaticType::I64,
                is_tuple: true,
            })));
        program.add_function(first_from_literal);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let second = module
            .functions
            .iter()
            .find(|func| func.name == "second_from_tuple")
            .unwrap();
        assert!(second.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(
                    inst,
                    ir::Instruction::Copy { dest, .. } if dest.name == "pair#2"
                )
            })
        }));

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen.generate_module(&module).unwrap();
        unsafe {
            let second_from_tuple: fn(i64, i64) -> i64 =
                codegen.get_typed_function("second_from_tuple").unwrap();
            let first_from_literal: fn(i64, i64) -> i64 =
                codegen.get_typed_function("first_from_literal").unwrap();
            assert_eq!(second_from_tuple(4, 9), 9);
            assert_eq!(first_from_literal(4, 9), 4);
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_tuple_return_and_destructuring_issue_7117() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        fn i64_var(name: &str) -> ir::AotExpr {
            ir::AotExpr::Var {
                name: name.to_string(),
                ty: types::StaticType::I64,
            }
        }

        let tuple_ty =
            types::StaticType::Tuple(vec![types::StaticType::I64, types::StaticType::I64]);
        let mut program = ir::AotProgram::new();

        let mut pair = ir::AotFunction::new("pair".to_string(), vec![], tuple_ty.clone());
        pair.body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::TupleLit {
                elements: vec![ir::AotExpr::LitI64(1), ir::AotExpr::LitI64(2)],
            })));
        program.add_function(pair);

        let mut sum_pair =
            ir::AotFunction::new("sum_pair".to_string(), vec![], types::StaticType::I64);
        sum_pair.body.push(ir::AotStmt::Let {
            name: "tmp".to_string(),
            ty: tuple_ty.clone(),
            value: ir::AotExpr::CallStatic {
                function: "pair".to_string(),
                args: vec![],
                return_ty: tuple_ty.clone(),
                inline_policy: ir::AotInlinePolicy::Auto,
            },
            is_mutable: false,
        });
        sum_pair.body.push(ir::AotStmt::Let {
            name: "a".to_string(),
            ty: types::StaticType::I64,
            value: ir::AotExpr::Index {
                array: Box::new(ir::AotExpr::Var {
                    name: "tmp".to_string(),
                    ty: tuple_ty.clone(),
                }),
                indices: vec![ir::AotExpr::LitI64(1)],
                elem_ty: types::StaticType::I64,
                is_tuple: true,
            },
            is_mutable: false,
        });
        sum_pair.body.push(ir::AotStmt::Let {
            name: "b".to_string(),
            ty: types::StaticType::I64,
            value: ir::AotExpr::Index {
                array: Box::new(ir::AotExpr::Var {
                    name: "tmp".to_string(),
                    ty: tuple_ty.clone(),
                }),
                indices: vec![ir::AotExpr::LitI64(2)],
                elem_ty: types::StaticType::I64,
                is_tuple: true,
            },
            is_mutable: false,
        });
        sum_pair
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(i64_var("a")),
                right: Box::new(i64_var("b")),
                result_ty: types::StaticType::I64,
            })));
        program.add_function(sum_pair);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let pair = module
            .functions
            .iter()
            .find(|func| func.name == "pair")
            .unwrap();
        assert!(
            pair.blocks
                .iter()
                .any(|block| matches!(block.terminator, Some(ir::Terminator::ReturnMany(ref vars)) if vars.len() == 2)),
            "tuple-returning function should lower to ReturnMany"
        );

        let sum_pair = module
            .functions
            .iter()
            .find(|func| func.name == "sum_pair")
            .unwrap();
        assert!(
            sum_pair.blocks.iter().any(|block| {
                block.instructions.iter().any(|inst| {
                    matches!(
                        inst,
                        ir::Instruction::CallMulti {
                            dests,
                            func,
                            args,
                        } if func == "pair" && args.is_empty() && dests.len() == 2
                    )
                })
            }),
            "destructuring a tuple-returning call should lower to CallMulti"
        );

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen.generate_module(&module).unwrap();
        unsafe {
            let sum_pair: fn() -> i64 = codegen.get_typed_function("sum_pair").unwrap();
            assert_eq!(sum_pair(), 3);
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_string_local_literal_lowers_issue_7094() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        let mut program = ir::AotProgram::new();
        let mut func =
            ir::AotFunction::new("string_local".to_string(), vec![], types::StaticType::I64);
        func.body.push(ir::AotStmt::Let {
            name: "s".to_string(),
            ty: types::StaticType::Str,
            value: ir::AotExpr::LitStr("hello".to_string()),
            is_mutable: false,
        });
        func.body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::CallBuiltin {
                builtin: ir::AotBuiltinOp::Length,
                args: vec![ir::AotExpr::Var {
                    name: "s".to_string(),
                    ty: types::StaticType::Str,
                }],
                return_ty: types::StaticType::I64,
            })));
        program.add_function(func);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let string_local = module
            .functions
            .iter()
            .find(|func| func.name == "string_local")
            .unwrap();
        assert!(string_local.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(
                    inst,
                    ir::Instruction::LoadConst {
                        value: ir::ConstValue::String(value),
                        ..
                    } if value == "hello"
                )
            })
        }));

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen.generate_module(&module).unwrap();
        unsafe {
            let string_local: fn() -> i64 = codegen.get_typed_function("string_local").unwrap();
            assert_eq!(string_local(), 5);
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_struct_field_layout_load_store_issue_7095() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        fn point_var() -> ir::AotExpr {
            ir::AotExpr::Var {
                name: "p".to_string(),
                ty: types::StaticType::Struct {
                    type_id: 0,
                    name: "Point".to_string(),
                },
            }
        }

        let point_ty = types::StaticType::Struct {
            type_id: 0,
            name: "Point".to_string(),
        };
        let mut program = ir::AotProgram::new();
        let mut point = ir::AotStruct::new("Point".to_string(), true);
        point.add_field("x".to_string(), types::StaticType::I64);
        point.add_field("pad".to_string(), types::StaticType::I32);
        point.add_field("y".to_string(), types::StaticType::I64);
        program.add_struct(point);

        let mut sum_point =
            ir::AotFunction::new("sum_point".to_string(), vec![], types::StaticType::I64);
        sum_point.body.push(ir::AotStmt::Let {
            name: "p".to_string(),
            ty: point_ty.clone(),
            value: ir::AotExpr::StructNew {
                name: "Point".to_string(),
                fields: vec![
                    ir::AotExpr::LitI64(1),
                    ir::AotExpr::LitI32(3),
                    ir::AotExpr::LitI64(2),
                ],
            },
            is_mutable: true,
        });
        sum_point.body.push(ir::AotStmt::Assign {
            target: ir::AotExpr::FieldAccess {
                object: Box::new(point_var()),
                field: "x".to_string(),
                field_ty: types::StaticType::I64,
            },
            value: ir::AotExpr::LitI64(5),
        });
        sum_point.body.push(ir::AotStmt::Let {
            name: "x".to_string(),
            ty: types::StaticType::I64,
            value: ir::AotExpr::FieldAccess {
                object: Box::new(point_var()),
                field: "x".to_string(),
                field_ty: types::StaticType::I64,
            },
            is_mutable: false,
        });
        sum_point.body.push(ir::AotStmt::Let {
            name: "y".to_string(),
            ty: types::StaticType::I64,
            value: ir::AotExpr::FieldAccess {
                object: Box::new(point_var()),
                field: "y".to_string(),
                field_ty: types::StaticType::I64,
            },
            is_mutable: false,
        });
        sum_point
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(ir::AotExpr::Var {
                    name: "y".to_string(),
                    ty: types::StaticType::I64,
                }),
                right: Box::new(ir::AotExpr::Var {
                    name: "x".to_string(),
                    ty: types::StaticType::I64,
                }),
                result_ty: types::StaticType::I64,
            })));
        program.add_function(sum_point);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let sum_point = module
            .functions
            .iter()
            .find(|func| func.name == "sum_point")
            .unwrap();
        let entry = sum_point.entry_block().unwrap();
        assert!(matches!(
            entry.instructions.iter().find_map(|inst| match inst {
                ir::Instruction::StructNew {
                    size,
                    align,
                    fields,
                    ..
                } => Some((*size, *align, fields.iter().map(|f| f.offset).collect::<Vec<_>>())),
                _ => None,
            }),
            Some((24, 8, offsets)) if offsets == vec![0, 8, 16]
        ));
        assert!(entry
            .instructions
            .iter()
            .any(|inst| { matches!(inst, ir::Instruction::SetFieldOffset { offset: 0, .. }) }));
        assert!(entry
            .instructions
            .iter()
            .any(|inst| { matches!(inst, ir::Instruction::GetFieldOffset { offset: 16, .. }) }));

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen.generate_module(&module).unwrap();
        unsafe {
            let sum_point: fn() -> i64 = codegen.get_typed_function("sum_point").unwrap();
            assert_eq!(sum_point(), 7);
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_complex_real_imag_abs2_arithmetic_issue_7099() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        fn complex_ty() -> types::StaticType {
            types::StaticType::Struct {
                type_id: 0,
                name: "ComplexF64".to_string(),
            }
        }

        fn complex_var(name: &str) -> ir::AotExpr {
            ir::AotExpr::Var {
                name: name.to_string(),
                ty: complex_ty(),
            }
        }

        fn complex_part(name: &str, builtin: ir::AotBuiltinOp) -> ir::AotExpr {
            ir::AotExpr::CallBuiltin {
                builtin,
                args: vec![complex_var(name)],
                return_ty: types::StaticType::F64,
            }
        }

        fn add_f64(left: ir::AotExpr, right: ir::AotExpr) -> ir::AotExpr {
            ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(left),
                right: Box::new(right),
                result_ty: types::StaticType::F64,
            }
        }

        let mut program = ir::AotProgram::new();
        let mut func =
            ir::AotFunction::new("complex_score".to_string(), vec![], types::StaticType::F64);
        func.body.push(ir::AotStmt::Let {
            name: "z".to_string(),
            ty: complex_ty(),
            value: ir::AotExpr::StructNew {
                name: "ComplexF64".to_string(),
                fields: vec![ir::AotExpr::LitF64(1.0), ir::AotExpr::LitF64(2.0)],
            },
            is_mutable: false,
        });
        func.body.push(ir::AotStmt::Let {
            name: "added".to_string(),
            ty: complex_ty(),
            value: ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(complex_var("z")),
                right: Box::new(complex_var("z")),
                result_ty: complex_ty(),
            },
            is_mutable: false,
        });
        func.body.push(ir::AotStmt::Let {
            name: "multiplied".to_string(),
            ty: complex_ty(),
            value: ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Mul,
                left: Box::new(complex_var("z")),
                right: Box::new(complex_var("z")),
                result_ty: complex_ty(),
            },
            is_mutable: false,
        });
        let added_sum = add_f64(
            complex_part("added", ir::AotBuiltinOp::Real),
            complex_part("added", ir::AotBuiltinOp::Imag),
        );
        let multiplied_sum = add_f64(
            complex_part("multiplied", ir::AotBuiltinOp::Real),
            complex_part("multiplied", ir::AotBuiltinOp::Imag),
        );
        let abs2 = ir::AotExpr::CallBuiltin {
            builtin: ir::AotBuiltinOp::Abs2,
            args: vec![complex_var("z")],
            return_ty: types::StaticType::F64,
        };
        func.body.push(ir::AotStmt::Return(Some(add_f64(
            add_f64(added_sum, multiplied_sum),
            abs2,
        ))));
        program.add_function(func);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let complex_score = module
            .functions
            .iter()
            .find(|func| func.name == "complex_score")
            .unwrap();
        let entry = complex_score.entry_block().unwrap();
        assert!(matches!(
            entry.instructions.iter().find_map(|inst| match inst {
                ir::Instruction::StructNew { size, align, fields, .. } => {
                    Some((*size, *align, fields.iter().map(|f| f.offset).collect::<Vec<_>>()))
                }
                _ => None,
            }),
            Some((16, 8, offsets)) if offsets == vec![0, 8]
        ));
        assert!(entry
            .instructions
            .iter()
            .any(|inst| { matches!(inst, ir::Instruction::GetFieldOffset { offset: 0, .. }) }));
        assert!(entry
            .instructions
            .iter()
            .any(|inst| { matches!(inst, ir::Instruction::GetFieldOffset { offset: 8, .. }) }));
        assert!(entry.instructions.iter().any(|inst| {
            matches!(
                inst,
                ir::Instruction::BinOp {
                    op: ir::BinOpKind::Mul,
                    ..
                }
            )
        }));

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen.generate_module(&module).unwrap();
        unsafe {
            let complex_score: fn() -> f64 = codegen.get_typed_function("complex_score").unwrap();
            assert_eq!(complex_score(), 12.0);
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_scalar_globals_lower_as_constants_issue_7103() {
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::CodeGenerator;

        let mut program = ir::AotProgram::new();
        program.add_global(ir::AotGlobal::with_init(
            "SCALE".to_string(),
            types::StaticType::I64,
            ir::AotExpr::LitI64(7),
        ));

        let mut add_scale = ir::AotFunction::new(
            "add_scale".to_string(),
            vec![("x".to_string(), types::StaticType::I64)],
            types::StaticType::I64,
        );
        add_scale
            .body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(ir::AotExpr::Var {
                    name: "x".to_string(),
                    ty: types::StaticType::I64,
                }),
                right: Box::new(ir::AotExpr::Var {
                    name: "SCALE".to_string(),
                    ty: types::StaticType::I64,
                }),
                result_ty: types::StaticType::I64,
            })));
        program.add_function(add_scale);

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let add_scale = module
            .functions
            .iter()
            .find(|func| func.name == "add_scale")
            .unwrap();
        assert!(add_scale.blocks.iter().any(|block| {
            block.instructions.iter().any(|inst| {
                matches!(
                    inst,
                    ir::Instruction::LoadConst {
                        dest,
                        value: ir::ConstValue::Int64(7),
                    } if dest.ty == types::StaticType::I64
                )
            })
        }));

        let mut codegen = CraneliftCodeGenerator::new().unwrap();
        codegen.generate_module(&module).unwrap();
        unsafe {
            let add_scale: fn(i64) -> i64 = codegen.get_typed_function("add_scale").unwrap();
            assert_eq!(add_scale(5), 12);
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_rejects_uninitialized_globals_issue_7103() {
        let mut program = ir::AotProgram::new();
        program.add_global(ir::AotGlobal::new(
            "MISSING_INIT".to_string(),
            types::StaticType::I64,
        ));

        let err = lower_aot_program_for_cranelift(&program, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("MISSING_INIT"),
            "missing global name in error: {msg}"
        );
        assert!(msg.contains("Issue #7103"), "missing issue marker: {msg}");
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_rejects_non_constant_global_initializers_issue_7103() {
        let mut program = ir::AotProgram::new();
        program.add_global(ir::AotGlobal::with_init(
            "DYNAMIC_INIT".to_string(),
            types::StaticType::F64,
            ir::AotExpr::CallBuiltin {
                builtin: ir::AotBuiltinOp::Rand,
                args: vec![],
                return_ty: types::StaticType::F64,
            },
        ));

        let err = lower_aot_program_for_cranelift(&program, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DYNAMIC_INIT"),
            "missing global name in error: {msg}"
        );
        assert!(
            msg.contains("scalar constant initializer"),
            "missing initializer context: {msg}"
        );
        assert!(msg.contains("Issue #7103"), "missing issue marker: {msg}");
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_enum_members_lower_as_i32_constants_issue_7096() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut program = empty_program();
        program.main.stmts.push(Stmt::EnumDef {
            enum_def: EnumDef {
                name: "Color".to_string(),
                base_type: "Int32".to_string(),
                members: vec![
                    EnumMember {
                        name: "red".to_string(),
                        value: 0,
                        span,
                    },
                    EnumMember {
                        name: "green".to_string(),
                        value: 1,
                        span,
                    },
                    EnumMember {
                        name: "blue".to_string(),
                        value: 2,
                        span,
                    },
                ],
                span,
            },
            published_members: None,
            span,
        });
        program.functions.push(Arc::new(Function {
            name: "pick_green".to_string(),
            params: vec![],
            kwparams: vec![],
            type_params: vec![],
            return_type: Some(JuliaType::Int32),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::var("green", span)),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            new_struct_name: None,
            span,
        }));
        program.main.stmts.push(Stmt::Expr {
            expr: Expr::Call {
                function: "pick_green".to_string().into(),
                args: vec![],
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            },
            span,
        });

        let config = CompileConfig {
            backend: AotBackend::Cranelift,
            ..CompileConfig::default()
        };
        let result = compile_program(program, &config).unwrap();

        assert!(result
            .output
            .rust_code
            .contains("Cranelift: compiled module juliars_cranelift"));
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_division_family_builtins_are_gated_issue_7119() {
        for builtin in [
            ir::AotBuiltinOp::Div,
            ir::AotBuiltinOp::Rem,
            ir::AotBuiltinOp::Mod,
            ir::AotBuiltinOp::Fld,
            ir::AotBuiltinOp::Cld,
        ] {
            let mut program = ir::AotProgram::new();
            program
                .main
                .push(ir::AotStmt::Expr(ir::AotExpr::CallBuiltin {
                    builtin,
                    args: vec![ir::AotExpr::LitI64(5), ir::AotExpr::LitI64(3)],
                    return_ty: types::StaticType::I64,
                }));

            let err = lower_aot_program_for_cranelift(&program, None).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains(&format!("builtin `{}`", builtin)),
                "missing builtin name for {builtin}: {msg}"
            );
            assert!(msg.contains("Issue #7119"), "missing issue marker: {msg}");
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_display_builtins_are_gated_issue_7121() {
        for (builtin, return_ty) in [
            (ir::AotBuiltinOp::Print, types::StaticType::Nothing),
            (ir::AotBuiltinOp::Println, types::StaticType::Nothing),
            (ir::AotBuiltinOp::StringConcat, types::StaticType::Str),
        ] {
            let mut program = ir::AotProgram::new();
            program
                .main
                .push(ir::AotStmt::Expr(ir::AotExpr::CallBuiltin {
                    builtin,
                    args: vec![ir::AotExpr::LitF64(1.0)],
                    return_ty,
                }));

            let err = lower_aot_program_for_cranelift(&program, None).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains(&format!("display builtin `{}`", builtin)),
                "missing builtin name for {builtin}: {msg}"
            );
            assert!(
                msg.contains("Julia's print/show formatting runtime"),
                "missing display runtime context: {msg}"
            );
            assert!(msg.contains("Issue #7121"), "missing issue marker: {msg}");
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_math_builtins_lower_to_libm_calls_issue_7122() {
        let builtins = [
            (ir::AotBuiltinOp::Sqrt, "sqrt"),
            (ir::AotBuiltinOp::Sin, "sin"),
            (ir::AotBuiltinOp::Cos, "cos"),
            (ir::AotBuiltinOp::Exp, "exp"),
            (ir::AotBuiltinOp::Log, "log"),
            (ir::AotBuiltinOp::Abs, "abs"),
        ];
        let mut program = ir::AotProgram::new();
        for (builtin, _) in builtins {
            program
                .main
                .push(ir::AotStmt::Expr(ir::AotExpr::CallBuiltin {
                    builtin,
                    args: vec![ir::AotExpr::LitF64(1.0)],
                    return_ty: types::StaticType::F64,
                }));
        }

        let module = lower_aot_program_for_cranelift(&program, None).unwrap();
        let main = module
            .functions
            .iter()
            .find(|func| func.name == "__juliars_main")
            .unwrap();
        let lowered_calls: Vec<&str> = main
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .filter_map(|inst| match inst {
                ir::Instruction::Call { func, .. } => Some(func.as_str()),
                _ => None,
            })
            .collect();

        for (_, expected) in builtins {
            assert!(
                lowered_calls.contains(&expected),
                "missing lowered libm call `{expected}` in {lowered_calls:?}"
            );
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_numeric_conversions_are_gated_issue_7123() {
        let mut program = ir::AotProgram::new();
        program.main.push(ir::AotStmt::Expr(ir::AotExpr::Convert {
            value: Box::new(ir::AotExpr::LitF64(1.0)),
            target_ty: types::StaticType::I64,
        }));

        let err = lower_aot_program_for_cranelift(&program, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Float64"), "missing source type: {msg}");
        assert!(msg.contains("Int64"), "missing target type: {msg}");
        assert!(msg.contains("Issue #7123"), "missing issue marker: {msg}");

        for builtin in [ir::AotBuiltinOp::Sitofp, ir::AotBuiltinOp::Fptosi] {
            let mut program = ir::AotProgram::new();
            program
                .main
                .push(ir::AotStmt::Expr(ir::AotExpr::CallBuiltin {
                    builtin,
                    args: vec![ir::AotExpr::LitI64(1)],
                    return_ty: builtin.return_type(&[types::StaticType::I64]),
                }));

            let err = lower_aot_program_for_cranelift(&program, None).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains(&format!("conversion builtin `{}`", builtin)),
                "missing builtin name for {builtin}: {msg}"
            );
            assert!(msg.contains("Issue #7123"), "missing issue marker: {msg}");
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_c_abi_export_adds_object_symbol_issue_7086() {
        use crate::aot::codegen::cranelift::CraneliftObjectCodeGenerator;
        use crate::aot::codegen::CAbiExport;

        let mut program = ir::AotProgram::new();
        let mut add = ir::AotFunction::new(
            "add".to_string(),
            vec![
                ("x".to_string(), types::StaticType::I64),
                ("y".to_string(), types::StaticType::I64),
            ],
            types::StaticType::I64,
        );
        add.body
            .push(ir::AotStmt::Return(Some(ir::AotExpr::BinOpStatic {
                op: ir::AotBinOp::Add,
                left: Box::new(ir::AotExpr::Var {
                    name: "x".to_string(),
                    ty: types::StaticType::I64,
                }),
                right: Box::new(ir::AotExpr::Var {
                    name: "y".to_string(),
                    ty: types::StaticType::I64,
                }),
                result_ty: types::StaticType::I64,
            })));
        program.add_function(add);

        let mut module = lower_aot_program_for_cranelift(&program, None).unwrap();
        append_cranelift_c_abi_export_wrappers(
            &mut module,
            &program,
            &[CAbiExport::with_arg_types(
                "sjulia_add_i64",
                "add",
                vec![types::StaticType::I64, types::StaticType::I64],
            )],
        )
        .unwrap();

        assert!(module
            .functions
            .iter()
            .any(|func| func.name == "sjulia_add_i64"));
        let object_bytes = CraneliftObjectCodeGenerator::new()
            .unwrap()
            .generate_object(&module)
            .unwrap();
        assert!(
            object_bytes
                .windows(b"sjulia_add_i64".len())
                .any(|window| window == b"sjulia_add_i64"),
            "object output should retain the requested C ABI export symbol"
        );
    }

    #[cfg(feature = "cranelift")]
    struct CraneliftLoweringFuzzer {
        state: u64,
    }

    #[cfg(feature = "cranelift")]
    impl CraneliftLoweringFuzzer {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }

        fn next(&mut self) -> u64 {
            self.state = self
                .state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            self.state
        }

        fn choice(&mut self, count: u64) -> u64 {
            self.next() % count
        }

        fn small_i64(&mut self) -> i64 {
            (self.next() % 17) as i64 - 8
        }

        fn int_leaf(&mut self) -> ir::AotExpr {
            match self.choice(4) {
                0 => ir::AotExpr::Var {
                    name: "x".to_string(),
                    ty: types::StaticType::I64,
                },
                1 => ir::AotExpr::Var {
                    name: "y".to_string(),
                    ty: types::StaticType::I64,
                },
                _ => ir::AotExpr::LitI64(self.small_i64()),
            }
        }

        fn int_expr(&mut self, depth: usize) -> ir::AotExpr {
            if depth == 0 {
                return self.int_leaf();
            }

            match self.choice(11) {
                0 => ir::AotExpr::UnaryOp {
                    op: ir::AotUnaryOp::Neg,
                    operand: Box::new(self.int_expr(depth - 1)),
                    result_ty: types::StaticType::I64,
                },
                1 => ir::AotExpr::UnaryOp {
                    op: ir::AotUnaryOp::BitNot,
                    operand: Box::new(self.int_expr(depth - 1)),
                    result_ty: types::StaticType::I64,
                },
                2 => ir::AotExpr::BinOpStatic {
                    op: ir::AotBinOp::BitAnd,
                    left: Box::new(self.int_expr(depth - 1)),
                    right: Box::new(self.int_expr(depth - 1)),
                    result_ty: types::StaticType::I64,
                },
                3 => ir::AotExpr::BinOpStatic {
                    op: ir::AotBinOp::BitOr,
                    left: Box::new(self.int_expr(depth - 1)),
                    right: Box::new(self.int_expr(depth - 1)),
                    result_ty: types::StaticType::I64,
                },
                4 => ir::AotExpr::BinOpStatic {
                    op: ir::AotBinOp::BitXor,
                    left: Box::new(self.int_expr(depth - 1)),
                    right: Box::new(self.int_expr(depth - 1)),
                    result_ty: types::StaticType::I64,
                },
                5 => ir::AotExpr::BinOpStatic {
                    op: ir::AotBinOp::Shl,
                    left: Box::new(self.int_expr(depth - 1)),
                    right: Box::new(ir::AotExpr::LitI64((self.choice(4) + 1) as i64)),
                    result_ty: types::StaticType::I64,
                },
                6 => ir::AotExpr::BinOpStatic {
                    op: ir::AotBinOp::Shr,
                    left: Box::new(self.int_expr(depth - 1)),
                    right: Box::new(ir::AotExpr::LitI64((self.choice(4) + 1) as i64)),
                    result_ty: types::StaticType::I64,
                },
                7 => ir::AotExpr::CallBuiltin {
                    builtin: ir::AotBuiltinOp::Abs,
                    args: vec![self.int_expr(depth - 1)],
                    return_ty: types::StaticType::I64,
                },
                8 => ir::AotExpr::CallStatic {
                    function: "id_i64".to_string(),
                    args: vec![self.int_expr(depth - 1)],
                    return_ty: types::StaticType::I64,
                    inline_policy: ir::AotInlinePolicy::Auto,
                },
                _ => ir::AotExpr::BinOpStatic {
                    op: match self.choice(3) {
                        0 => ir::AotBinOp::Add,
                        1 => ir::AotBinOp::Sub,
                        _ => ir::AotBinOp::Mul,
                    },
                    left: Box::new(self.int_expr(depth - 1)),
                    right: Box::new(self.int_expr(depth - 1)),
                    result_ty: types::StaticType::I64,
                },
            }
        }

        fn bool_expr(&mut self, depth: usize) -> ir::AotExpr {
            match self.choice(4) {
                0 => ir::AotExpr::LitBool(self.choice(2) == 0),
                1 => ir::AotExpr::UnaryOp {
                    op: ir::AotUnaryOp::Not,
                    operand: Box::new(ir::AotExpr::LitBool(self.choice(2) == 0)),
                    result_ty: types::StaticType::Bool,
                },
                _ => ir::AotExpr::BinOpStatic {
                    op: match self.choice(6) {
                        0 => ir::AotBinOp::Eq,
                        1 => ir::AotBinOp::Ne,
                        2 => ir::AotBinOp::Lt,
                        3 => ir::AotBinOp::Le,
                        4 => ir::AotBinOp::Gt,
                        _ => ir::AotBinOp::Ge,
                    },
                    left: Box::new(self.int_expr(depth)),
                    right: Box::new(self.int_expr(depth)),
                    result_ty: types::StaticType::Bool,
                },
            }
        }

        fn program(&mut self) -> ir::AotProgram {
            let mut program = ir::AotProgram::new();
            let mut id = ir::AotFunction::new(
                "id_i64".to_string(),
                vec![("value".to_string(), types::StaticType::I64)],
                types::StaticType::I64,
            );
            id.body.push(ir::AotStmt::Return(Some(ir::AotExpr::Var {
                name: "value".to_string(),
                ty: types::StaticType::I64,
            })));
            program.add_function(id);

            program.main.push(ir::AotStmt::Let {
                name: "x".to_string(),
                ty: types::StaticType::I64,
                value: ir::AotExpr::LitI64(self.small_i64()),
                is_mutable: false,
            });
            program.main.push(ir::AotStmt::Let {
                name: "y".to_string(),
                ty: types::StaticType::I64,
                value: ir::AotExpr::LitI64(self.small_i64()),
                is_mutable: false,
            });
            program.main.push(ir::AotStmt::Let {
                name: "acc".to_string(),
                ty: types::StaticType::I64,
                value: self.int_expr(3),
                is_mutable: true,
            });
            program.main.push(ir::AotStmt::Assign {
                target: ir::AotExpr::Var {
                    name: "acc".to_string(),
                    ty: types::StaticType::I64,
                },
                value: self.int_expr(3),
            });
            program.main.push(ir::AotStmt::Expr(self.bool_expr(2)));
            program
        }
    }

    #[cfg(feature = "cranelift")]
    #[test]
    fn cranelift_lowering_property_accepts_scalar_aot_ir_issue_7128() {
        use crate::aot::codegen::aot_codegen::AotCodeGenerator;
        use crate::aot::codegen::cranelift::CraneliftCodeGenerator;
        use crate::aot::codegen::{CodeGenerator, CodegenConfig};

        for seed in 0..64 {
            let mut generator = CraneliftLoweringFuzzer::new(seed);
            let program = generator.program();

            let rust_code = AotCodeGenerator::new(CodegenConfig::default())
                .generate_program(&program)
                .unwrap_or_else(|err| panic!("Rust backend rejected seed {seed}: {err}"));
            assert!(
                rust_code.contains("fn main"),
                "Rust backend output for seed {seed} did not contain main"
            );

            let module = lower_aot_program_for_cranelift(&program, None)
                .unwrap_or_else(|err| panic!("Cranelift lowering rejected seed {seed}: {err}"));
            assert!(
                module
                    .functions
                    .iter()
                    .any(|func| func.name == "__juliars_main"),
                "Cranelift lowering for seed {seed} did not emit __juliars_main"
            );
            CraneliftCodeGenerator::new()
                .and_then(|mut codegen| {
                    codegen.generate_module(&module).map_err(|err| {
                        crate::aot::codegen::cranelift::CraneliftError::FunctionCompilation(
                            err.to_string(),
                        )
                    })
                })
                .unwrap_or_else(|err| {
                    panic!("Cranelift verifier/codegen rejected seed {seed}: {err}")
                });
        }
    }

    #[test]
    fn test_aot_stats_new() {
        let stats = AotStats::new();
        assert_eq!(stats.functions_compiled, 0);
        assert_eq!(stats.instructions_processed, 0);
    }

    #[test]
    fn test_aot_stats_merge() {
        let mut stats1 = AotStats {
            functions_compiled: 5,
            functions_total: 10,
            functions_eliminated: 5,
            instructions_processed: 100,
            type_inferences: 20,
            dynamic_fallbacks: 3,
            optimizations_applied: 10,
        };
        let stats2 = AotStats {
            functions_compiled: 3,
            functions_total: 6,
            functions_eliminated: 3,
            instructions_processed: 50,
            type_inferences: 10,
            dynamic_fallbacks: 1,
            optimizations_applied: 5,
        };
        stats1.merge(&stats2);
        assert_eq!(stats1.functions_compiled, 8);
        assert_eq!(stats1.functions_total, 16);
        assert_eq!(stats1.functions_eliminated, 8);
        assert_eq!(stats1.instructions_processed, 150);
        assert_eq!(stats1.type_inferences, 30);
        assert_eq!(stats1.dynamic_fallbacks, 4);
        assert_eq!(stats1.optimizations_applied, 15);
    }

    #[test]
    fn test_aot_output_new() {
        let stats = AotStats::new();
        let output = AotOutput::new("fn main() {}".to_string(), stats);
        assert_eq!(output.rust_code, "fn main() {}");
        assert!(output.warnings.is_empty());
    }

    #[test]
    fn test_aot_output_add_warning() {
        let stats = AotStats::new();
        let mut output = AotOutput::new(String::new(), stats);
        output.add_warning("unused variable".to_string());
        assert_eq!(output.warnings.len(), 1);
        assert_eq!(output.warnings[0], "unused variable");
    }

    #[test]
    fn compile_from_ir_bytes_rejects_invalid_bytes() {
        let result = compile_from_ir_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn compile_from_ir_bytes_compiles_serialized_core_ir() {
        let bytes = crate::core_ir_file::save_to_bytes(&empty_program()).unwrap();
        let output = compile_from_ir_bytes(&bytes).unwrap();
        assert!(output.rust_code.contains("fn main"));
    }

    #[test]
    fn c_abi_export_keeps_uncalled_function_through_pipeline_issue_6990() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut program = empty_program();
        program.functions.push(Arc::new(Function {
            new_struct_name: None,
            name: "add".to_string(),
            params: vec![
                TypedParam::new("x".to_string(), Some(JuliaType::Int64), span),
                TypedParam::new("y".to_string(), Some(JuliaType::Int64), span),
            ],
            kwparams: vec![],
            type_params: vec![],
            return_type: Some(JuliaType::Int64),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string().into(), span)),
                        right: Box::new(Expr::Var("y".to_string().into(), span)),
                        span,
                    }),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
        }));

        let config = CompileConfig {
            c_abi_exports: vec![CAbiExport::new("sjulia_add", "add")],
            ..CompileConfig::default()
        };
        let result = compile_program(program, &config).unwrap();

        assert_eq!(result.output.stats.functions_eliminated, 0);
        assert!(result
            .output
            .rust_code
            .contains("pub fn add(x: i64, y: i64) -> i64"));
        assert!(result.output.rust_code.contains(
            "#[no_mangle]\npub extern \"C\" fn sjulia_add(x: i64, y: i64) -> i64 {\n    add(x, y)\n}"
        ));
    }

    #[test]
    fn many_function_program_compiles_with_bounded_time_issue_7002() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let function_count = 128;
        let mut program = empty_program();

        for index in 0..function_count {
            let body_expr = if index + 1 == function_count {
                Expr::BinaryOp {
                    op: BinaryOp::Add,
                    left: Box::new(Expr::Var("x".to_string().into(), span)),
                    right: Box::new(Expr::Literal(crate::ir::core::Literal::Int(1), span)),
                    span,
                }
            } else {
                Expr::Call {
                    function: format!("f{}", index + 1).into(),
                    args: vec![Expr::BinaryOp {
                        op: BinaryOp::Add,
                        left: Box::new(Expr::Var("x".to_string().into(), span)),
                        right: Box::new(Expr::Literal(crate::ir::core::Literal::Int(1), span)),
                        span,
                    }],
                    kwargs: vec![],
                    splat_mask: vec![],
                    kwargs_splat_mask: vec![],
                    span,
                }
            };
            program.functions.push(Arc::new(Function {
                new_struct_name: None,
                name: format!("f{}", index),
                params: vec![TypedParam::new(
                    "x".to_string(),
                    Some(JuliaType::Int64),
                    span,
                )],
                kwparams: vec![],
                type_params: vec![],
                return_type: Some(JuliaType::Int64),
                body: Block {
                    stmts: vec![
                        Stmt::Meta {
                            annotation: MetaAnnotation {
                                name: "noinline".to_string(),
                                args: vec![],
                            },
                            span,
                        },
                        Stmt::Return {
                            value: Some(body_expr),
                            span,
                        },
                    ],
                    span,
                },
                is_base_extension: false,
                is_runtime_eval: false,
                span,
            }));
        }

        program.main.stmts.push(Stmt::Expr {
            expr: Expr::Call {
                function: "f0".to_string().into(),
                args: vec![Expr::Literal(crate::ir::core::Literal::Int(0), span)],
                kwargs: vec![],
                splat_mask: vec![],
                kwargs_splat_mask: vec![],
                span,
            },
            span,
        });

        let started = AotTimer::start();
        let result = compile_program(program, &CompileConfig::default()).unwrap();
        let elapsed = started.elapsed();

        assert_eq!(result.output.stats.functions_compiled, function_count);
        assert_eq!(result.output.stats.functions_eliminated, 0);
        assert!(result.output.rust_code.contains("pub fn f0(x: i64) -> i64"));
        assert!(result
            .output
            .rust_code
            .contains("pub fn f127(x: i64) -> i64"));
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "many-function AoT compile took {:?}",
            elapsed
        );
    }

    #[test]
    fn abstract_return_value_boundary_codegen_is_valid_issue_7074() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut program = empty_program();
        program.functions.push(Arc::new(Function {
            new_struct_name: None,
            name: "f".to_string(),
            params: vec![TypedParam::new(
                "x".to_string(),
                Some(JuliaType::Int64),
                span,
            )],
            kwparams: vec![],
            type_params: vec![],
            return_type: Some(JuliaType::Real),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "convert".to_string().into(),
                        args: vec![
                            Expr::Var("Real".to_string().into(), span),
                            Expr::Var("x".to_string().into(), span),
                        ],
                        kwargs: vec![],
                        splat_mask: vec![false, false],
                        kwargs_splat_mask: vec![],
                        span,
                    }),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
        }));
        program.functions.push(Arc::new(Function {
            new_struct_name: None,
            name: "g".to_string(),
            params: vec![TypedParam::new(
                "flag".to_string(),
                Some(JuliaType::Bool),
                span,
            )],
            kwparams: vec![],
            type_params: vec![],
            return_type: Some(JuliaType::Real),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Call {
                        function: "convert".to_string().into(),
                        args: vec![
                            Expr::Var("Real".to_string().into(), span),
                            Expr::Ternary {
                                condition: Box::new(Expr::Var("flag".to_string().into(), span)),
                                then_expr: Box::new(Expr::Literal(Literal::Int(1), span)),
                                else_expr: Box::new(Expr::Literal(Literal::Float(2.5), span)),
                                span,
                            },
                        ],
                        kwargs: vec![],
                        splat_mask: vec![false, false],
                        kwargs_splat_mask: vec![],
                        span,
                    }),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
        }));

        program.main.stmts.push(Stmt::Expr {
            expr: Expr::LetBlock {
                bindings: vec![(
                    "y".to_string().into(),
                    Expr::Call {
                        function: "f".to_string().into(),
                        args: vec![Expr::Literal(Literal::Int(1), span)],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span,
                    },
                )],
                body: Block {
                    stmts: vec![Stmt::Expr {
                        expr: Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("y".to_string().into(), span)),
                            right: Box::new(Expr::Literal(Literal::Int(2), span)),
                            span,
                        },
                        span,
                    }],
                    span,
                },
                span,
            },
            span,
        });
        program.main.stmts.push(Stmt::Expr {
            expr: Expr::LetBlock {
                bindings: vec![(
                    "z".to_string().into(),
                    Expr::Call {
                        function: "g".to_string().into(),
                        args: vec![Expr::Literal(Literal::Bool(false), span)],
                        kwargs: vec![],
                        splat_mask: vec![false],
                        kwargs_splat_mask: vec![],
                        span,
                    },
                )],
                body: Block {
                    stmts: vec![Stmt::Expr {
                        expr: Expr::BinaryOp {
                            op: BinaryOp::Add,
                            left: Box::new(Expr::Var("z".to_string().into(), span)),
                            right: Box::new(Expr::Literal(Literal::Int(2), span)),
                            span,
                        },
                        span,
                    }],
                    span,
                },
                span,
            },
            span,
        });

        let config = CompileConfig {
            opt_level: optimizer::OptLevel::O0,
            ..CompileConfig::default()
        };
        let result = compile_program(program, &config).unwrap();

        assert!(result
            .output
            .rust_code
            .contains("pub fn f(x: i64) -> Value"));
        assert!(result.output.rust_code.contains("return Value::from(x);"));
        assert!(result
            .output
            .rust_code
            .contains("subset_julia_vm_runtime::dynamic_binop"));
        assert!(!result.output.rust_code.contains("dynamic_call(\"convert\""));
        assert!(!result.output.rust_code.contains("Value::from(if"));
        assert!(!result.output.rust_code.contains("Real,"));
        assert!(!result.output.rust_code.contains("(y + 2i64)"));
        assert!(!result.output.rust_code.contains("(z + 2i64)"));
    }

    #[test]
    fn type_unstable_local_reassignment_uses_value_slot_issue_7075() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut program = empty_program();
        program.main.stmts.push(Stmt::Expr {
            expr: Expr::LetBlock {
                bindings: vec![("x".to_string().into(), Expr::Literal(Literal::Int(1), span))],
                body: Block {
                    stmts: vec![
                        Stmt::Assign {
                            var: "x".to_string(),
                            value: Expr::Literal(Literal::Str("s".to_string()), span),
                            span,
                        },
                        Stmt::Expr {
                            expr: Expr::Var("x".to_string().into(), span),
                            span,
                        },
                    ],
                    span,
                },
                span,
            },
            span,
        });

        let config = CompileConfig {
            opt_level: optimizer::OptLevel::O0,
            ..CompileConfig::default()
        };
        let result = compile_program(program, &config).unwrap();

        assert!(result
            .output
            .rust_code
            .contains("let mut x: Value = Value::from(1i64);"));
        assert!(result
            .output
            .rust_code
            .contains("x = Value::from(\"s\".to_string());"));
        assert!(!result.output.rust_code.contains("let mut x: i64 = 1i64;"));
    }

    /// Issue #10537 (codex review of #10528): when the scope-local join of a
    /// `let` binding is `StaticType::Any` — not `Union` — because a later
    /// assignment is a call whose return type is `Any`, `enter_lexical_scope`
    /// must keep the boxed `Value` slot. Dropping the `Any` env entry lets the
    /// first concrete assignment declare `i64` and the later Any store fails
    /// codegen (#6978). Distinct from #7075, which covers the Int+Str → Union path.
    #[test]
    fn type_unstable_any_return_reassignment_uses_value_slot_issue_10537() {
        let span = Span::new(0, 0, 1, 1, 1, 1);
        let mut program = empty_program();
        program.functions.push(Arc::new(Function {
            new_struct_name: None,
            name: "g".to_string(),
            params: vec![TypedParam::new("x".to_string(), Some(JuliaType::Any), span)],
            kwparams: vec![],
            type_params: vec![],
            return_type: Some(JuliaType::Any),
            body: Block {
                stmts: vec![Stmt::Return {
                    value: Some(Expr::Var("x".to_string().into(), span)),
                    span,
                }],
                span,
            },
            is_base_extension: false,
            is_runtime_eval: false,
            span,
        }));
        program.main.stmts.push(Stmt::Expr {
            expr: Expr::LetBlock {
                bindings: vec![("x".to_string().into(), Expr::Literal(Literal::Int(1), span))],
                body: Block {
                    stmts: vec![
                        Stmt::Assign {
                            var: "x".to_string(),
                            value: Expr::Call {
                                function: "g".to_string().into(),
                                args: vec![Expr::Literal(Literal::Str("s".to_string()), span)],
                                kwargs: vec![],
                                splat_mask: vec![false],
                                kwargs_splat_mask: vec![],
                                span,
                            },
                            span,
                        },
                        Stmt::Expr {
                            expr: Expr::Var("x".to_string().into(), span),
                            span,
                        },
                    ],
                    span,
                },
                span,
            },
            span,
        });

        let config = CompileConfig {
            opt_level: optimizer::OptLevel::O0,
            ..CompileConfig::default()
        };
        let result = compile_program(program, &config).expect(
            "let local reassigned through an Any-returning call must compile with a Value slot (#10537)",
        );

        assert!(
            result
                .output
                .rust_code
                .contains("let mut x: Value = Value::from(1i64);"),
            "scope-local Any join must keep a boxed Value slot, got:\n{}",
            result.output.rust_code
        );
        assert!(
            !result.output.rust_code.contains("let mut x: i64 = 1i64;"),
            "must not declare a concrete i64 slot when a later Any store is in scope, got:\n{}",
            result.output.rust_code
        );
    }

    #[test]
    fn pure_rust_runtime_references_lists_residual_runtime_lines() {
        let refs = pure_rust_runtime_references(
            r#"
extern crate subset_julia_vm_runtime;
use subset_julia_vm_runtime::Value;
fn main() {
    let _ = subset_julia_vm_runtime::RuntimeResult::Ok(());
}
"#,
        );
        assert_eq!(
            refs,
            vec![
                "extern crate subset_julia_vm_runtime;".to_string(),
                "use subset_julia_vm_runtime::Value;".to_string(),
                "let _ = subset_julia_vm_runtime::RuntimeResult::Ok(());".to_string(),
            ]
        );
    }
}
