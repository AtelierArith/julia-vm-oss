#![deny(clippy::expect_used)]
//! `juliars` — the SubsetJuliaVM AoT (Ahead-of-Time) Compiler CLI.
//!
//! This binary compiles a strict subset of Julia to Rust source code.
//!
//! Usage:
//!   juliars input.jl -o output.rs
//!   juliars input.jl --stats
//!   juliars -e "1 + 2" -o output.rs
//!   juliars --ir input.sjir -o output.rs
//!   cat input.jl | juliars - -o -        # stdin → stdout

use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use subset_julia_vm::aot::call_graph::CallGraph;
use subset_julia_vm::aot::codegen::CAbiExport;
use subset_julia_vm::aot::linker::{link_objects, LinkOutputKind, LinkerConfig};
use subset_julia_vm::aot::optimizer::OptLevel;
use subset_julia_vm::aot::types::StaticType;
use subset_julia_vm::aot::{compile_cranelift_object_for_target, run_cranelift_jit_main};
use subset_julia_vm::aot::{compile_program, AotBackend, AotError, AotOutput, CompileConfig};
use subset_julia_vm::base;
use subset_julia_vm::core_ir_file;
use subset_julia_vm::error::UnsupportedFeature;
use subset_julia_vm::ir::core::{Block, DefinitionOrderCursor, Expr, Program, Stmt};
use subset_julia_vm::loader;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::{ParseOutcome, Parser, RustParsedSource};
use subset_julia_vm::span::Span;

#[cfg(feature = "aot-wasm")]
#[path = "aot_wasm.rs"]
mod aot_wasm;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Process exit codes, classified by failure category (Issue #6997) so that
/// scripts and CI can distinguish *why* compilation failed.
mod exit_code {
    pub const SUCCESS: i32 = 0;
    /// Internal compiler error (a bug in juliars).
    pub const INTERNAL: i32 = 1;
    /// Bad command-line usage (conflicting/missing arguments).
    pub const USAGE: i32 = 2;
    /// I/O error reading input or writing output.
    pub const IO: i32 = 3;
    /// Parse or lowering error in the Julia source.
    pub const PARSE: i32 = 4;
    /// A required feature is unsupported by the AoT backend.
    pub const UNSUPPORTED: i32 = 5;
    /// Code generation failed (e.g. pure-rust requested but not achievable).
    pub const CODEGEN: i32 = 6;
}

/// Map an [`AotError`] to its classified exit code.
fn exit_code_for(err: &AotError) -> i32 {
    match err {
        AotError::ParseError { .. } | AotError::LoweringError { .. } => exit_code::PARSE,
        AotError::UnsupportedInstruction(_) => exit_code::UNSUPPORTED,
        AotError::CodegenError(_) => exit_code::CODEGEN,
        AotError::TypeInferenceError(_)
        | AotError::OptimizationError(_)
        | AotError::InvalidIR(_)
        | AotError::ConversionError(_)
        | AotError::InternalError(_) => exit_code::INTERNAL,
    }
}

fn get_method_signature(func: &subset_julia_vm::ir::core::Function) -> String {
    let param_types: Vec<String> = func
        .params
        .iter()
        .map(|p| p.effective_type().to_string())
        .collect();
    format!("{}({})", func.name, param_types.join(", "))
}

/// Code-generation backend selection (Issue #6927).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Rust,
    Cranelift,
    #[cfg(feature = "aot-wasm")]
    Wasm,
}

impl Backend {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "rust" => Ok(Self::Rust),
            "cranelift" => Ok(Self::Cranelift),
            #[cfg(feature = "aot-wasm")]
            "wasm" => Ok(Self::Wasm),
            other => Err(format!(
                "--backend requires {} (got {})",
                backend_values(),
                other
            )),
        }
    }
}

const fn backend_values() -> &'static str {
    if cfg!(feature = "aot-wasm") {
        "rust|cranelift|wasm"
    } else {
        "rust|cranelift"
    }
}

impl From<Backend> for AotBackend {
    fn from(value: Backend) -> Self {
        match value {
            Backend::Rust => AotBackend::Rust,
            Backend::Cranelift => AotBackend::Cranelift,
            #[cfg(feature = "aot-wasm")]
            Backend::Wasm => AotBackend::Wasm,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryKind {
    Static,
    Shared,
}

impl LibraryKind {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "static" => Ok(Self::Static),
            "shared" => Ok(Self::Shared),
            other => Err(format!(
                "--library-kind requires static|shared (got {})",
                other
            )),
        }
    }
}

/// Diagnostic output format (Issue #6996).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticFormat {
    Human,
    Json,
}

impl DiagnosticFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "--diagnostic-format requires human|json (got {})",
                other
            )),
        }
    }
}

/// CLI color policy for human diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            other => Err(format!(
                "--color requires auto|always|never (got {})",
                other
            )),
        }
    }

    fn enabled(self) -> bool {
        match self {
            Self::Auto => std::io::stderr().is_terminal(),
            Self::Always => true,
            Self::Never => false,
        }
    }
}

fn split_c_abi_export_specs(value: &str) -> Result<Vec<&str>, String> {
    let mut specs = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    "--export-c-abi has an unmatched `)` in its signature".to_string()
                })?;
            }
            ',' if depth == 0 => {
                let spec = value[start..idx].trim();
                if spec.is_empty() {
                    return Err("--export-c-abi contains an empty export spec".to_string());
                }
                specs.push(spec);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("--export-c-abi has an unmatched `(` in its signature".to_string());
    }
    let spec = value[start..].trim();
    if spec.is_empty() {
        return Err("--export-c-abi contains an empty export spec".to_string());
    }
    specs.push(spec);
    Ok(specs)
}

fn parse_c_abi_type_name(value: &str) -> Result<StaticType, String> {
    let name = value.trim();
    let parsed = StaticType::from_julia_name_lossy(name);
    match parsed {
        Some(
            ty @ (StaticType::I64
            | StaticType::I32
            | StaticType::I16
            | StaticType::I8
            | StaticType::U64
            | StaticType::U32
            | StaticType::U16
            | StaticType::U8
            | StaticType::F64
            | StaticType::F32
            | StaticType::Bool
            | StaticType::Nothing
            | StaticType::Array { .. }),
        ) => Ok(ty),
        Some(_) | None => Err(format!(
            "--export-c-abi signature type `{name}` is not supported; use a C-stable scalar or typed array"
        )),
    }
}

fn parse_c_abi_function_spec(value: &str) -> Result<(String, Option<Vec<StaticType>>), String> {
    let Some(open_idx) = value.find('(') else {
        if value.contains(')') {
            return Err("--export-c-abi has an unmatched `)` in its signature".to_string());
        }
        return Ok((value.to_string(), None));
    };
    if !value.ends_with(')') {
        return Err("--export-c-abi signature must end with `)`".to_string());
    }
    let function_name = value[..open_idx].trim();
    if function_name.is_empty() {
        return Err("--export-c-abi requires a function name before `(`".to_string());
    }
    let type_list = &value[open_idx + 1..value.len() - 1];
    let arg_types = if type_list.trim().is_empty() {
        Vec::new()
    } else {
        type_list
            .split(',')
            .map(parse_c_abi_type_name)
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok((function_name.to_string(), Some(arg_types)))
}

fn parse_c_abi_export(value: &str) -> Result<Vec<CAbiExport>, String> {
    if value.is_empty() {
        return Err(
            "--export-c-abi requires symbol, symbol=function, or symbol=function(Int64,...)"
                .to_string(),
        );
    }
    split_c_abi_export_specs(value)?
        .into_iter()
        .map(parse_c_abi_export_spec)
        .collect()
}

fn parse_c_abi_export_spec(value: &str) -> Result<CAbiExport, String> {
    if let Some((export_name, function_spec)) = value.split_once('=') {
        let export_name = export_name.trim();
        let function_spec = function_spec.trim();
        if export_name.is_empty() || function_spec.is_empty() {
            return Err("--export-c-abi requires non-empty symbol and function names".to_string());
        }
        let (function_name, arg_types) = parse_c_abi_function_spec(function_spec)?;
        if let Some(arg_types) = arg_types {
            Ok(CAbiExport::with_arg_types(
                export_name,
                function_name,
                arg_types,
            ))
        } else {
            Ok(CAbiExport::new(export_name, function_name))
        }
    } else {
        let (function_name, arg_types) = parse_c_abi_function_spec(value.trim())?;
        if arg_types.is_some() {
            return Err("--export-c-abi overload signatures require an explicit export symbol, e.g. symbol=function(Int64)".to_string());
        }
        Ok(CAbiExport::new(&function_name, function_name.clone()))
    }
}

/// Command-line arguments
#[derive(Debug)]
struct Args {
    /// Input file path (None if using -e); `-` means stdin.
    input_file: Option<String>,
    /// Code string (for -e option)
    code: Option<String>,
    /// Core IR file path (for --ir option)
    ir_file: Option<String>,
    /// Output file path; `-` means stdout.
    output_file: Option<String>,
    /// Build generated Rust into a binary at this path.
    emit_binary: Option<String>,
    /// Emit a relocatable Cranelift object file at this path.
    emit_object: Option<String>,
    /// Emit a Cranelift static/shared library at this path.
    emit_library: Option<String>,
    #[cfg(feature = "aot-wasm")]
    emit_wasm: Option<String>,
    /// Library output kind for --emit-library.
    library_kind: LibraryKind,
    /// Whether --library-kind was explicitly specified.
    library_kind_specified: bool,
    /// Optional target triple for native artifact emission.
    target: Option<String>,
    /// C ABI exports requested as `symbol` or `symbol=function`.
    c_abi_exports: Vec<CAbiExport>,
    /// Show statistics
    show_stats: bool,
    /// Dump a named AoT stage (`all` dumps every supported AoT IR stage)
    dump_aot_stage: Option<String>,
    /// Emit debug comments in generated code
    emit_comments: bool,
    /// Emit native debug information where supported.
    debug_info: bool,
    /// Show help
    show_help: bool,
    /// Show version
    show_version: bool,
    /// Generate pure Rust code without Value type dependency.
    pure_rust: bool,
    /// Use minimal prelude for AoT compilation (fully-typed functions only)
    minimal_prelude: bool,
    /// Optimization level (`-O0`..`-O3`).
    opt_level: OptLevel,
    /// Print per-pass timing.
    time_passes: bool,
    /// Dry-run: report unsupported features without writing output.
    check: bool,
    /// Explicitly compile and run the Cranelift JIT entry point.
    jit_run: bool,
    /// Code-generation backend.
    backend: Backend,
    /// Diagnostic output format.
    diagnostic_format: DiagnosticFormat,
    /// Human diagnostic color policy.
    color: ColorChoice,
}

impl Args {
    fn parse_from<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let mut parsed = Args {
            input_file: None,
            code: None,
            ir_file: None,
            output_file: None,
            emit_binary: None,
            emit_object: None,
            emit_library: None,
            #[cfg(feature = "aot-wasm")]
            emit_wasm: None,
            library_kind: LibraryKind::Static,
            library_kind_specified: false,
            target: None,
            c_abi_exports: Vec::new(),
            show_stats: false,
            dump_aot_stage: None,
            emit_comments: false,
            debug_info: false,
            show_help: false,
            show_version: false,
            pure_rust: false,
            minimal_prelude: false,
            opt_level: OptLevel::default(),
            time_passes: false,
            check: false,
            jit_run: false,
            backend: Backend::Rust,
            diagnostic_format: DiagnosticFormat::Human,
            color: ColorChoice::Auto,
        };

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "-h" | "--help" => parsed.show_help = true,
                "-v" | "--version" => parsed.show_version = true,
                "-o" | "--output" => {
                    i += 1;
                    if i < args.len() {
                        parsed.output_file = Some(args[i].clone());
                    } else {
                        return Err("-o/--output requires a path (use - for stdout)".to_string());
                    }
                }
                "-e" | "--eval" => {
                    i += 1;
                    if i < args.len() {
                        parsed.code = Some(args[i].clone());
                    } else {
                        return Err("-e/--eval requires a code string".to_string());
                    }
                }
                "-i" | "--ir" => {
                    i += 1;
                    if i < args.len() {
                        parsed.ir_file = Some(args[i].clone());
                    } else {
                        return Err("-i/--ir requires a .sjir path".to_string());
                    }
                }
                "--emit-binary" => {
                    i += 1;
                    if i < args.len() {
                        parsed.emit_binary = Some(args[i].clone());
                    } else {
                        return Err("--emit-binary requires an output binary path".to_string());
                    }
                }
                arg if arg.starts_with("--emit-binary=") => {
                    let value = arg.trim_start_matches("--emit-binary=");
                    if value.is_empty() {
                        return Err("--emit-binary requires an output binary path".to_string());
                    }
                    parsed.emit_binary = Some(value.to_string());
                }
                "--emit-object" => {
                    i += 1;
                    if i < args.len() {
                        parsed.emit_object = Some(args[i].clone());
                    } else {
                        return Err("--emit-object requires an output object path".to_string());
                    }
                }
                arg if arg.starts_with("--emit-object=") => {
                    let value = arg.trim_start_matches("--emit-object=");
                    if value.is_empty() {
                        return Err("--emit-object requires an output object path".to_string());
                    }
                    parsed.emit_object = Some(value.to_string());
                }
                "--emit-library" => {
                    i += 1;
                    if i < args.len() {
                        parsed.emit_library = Some(args[i].clone());
                    } else {
                        return Err("--emit-library requires an output library path".to_string());
                    }
                }
                arg if arg.starts_with("--emit-library=") => {
                    let value = arg.trim_start_matches("--emit-library=");
                    if value.is_empty() {
                        return Err("--emit-library requires an output library path".to_string());
                    }
                    parsed.emit_library = Some(value.to_string());
                }
                #[cfg(feature = "aot-wasm")]
                "--emit-wasm" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) if !value.is_empty() => parsed.emit_wasm = Some(value.clone()),
                        _ => return Err("--emit-wasm requires an output Wasm path".to_string()),
                    }
                }
                #[cfg(feature = "aot-wasm")]
                arg if arg.starts_with("--emit-wasm=") => {
                    let value = arg.trim_start_matches("--emit-wasm=");
                    if value.is_empty() {
                        return Err("--emit-wasm requires an output Wasm path".to_string());
                    }
                    parsed.emit_wasm = Some(value.to_string());
                }
                "--library-kind" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) => {
                            parsed.library_kind = LibraryKind::parse(value)?;
                            parsed.library_kind_specified = true;
                        }
                        None => return Err("--library-kind requires static|shared".to_string()),
                    }
                }
                arg if arg.starts_with("--library-kind=") => {
                    parsed.library_kind =
                        LibraryKind::parse(arg.trim_start_matches("--library-kind="))?;
                    parsed.library_kind_specified = true;
                }
                "--target" => {
                    i += 1;
                    if i < args.len() {
                        parsed.target = Some(args[i].clone());
                    } else {
                        return Err("--target requires a Rust target triple".to_string());
                    }
                }
                arg if arg.starts_with("--target=") => {
                    let value = arg.trim_start_matches("--target=");
                    if value.is_empty() {
                        return Err("--target requires a Rust target triple".to_string());
                    }
                    parsed.target = Some(value.to_string());
                }
                "--export-c-abi" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) => parsed.c_abi_exports.extend(parse_c_abi_export(value)?),
                        None => {
                            return Err(
                                "--export-c-abi requires symbol, symbol=function, or symbol=function(Int64,...)"
                                    .to_string(),
                            )
                        }
                    }
                }
                arg if arg.starts_with("--export-c-abi=") => {
                    parsed.c_abi_exports.extend(parse_c_abi_export(
                        arg.trim_start_matches("--export-c-abi="),
                    )?);
                }
                "--stats" => parsed.show_stats = true,
                "--check" => parsed.check = true,
                "--jit-run" => parsed.jit_run = true,
                "--time-passes" => parsed.time_passes = true,
                "--dump-aot-stage" => {
                    i += 1;
                    if i < args.len() {
                        parsed.dump_aot_stage = Some(args[i].clone());
                    } else {
                        return Err("--dump-aot-stage requires a stage name or `all`".to_string());
                    }
                }
                arg if arg.starts_with("--dump-aot-stage=") => {
                    let value = arg.trim_start_matches("--dump-aot-stage=");
                    if value.is_empty() {
                        return Err("--dump-aot-stage requires a stage name or `all`".to_string());
                    }
                    parsed.dump_aot_stage = Some(value.to_string());
                }
                "--backend" => {
                    i += 1;
                    parsed.backend =
                        Backend::parse(args.get(i).map(String::as_str).unwrap_or("<missing>"))?;
                }
                arg if arg.starts_with("--backend=") => {
                    parsed.backend = Backend::parse(arg.trim_start_matches("--backend="))?;
                }
                "--diagnostic-format" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) => {
                            parsed.diagnostic_format = DiagnosticFormat::parse(value)?;
                        }
                        None => return Err("--diagnostic-format requires human|json".to_string()),
                    }
                }
                arg if arg.starts_with("--diagnostic-format=") => {
                    parsed.diagnostic_format =
                        DiagnosticFormat::parse(arg.trim_start_matches("--diagnostic-format="))?;
                }
                "--color" => {
                    i += 1;
                    match args.get(i) {
                        Some(value) => {
                            parsed.color = ColorChoice::parse(value)?;
                        }
                        None => return Err("--color requires auto|always|never".to_string()),
                    }
                }
                arg if arg.starts_with("--color=") => {
                    parsed.color = ColorChoice::parse(arg.trim_start_matches("--color="))?;
                }
                "--opt-level" => {
                    i += 1;
                    match args.get(i) {
                        Some(v) => parsed.opt_level = OptLevel::parse(v)?,
                        None => return Err("--opt-level requires a level 0-3".to_string()),
                    }
                }
                arg if arg.starts_with("--opt-level=") => {
                    parsed.opt_level = OptLevel::parse(arg.trim_start_matches("--opt-level="))?;
                }
                arg if arg.starts_with("-O") => {
                    parsed.opt_level = OptLevel::parse(arg.trim_start_matches("-O"))?;
                }
                "--comments" => parsed.emit_comments = true,
                "--debug-info" => parsed.debug_info = true,
                "--pure-rust" => parsed.pure_rust = true,
                "--minimal-prelude" => parsed.minimal_prelude = true,
                "-" => {
                    // bare `-` as a positional means "read source from stdin"
                    if parsed.input_file.is_none()
                        && parsed.code.is_none()
                        && parsed.ir_file.is_none()
                    {
                        parsed.input_file = Some("-".to_string());
                    } else {
                        return Err("multiple inputs specified".to_string());
                    }
                }
                arg if !arg.starts_with('-') => {
                    if parsed.input_file.is_some() {
                        return Err(format!(
                            "multiple input files specified ('{}' and '{}')",
                            parsed.input_file.as_deref().unwrap_or(""),
                            arg
                        ));
                    }
                    parsed.input_file = Some(arg.to_string());
                }
                _ => {
                    return Err(format!("Unknown option: {}", args[i]));
                }
            }
            i += 1;
        }

        // Validate mutual exclusion of input sources (Issue #6929).
        let mut sources = Vec::new();
        if parsed.input_file.is_some() {
            sources.push("<input file>");
        }
        if parsed.code.is_some() {
            sources.push("-e/--eval");
        }
        if parsed.ir_file.is_some() {
            sources.push("--ir");
        }
        if sources.len() > 1 {
            return Err(format!(
                "conflicting inputs: {} are mutually exclusive; specify exactly one",
                sources.join(", ")
            ));
        }
        if parsed.target.is_some()
            && parsed.emit_binary.is_none()
            && parsed.emit_object.is_none()
            && parsed.emit_library.is_none()
        {
            return Err(
                "--target requires --emit-binary, --emit-object, or --emit-library".to_string(),
            );
        }
        #[cfg(feature = "aot-wasm")]
        if parsed.backend == Backend::Wasm {
            if parsed.emit_wasm.is_none() {
                return Err("--backend wasm requires --emit-wasm PATH".to_string());
            }
            if parsed.ir_file.is_some() {
                return Err("--backend wasm does not accept --ir; provide Julia source".to_string());
            }
            if parsed.output_file.is_some()
                || parsed.emit_binary.is_some()
                || parsed.emit_object.is_some()
                || parsed.emit_library.is_some()
            {
                return Err(
                    "--emit-wasm cannot be combined with Rust or native artifact outputs"
                        .to_string(),
                );
            }
            if parsed.check || parsed.jit_run || parsed.target.is_some() || parsed.minimal_prelude {
                return Err(
                    "--backend wasm cannot be combined with --check, --jit-run, --target, or --minimal-prelude"
                        .to_string(),
                );
            }
        }
        #[cfg(feature = "aot-wasm")]
        if parsed.emit_wasm.is_some() && parsed.backend != Backend::Wasm {
            return Err("--emit-wasm requires --backend wasm".to_string());
        }
        if parsed.library_kind_specified && parsed.emit_library.is_none() {
            return Err("--library-kind requires --emit-library".to_string());
        }
        if parsed.emit_object.is_some() && parsed.backend != Backend::Cranelift {
            return Err("--emit-object requires --backend cranelift".to_string());
        }
        if parsed.emit_library.is_some() && parsed.backend != Backend::Cranelift {
            return Err("--emit-library requires --backend cranelift".to_string());
        }
        if parsed.emit_object.is_some() && parsed.emit_binary.is_some() {
            return Err("--emit-object cannot be combined with --emit-binary".to_string());
        }
        if parsed.emit_library.is_some()
            && (parsed.emit_binary.is_some() || parsed.emit_object.is_some())
        {
            return Err(
                "--emit-library cannot be combined with --emit-binary or --emit-object".to_string(),
            );
        }
        if parsed.backend == Backend::Cranelift
            && parsed.emit_binary.is_some()
            && parsed.output_file.is_some()
        {
            return Err(
                "--emit-binary with --backend cranelift cannot be combined with -o/--output"
                    .to_string(),
            );
        }
        if parsed.backend == Backend::Cranelift && parsed.emit_binary.is_some() && parsed.check {
            return Err(
                "--emit-binary with --backend cranelift cannot be combined with --check"
                    .to_string(),
            );
        }
        if parsed.emit_object.is_some() && parsed.check {
            return Err("--emit-object cannot be combined with --check".to_string());
        }
        if parsed.emit_library.is_some() && parsed.check {
            return Err("--emit-library cannot be combined with --check".to_string());
        }
        if parsed.emit_object.is_some() && parsed.output_file.is_some() {
            return Err("--emit-object cannot be combined with -o/--output".to_string());
        }
        if parsed.emit_library.is_some() && parsed.output_file.is_some() {
            return Err("--emit-library cannot be combined with -o/--output".to_string());
        }
        if parsed.debug_info && parsed.backend != Backend::Cranelift {
            return Err("--debug-info requires --backend cranelift".to_string());
        }
        if parsed.debug_info
            && parsed.emit_binary.is_none()
            && parsed.emit_object.is_none()
            && parsed.emit_library.is_none()
        {
            return Err(
                "--debug-info requires Cranelift native artifact output (--emit-object, --emit-binary, or --emit-library)"
                    .to_string(),
            );
        }
        if parsed.jit_run && parsed.backend != Backend::Cranelift {
            return Err("--jit-run requires --backend cranelift".to_string());
        }
        if parsed.jit_run && parsed.emit_binary.is_some() {
            return Err("--jit-run cannot be combined with --emit-binary".to_string());
        }
        if parsed.jit_run && parsed.emit_object.is_some() {
            return Err("--jit-run cannot be combined with --emit-object".to_string());
        }
        if parsed.jit_run && parsed.emit_library.is_some() {
            return Err("--jit-run cannot be combined with --emit-library".to_string());
        }
        if parsed.jit_run && parsed.check {
            return Err("--jit-run cannot be combined with --check".to_string());
        }
        if parsed.jit_run && parsed.output_file.is_some() {
            return Err("--jit-run cannot be combined with -o/--output".to_string());
        }

        Ok(parsed)
    }
}

fn print_help() {
    println!(
        r#"juliars — SubsetJuliaVM AoT Compiler v{}

USAGE:
    juliars [OPTIONS] <input.jl>
    juliars -e <code> [OPTIONS]
    juliars --ir <program.sjir> [OPTIONS]
    cat input.jl | juliars - -o -          # stdin → stdout

OPTIONS:
    -h, --help        Show this help message
    -v, --version     Show version information
    -o, --output      Output file path (`-` for stdout; default: <input>.rs)
    -e, --eval        Compile code string instead of file
    -i, --ir          Compile from Core IR file (.sjir) instead of source
    -O<N>             Optimization level: -O0 (none) .. -O3 (default -O2)
        --opt-level N Same as -O<N>
        --stats       Show compilation statistics
        --check       Dry-run: report unsupported features, write nothing
        --jit-run     With --backend cranelift, compile and run the in-process JIT entry point
        --time-passes Print wall-clock time spent in each pipeline stage
        --emit-binary PATH
                      Build a native binary at PATH; Rust backend uses a
                      temporary Cargo project, Cranelift links object output
        --emit-object PATH
                      With --backend cranelift, emit a relocatable object file at PATH
        --emit-library PATH
                      With --backend cranelift, emit a static/shared library at PATH
        --library-kind K
                      Library kind for --emit-library: static (default) | shared
        --target TRIPLE
                      Target triple for native artifact emission
        --export-c-abi SPEC
                      Export C ABI entry: symbol, symbol=function,
                      symbol=function(Int64,...), or comma-separated specs
        --diagnostic-format F
                      Diagnostic format: human (default) | json
        --color WHEN   Human diagnostic colors: auto (default) | always | never
        --backend B   Code generation backend: {}
{}        --dump-aot-stage S  Dump AoT IR at a named stage or `all`
        --comments    Emit debug comments in generated code
        --debug-info  With --backend cranelift native artifact output, emit DWARF debug info
        --pure-rust   Generate standalone Rust (fails if dynamic dispatch needed)
        --minimal-prelude  Use the minimal typed prelude for pure-Rust codegen

EXIT CODES:
    0 success   2 usage error   3 I/O error
    4 parse/lowering error   5 unsupported feature   6 codegen error

EXAMPLES:
    juliars input.jl -o output.rs
    juliars input.jl --stats
    juliars input.jl --pure-rust -o output.rs    # Standalone Rust (no runtime)
    juliars input.jl --emit-binary ./program     # Rust source stays temporary
    juliars input.jl --backend cranelift --emit-binary ./program
    juliars input.jl --backend cranelift --emit-object ./program.o
    juliars input.jl --backend cranelift --emit-library ./libprogram.a --library-kind static
{}    juliars input.jl --emit-binary ./program --target aarch64-apple-ios-sim
    juliars input.jl --export-c-abi add_i64=add(Int64,Int64)
    juliars input.jl -o output.rs --emit-binary ./program
    juliars -e "function add(x, y) x + y end" -o add.rs
    juliars --ir program.sjir -o output.rs

GENERATED CODE:
    scripts/juliars_build_generated.sh output.rs ./program && ./program

    Output depending on the runtime links against subset_julia_vm_runtime;
    --pure-rust output compiles standalone.
"#,
        VERSION,
        backend_values(),
        wasm_help(),
        wasm_example(),
    );
}

const fn wasm_help() -> &'static str {
    if cfg!(feature = "aot-wasm") {
        "        --emit-wasm PATH\n                      With --backend wasm, atomically emit a standalone Wasm module at PATH\n"
    } else {
        ""
    }
}

const fn wasm_example() -> &'static str {
    if cfg!(feature = "aot-wasm") {
        "    juliars input.jl --backend wasm --emit-wasm ./program.wasm\n"
    } else {
        ""
    }
}

fn print_version() {
    println!("juliars — SubsetJuliaVM AoT Compiler v{}", VERSION);
}

/// Read source from a file path, or stdin if the path is `-`.
fn read_source(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("Error reading stdin: {}", e))?;
        Ok(buf)
    } else {
        if !Path::new(path).exists() {
            return Err(format!("File '{}' not found", path));
        }
        fs::read_to_string(path).map_err(|e| format!("Error reading file '{}': {}", path, e))
    }
}

fn source_context_for_span(source: &str, span: Span) -> String {
    let line_idx = span.start_line.saturating_sub(1);
    let Some(line) = source.lines().nth(line_idx) else {
        return String::new();
    };

    let col = span.start_column.saturating_sub(1);
    let len = if span.start_line == span.end_line {
        span.end_column.saturating_sub(span.start_column).max(1)
    } else {
        1
    };
    let marker_len = len.min(line.len().saturating_sub(col)).max(1);

    format!(
        "  {} | {}\n  {} | {}{}",
        span.start_line,
        line,
        " ".repeat(span.start_line.to_string().len()),
        " ".repeat(col),
        "^".repeat(marker_len)
    )
}

fn source_context_for_span_colored(source: &str, span: Span, color: bool) -> String {
    let context = source_context_for_span(source, span);
    if !color || context.is_empty() {
        return context;
    }
    context
        .lines()
        .map(|line| {
            if line.contains('^') {
                format!("\x1b[31m{}\x1b[0m", line)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn colorize(label: &str, color: bool) -> String {
    if color {
        format!("\x1b[31m{}\x1b[0m", label)
    } else {
        label.to_string()
    }
}

fn format_parse_error(error: &subset_julia_vm_parser::ParseError) -> String {
    use subset_julia_vm_parser::ParseError;

    let location = |span: &subset_julia_vm_parser::Span| {
        format!("line {}, column {}", span.start_line, span.start_column)
    };

    match error {
        ParseError::UnexpectedToken {
            found,
            expected,
            span,
        } => format!(
            "unexpected token '{}', expected {} at {}",
            found,
            expected,
            location(span)
        ),
        ParseError::UnexpectedEof { expected, span } => format!(
            "unexpected end of input, expected {} at {}",
            expected,
            location(span)
        ),
        ParseError::InvalidEscape { sequence, span } => format!(
            "invalid escape sequence '{}' at {}",
            sequence,
            location(span)
        ),
        ParseError::UnterminatedString { span } => {
            format!("unterminated string literal starting at {}", location(span))
        }
        ParseError::UnterminatedCommand { span } => {
            format!(
                "unterminated command literal starting at {}",
                location(span)
            )
        }
        ParseError::UnterminatedCharacter { span } => {
            format!(
                "unterminated character literal starting at {}",
                location(span)
            )
        }
        ParseError::UnterminatedBlockComment { span } => {
            format!("unterminated block comment starting at {}", location(span))
        }
        ParseError::InvalidNumber { literal, span } => {
            format!("invalid number literal '{}' at {}", literal, location(span))
        }
        ParseError::InvalidCharacter { span } => {
            format!("invalid character literal at {}", location(span))
        }
        ParseError::MismatchedBrackets {
            expected,
            found,
            span,
        } => format!(
            "mismatched brackets: expected '{}', found '{}' at {}",
            expected,
            found,
            location(span)
        ),
        ParseError::UnclosedBracket { bracket, span } => {
            format!("unclosed bracket '{}' at {}", bracket, location(span))
        }
        ParseError::InvalidSyntax { message, span } => {
            format!("{} at {}", message, location(span))
        }
        ParseError::LexerError { span } => format!("unrecognized token at {}", location(span)),
    }
}

fn format_parse_errors(errors: &subset_julia_vm_parser::ParseErrors) -> String {
    errors
        .iter()
        .map(format_parse_error)
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_source_with_diagnostics(source: &str) -> Result<ParseOutcome, Box<SourceDiagnostic>> {
    let parser = subset_julia_vm_parser::Parser::new(source);
    let (cst, errors) = parser.parse();
    if !errors.is_empty() {
        let span = errors
            .first()
            .and_then(|error| error.span().map(Span::from_parser_span));
        let message = format_parse_errors(&errors);
        return Err(Box::new(SourceDiagnostic::new(
            AotError::ParseError {
                message,
                span: None,
            },
            span,
        )));
    }

    Ok(ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    }))
}

fn format_lowering_error(error: &UnsupportedFeature) -> String {
    format!(
        "{} at {}:{}",
        error, error.span.start_line, error.span.start_column
    )
}

/// Parse + lower a Julia source string, merging the AoT/base prelude.
fn build_program(source: &str, minimal_prelude: bool) -> Result<Program, Box<SourceDiagnostic>> {
    let mut parser = Parser::new().map_err(|e| {
        Box::new(SourceDiagnostic::from_error(AotError::InternalError(
            format!("Failed to create parser: {:?}", e),
        )))
    })?;

    let prelude_src = if minimal_prelude {
        base::get_aot_prelude()
    } else {
        base::get_prelude()
    };
    let prelude_outcome = parser.parse(&prelude_src).map_err(|e| {
        Box::new(SourceDiagnostic::from_error(AotError::InternalError(
            format!("Failed to parse prelude: {:?}", e),
        )))
    })?;
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    subset_julia_vm::macro_runtime::install();
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering.lower(prelude_outcome).map_err(|e| {
        Box::new(SourceDiagnostic::from_error(AotError::InternalError(
            format!("Prelude lowering error: {:?}", e),
        )))
    })?;

    let outcome = parse_source_with_diagnostics(source)?;
    // Macro expansion seam (Issue #8656): idempotent install of the VM-backed expander.
    subset_julia_vm::macro_runtime::install();
    let mut lowering = Lowering::new(source);
    let mut program = lowering.lower(outcome).map_err(|e| {
        Box::new(SourceDiagnostic::new(
            AotError::LoweringError {
                message: format_lowering_error(&e),
                span: Some(e.span),
            },
            Some(e.span),
        ))
    })?;
    localize_main_block(&mut program);

    merge_prelude(&mut program, prelude_program);
    load_external_modules(&mut program).map_err(|e| Box::new(SourceDiagnostic::from_error(e)))?;
    // Prune functions unreachable from user code before codegen, mirroring the
    // e2e pipeline (compile_to_rust_with_base_optimized). Without this,
    // prelude-only paths (e.g. a BigInt constructor, Issue #6975 class) reach
    // codegen and reject programs the pipeline can otherwise compile
    // (Issue #8789).
    let call_graph = CallGraph::from_program(&program);
    let program = call_graph.filter_program(&program);
    Ok(program)
}

fn localize_main_block(program: &mut Program) {
    if program.main.stmts.is_empty() {
        return;
    }

    let span = nonzero_span(program.main.span);
    let stmts = std::mem::take(&mut program.main.stmts);
    program.main.stmts.push(Stmt::Expr {
        expr: Expr::LetBlock {
            bindings: Vec::new(),
            body: Block { stmts, span },
            span,
        },
        span,
    });
}

fn nonzero_span(span: Span) -> Span {
    if span.start == 0 && span.end == 0 {
        Span::new(0, 0, 1, 1, 1, 1)
    } else {
        span
    }
}

/// Merge the lowered prelude into a user program (structs/abstract/functions).
fn merge_prelude(program: &mut Program, prelude_program: Program) {
    let mut chronology = DefinitionOrderCursor::after_program(&prelude_program);
    chronology.append_fragment(&mut *program);
    let user_method_sigs: HashSet<_> = program
        .functions
        .iter()
        .map(|f| get_method_signature(f))
        .collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.append(&mut program.structs);
    program.structs = all_structs;

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

    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_method_sigs.contains(&get_method_signature(f)))
        .map(|mut f| {
            // `prelude_program` was just lowered above, so each Arc here is
            // uniquely owned (refcount 1) — `make_mut` never clones.
            std::sync::Arc::make_mut(&mut f).is_base_extension = true;
            f
        })
        .collect();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;
}

/// Load external (non-relative) `using` modules referenced by the program.
fn load_external_modules(program: &mut Program) -> Result<(), AotError> {
    let mut package_loader = loader::PackageLoader::new(loader::LoaderConfig::from_env());
    package_loader
        .load_into_program(program)
        .map_err(|e| AotError::InternalError(format!("Load error: {:?}", e)))
}

/// Print a `--stats` report (Issue #6930).
fn print_stats(output: &AotOutput) {
    println!();
    println!("Statistics:");
    println!(
        "  Functions total (before DCE): {}",
        output.stats.functions_total
    );
    println!(
        "  Functions compiled (after DCE): {}",
        output.stats.functions_compiled
    );
    println!(
        "  Functions eliminated by DCE: {}",
        output.stats.functions_eliminated
    );
    println!(
        "  Instructions processed: {}",
        output.stats.instructions_processed
    );
    println!("  Type inferences: {}", output.stats.type_inferences);
    println!("  Dynamic fallbacks: {}", output.stats.dynamic_fallbacks);
    println!(
        "  Optimizations applied: {}",
        output.stats.optimizations_applied
    );
    println!("  Generated Rust LOC: {}", output.generated_loc());
    println!(
        "  Estimated output size: {} bytes",
        output.estimated_bytes()
    );
    if !output.dynamic_op_descriptions.is_empty() {
        println!("  Dynamic dispatch sites:");
        for desc in &output.dynamic_op_descriptions {
            println!("    - {}", desc);
        }
    }
}

/// Print a `--time-passes` report.
fn print_timings(timings: &[(&'static str, std::time::Duration)]) {
    println!();
    println!("Pass timings:");
    let mut total = std::time::Duration::ZERO;
    for (name, dur) in timings {
        total += *dur;
        println!("  {:<24} {:>8.3} ms", name, dur.as_secs_f64() * 1000.0);
    }
    println!("  {:<24} {:>8.3} ms", "total", total.as_secs_f64() * 1000.0);
}

#[derive(Debug)]
struct ResolvedInput {
    program: Program,
    source_name: String,
    source: Option<String>,
}

#[derive(Debug)]
struct SourceDiagnostic {
    error: AotError,
    message: String,
    span: Option<Span>,
    workaround: Option<String>,
}

impl SourceDiagnostic {
    fn new(error: AotError, span: Option<Span>) -> Self {
        match error {
            AotError::ParseError {
                message,
                span: error_span,
            } => Self {
                error: AotError::ParseError {
                    message: message.clone(),
                    span: error_span,
                },
                message,
                span: error_span.or(span),
                workaround: None,
            },
            AotError::LoweringError {
                message,
                span: error_span,
            } => Self {
                error: AotError::LoweringError {
                    message: message.clone(),
                    span: error_span,
                },
                message,
                span: error_span.or(span),
                workaround: None,
            },
            other => Self::from_error(other),
        }
    }

    fn from_error(error: AotError) -> Self {
        match error {
            AotError::UnsupportedInstruction(diag) => {
                let message = diag.message.clone();
                let span = diag.span;
                let workaround = diag.workaround.clone();
                Self {
                    error: AotError::UnsupportedInstruction(diag),
                    message,
                    span,
                    workaround,
                }
            }
            other => Self {
                message: other.to_string(),
                error: other,
                span: None,
                workaround: None,
            },
        }
    }

    fn kind(&self) -> &'static str {
        diagnostic_kind(&self.error)
    }
}

#[derive(Debug, Serialize)]
struct JsonDiagnostic<'a> {
    kind: &'a str,
    source: &'a str,
    message: String,
    span: Option<JsonSpan>,
    workaround: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsonSpan {
    start: usize,
    end: usize,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

impl From<Span> for JsonSpan {
    fn from(span: Span) -> Self {
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

fn diagnostic_kind(error: &AotError) -> &'static str {
    match error {
        AotError::ParseError { .. } => "parse",
        AotError::LoweringError { .. } => "lowering",
        AotError::UnsupportedInstruction(_) => "unsupported",
        AotError::CodegenError(_) => "codegen",
        AotError::TypeInferenceError(_) => "type-inference",
        AotError::OptimizationError(_) => "optimization",
        AotError::InvalidIR(_) => "invalid-ir",
        AotError::InternalError(_) => "internal",
        AotError::ConversionError(_) => "conversion",
    }
}

fn render_cli_diagnostic(
    diagnostic: &SourceDiagnostic,
    source_name: &str,
    source: Option<&str>,
    format: DiagnosticFormat,
    color: ColorChoice,
) -> String {
    match format {
        DiagnosticFormat::Human => render_human_diagnostic(diagnostic, source_name, source, color),
        DiagnosticFormat::Json => render_json_diagnostic(diagnostic, source_name),
    }
}

fn render_human_diagnostic(
    diagnostic: &SourceDiagnostic,
    source_name: &str,
    source: Option<&str>,
    color: ColorChoice,
) -> String {
    let color = color.enabled();
    let mut rendered = format!(
        "{}[{}]: {}",
        colorize("error", color),
        diagnostic.kind(),
        diagnostic.error
    );

    append_source_context(&mut rendered, diagnostic.span, source_name, source, color);

    rendered
}

fn append_source_context(
    rendered: &mut String,
    span: Option<Span>,
    source_name: &str,
    source: Option<&str>,
    color: bool,
) {
    let Some(span) = span else {
        return;
    };
    let Some(source) = source else {
        return;
    };
    let context = source_context_for_span_colored(source, span, color);
    if context.is_empty() {
        return;
    }
    rendered.push('\n');
    rendered.push_str(&format!(
        " --> {}:{}:{}\n{}",
        source_name, span.start_line, span.start_column, context
    ));
}

fn render_json_diagnostic(diagnostic: &SourceDiagnostic, source_name: &str) -> String {
    let json = JsonDiagnostic {
        kind: diagnostic.kind(),
        source: source_name,
        message: diagnostic.message.clone(),
        span: diagnostic.span.map(JsonSpan::from),
        workaround: diagnostic.workaround.clone(),
    };
    serde_json::to_string_pretty(&json)
        .unwrap_or_else(|_| format!("{{\"kind\":\"{}\"}}", diagnostic.kind()))
}

fn run() -> i32 {
    let args = match Args::parse_from(env::args()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("Error: {}", message);
            eprintln!("Use --help for usage information");
            return exit_code::USAGE;
        }
    };

    if args.show_help {
        print_help();
        return exit_code::SUCCESS;
    }
    if args.show_version {
        print_version();
        return exit_code::SUCCESS;
    }

    #[cfg(feature = "aot-wasm")]
    if args.backend == Backend::Wasm {
        return aot_wasm::run(&args);
    }

    let resolved = match resolve_program(&args) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let config = CompileConfig {
        source_name: resolved.source_name.clone(),
        backend: args.backend.into(),
        emit_comments: args.emit_comments,
        debug_info: args.debug_info,
        pure_rust: args.pure_rust,
        opt_level: args.opt_level,
        dump_stage: args.dump_aot_stage.clone(),
        c_abi_exports: args.c_abi_exports.clone(),
    };

    if let Some(object_target) = &args.emit_object {
        let result = match compile_cranelift_object_for_target(
            resolved.program,
            &config,
            args.target.as_deref(),
        ) {
            Ok(result) => result,
            Err(e) => {
                let diagnostic = SourceDiagnostic::from_error(e);
                eprintln!(
                    "{}",
                    render_cli_diagnostic(
                        &diagnostic,
                        &resolved.source_name,
                        resolved.source.as_deref(),
                        args.diagnostic_format,
                        args.color,
                    )
                );
                return exit_code_for(&diagnostic.error);
            }
        };

        if !result.dumps.is_empty() {
            println!("{}", result.dumps);
        }
        if object_target == "-" {
            if let Err(e) = std::io::stdout().write_all(&result.object_bytes) {
                eprintln!("Error writing object to stdout: {}", e);
                return exit_code::IO;
            }
        } else {
            if let Err(e) = fs::write(object_target, &result.object_bytes) {
                eprintln!("Error writing object file '{}': {}", object_target, e);
                return exit_code::IO;
            }
            println!("Generated object: {}", object_target);
        }
        if args.show_stats {
            print_stats(&AotOutput::new(String::new(), result.stats));
        }
        if args.time_passes {
            print_timings(&result.timings);
        }
        return exit_code::SUCCESS;
    }

    if args.backend == Backend::Cranelift {
        if let Some(library_target) = &args.emit_library {
            let result = match compile_cranelift_object_for_target(
                resolved.program,
                &config,
                args.target.as_deref(),
            ) {
                Ok(result) => result,
                Err(e) => {
                    let diagnostic = SourceDiagnostic::from_error(e);
                    eprintln!(
                        "{}",
                        render_cli_diagnostic(
                            &diagnostic,
                            &resolved.source_name,
                            resolved.source.as_deref(),
                            args.diagnostic_format,
                            args.color,
                        )
                    );
                    return exit_code_for(&diagnostic.error);
                }
            };

            if !result.dumps.is_empty() {
                println!("{}", result.dumps);
            }
            if let Err(e) = build_cranelift_library(
                &result.object_bytes,
                library_target,
                args.library_kind,
                args.target.as_deref(),
            ) {
                eprintln!(
                    "Error building Cranelift library '{}': {}",
                    library_target, e
                );
                return exit_code::CODEGEN;
            }
            println!("Generated library: {}", library_target);
            if args.show_stats {
                print_stats(&AotOutput::new(String::new(), result.stats));
            }
            if args.time_passes {
                print_timings(&result.timings);
            }
            return exit_code::SUCCESS;
        }

        if let Some(binary_target) = &args.emit_binary {
            let result = match compile_cranelift_object_for_target(
                resolved.program,
                &config,
                args.target.as_deref(),
            ) {
                Ok(result) => result,
                Err(e) => {
                    let diagnostic = SourceDiagnostic::from_error(e);
                    eprintln!(
                        "{}",
                        render_cli_diagnostic(
                            &diagnostic,
                            &resolved.source_name,
                            resolved.source.as_deref(),
                            args.diagnostic_format,
                            args.color,
                        )
                    );
                    return exit_code_for(&diagnostic.error);
                }
            };

            if !result.dumps.is_empty() {
                println!("{}", result.dumps);
            }
            if let Err(e) =
                build_cranelift_binary(&result.object_bytes, binary_target, args.target.as_deref())
            {
                eprintln!("Error building Cranelift binary '{}': {}", binary_target, e);
                return exit_code::CODEGEN;
            }
            println!("Generated binary: {}", binary_target);
            if args.show_stats {
                print_stats(&AotOutput::new(String::new(), result.stats));
            }
            if args.time_passes {
                print_timings(&result.timings);
            }
            return exit_code::SUCCESS;
        }
    }

    if args.jit_run {
        let result = match run_cranelift_jit_main(resolved.program, &config) {
            Ok(result) => result,
            Err(e) => {
                let diagnostic = SourceDiagnostic::from_error(e);
                eprintln!(
                    "{}",
                    render_cli_diagnostic(
                        &diagnostic,
                        &resolved.source_name,
                        resolved.source.as_deref(),
                        args.diagnostic_format,
                        args.color,
                    )
                );
                return exit_code_for(&diagnostic.error);
            }
        };

        if !result.dumps.is_empty() {
            println!("{}", result.dumps);
        }
        if args.show_stats {
            print_stats(&AotOutput::new(String::new(), result.stats));
        }
        if args.time_passes {
            print_timings(&result.timings);
        }
        return exit_code::SUCCESS;
    }

    let result = match compile_program(resolved.program, &config) {
        Ok(result) => result,
        Err(e) => {
            let diagnostic = SourceDiagnostic::from_error(e);
            eprintln!(
                "{}",
                render_cli_diagnostic(
                    &diagnostic,
                    &resolved.source_name,
                    resolved.source.as_deref(),
                    args.diagnostic_format,
                    args.color,
                )
            );
            return exit_code_for(&diagnostic.error);
        }
    };

    if !result.dumps.is_empty() {
        println!("{}", result.dumps);
    }

    // --check: report and exit without writing (Issue #6931).
    if args.check {
        if result.output.dynamic_op_descriptions.is_empty() {
            println!(
                "{}: OK — fully static, no unsupported features detected",
                resolved.source_name
            );
        } else {
            println!(
                "{}: compiles with {} dynamic-dispatch site(s):",
                resolved.source_name,
                result.output.dynamic_op_descriptions.len()
            );
            for desc in &result.output.dynamic_op_descriptions {
                println!("  - {}", desc);
            }
        }
        if args.show_stats {
            print_stats(&result.output);
        }
        if args.time_passes {
            print_timings(&result.timings);
        }
        return exit_code::SUCCESS;
    }

    let should_write_rust = args.output_file.is_some() || args.emit_binary.is_none();
    if should_write_rust {
        let output_target = args
            .output_file
            .clone()
            .unwrap_or_else(|| default_output(&args));

        if output_target == "-" {
            if let Err(e) = std::io::stdout().write_all(result.output.rust_code.as_bytes()) {
                eprintln!("Error writing to stdout: {}", e);
                return exit_code::IO;
            }
        } else {
            if let Err(e) = fs::write(&output_target, &result.output.rust_code) {
                eprintln!("Error writing output file '{}': {}", output_target, e);
                return exit_code::IO;
            }
            println!("Generated: {}", output_target);
        }
    }

    if let Some(binary_target) = &args.emit_binary {
        if let Err(e) = build_generated_binary(
            &result.output.rust_code,
            binary_target,
            args.target.as_deref(),
        ) {
            eprintln!("Error building binary '{}': {}", binary_target, e);
            return exit_code::CODEGEN;
        }
        println!("Generated binary: {}", binary_target);
    }

    if args.show_stats {
        print_stats(&result.output);
    }
    if args.time_passes {
        print_timings(&result.timings);
    }

    if !result.output.warnings.is_empty() {
        println!();
        println!("Warnings:");
        for warning in &result.output.warnings {
            println!("  - {}", warning);
        }
    }

    exit_code::SUCCESS
}

/// Resolve the input arguments into a Core IR program plus diagnostic context.
fn resolve_program(args: &Args) -> Result<ResolvedInput, i32> {
    if let Some(ir_file) = &args.ir_file {
        if !Path::new(ir_file).exists() {
            eprintln!("Error: Core IR file '{}' not found", ir_file);
            return Err(exit_code::IO);
        }
        let program = core_ir_file::load(ir_file).map_err(|e| {
            eprintln!("Error: failed to load Core IR '{}': {}", ir_file, e);
            exit_code::IO
        })?;
        // Apply the same reachability pruning as the source path (Issue #8789).
        let call_graph = CallGraph::from_program(&program);
        let program = call_graph.filter_program(&program);
        Ok(ResolvedInput {
            program,
            source_name: ir_file.clone(),
            source: None,
        })
    } else if let Some(code) = &args.code {
        let program = build_program(code, args.minimal_prelude).map_err(|e| {
            eprintln!(
                "{}",
                render_cli_diagnostic(&e, "<eval>", Some(code), args.diagnostic_format, args.color,)
            );
            exit_code_for(&e.error)
        })?;
        Ok(ResolvedInput {
            program,
            source_name: "<eval>".to_string(),
            source: Some(code.clone()),
        })
    } else if let Some(input) = &args.input_file {
        let source = read_source(input).map_err(|e| {
            eprintln!("Error: {}", e);
            exit_code::IO
        })?;
        let program = build_program(&source, args.minimal_prelude).map_err(|e| {
            eprintln!(
                "{}",
                render_cli_diagnostic(&e, input, Some(&source), args.diagnostic_format, args.color,)
            );
            exit_code_for(&e.error)
        })?;
        Ok(ResolvedInput {
            program,
            source_name: input.clone(),
            source: Some(source),
        })
    } else {
        eprintln!("Error: No input file, code, or Core IR file provided");
        eprintln!("Use --help for usage information");
        Err(exit_code::USAGE)
    }
}

/// Compute the default output path from the input arguments.
fn default_output(args: &Args) -> String {
    let stem_of = |p: &str| {
        Path::new(p)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty() && s != "-")
    };
    if let Some(ir) = &args.ir_file {
        if let Some(stem) = stem_of(ir) {
            return format!("{}.rs", stem);
        }
    }
    if let Some(input) = &args.input_file {
        if let Some(stem) = stem_of(input) {
            return format!("{}.rs", stem);
        }
    }
    "output.rs".to_string()
}

fn build_generated_binary(
    rust_code: &str,
    output_path: &str,
    target: Option<&str>,
) -> Result<(), String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .ok_or_else(|| "failed to resolve workspace root from CARGO_MANIFEST_DIR".to_string())?;
    let runtime_path = workspace_root.join("subset_julia_vm_runtime");
    if !runtime_path.exists() {
        return Err(format!(
            "runtime crate not found at {}",
            runtime_path.display()
        ));
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let tmp_dir = env::temp_dir().join(format!("sjulia-aot-link-{}-{}", process::id(), stamp));

    let result =
        build_generated_binary_in_tmp(rust_code, output_path, &runtime_path, &tmp_dir, target);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn build_generated_binary_in_tmp(
    rust_code: &str,
    output_path: &str,
    runtime_path: &Path,
    tmp_dir: &Path,
    target: Option<&str>,
) -> Result<(), String> {
    let src_dir = tmp_dir.join("src");
    fs::create_dir_all(&src_dir)
        .map_err(|e| format!("failed to create temp Cargo project: {}", e))?;
    fs::write(src_dir.join("main.rs"), rust_code)
        .map_err(|e| format!("failed to write generated Rust source: {}", e))?;

    let cargo_toml = format!(
        r#"[package]
name = "sjulia_aot_generated"
version = "0.1.0"
edition = "2021"

[dependencies]
subset_julia_vm_runtime = {{ path = "{}" }}
"#,
        runtime_path.display()
    );
    fs::write(tmp_dir.join("Cargo.toml"), cargo_toml)
        .map_err(|e| format!("failed to write temp Cargo.toml: {}", e))?;

    let target_dir = tmp_dir.join("target");
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--release")
        .arg("--manifest-path")
        .arg(tmp_dir.join("Cargo.toml"));
    if let Some(target) = target {
        command.arg("--target").arg(target);
    }
    let status = command
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .map_err(|e| format!("failed to run cargo build: {}", e))?;
    if !status.success() {
        return Err(format!("cargo build failed with status {}", status));
    }

    let binary = if let Some(target) = target {
        target_dir
            .join(target)
            .join("release")
            .join("sjulia_aot_generated")
    } else {
        target_dir.join("release").join("sjulia_aot_generated")
    };
    let output = PathBuf::from(output_path);
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create output directory: {}", e))?;
    }
    fs::copy(&binary, &output).map_err(|e| {
        format!(
            "failed to copy built binary from {} to {}: {}",
            binary.display(),
            output.display(),
            e
        )
    })?;
    Ok(())
}

fn build_cranelift_binary(
    object_bytes: &[u8],
    output_path: &str,
    target: Option<&str>,
) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let tmp_dir =
        env::temp_dir().join(format!("sjulia-cranelift-link-{}-{}", process::id(), stamp));
    let result = build_cranelift_binary_in_tmp(object_bytes, output_path, &tmp_dir, target);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn build_cranelift_binary_in_tmp(
    object_bytes: &[u8],
    output_path: &str,
    tmp_dir: &Path,
    target: Option<&str>,
) -> Result<(), String> {
    fs::create_dir_all(tmp_dir)
        .map_err(|e| format!("failed to create Cranelift link temp directory: {}", e))?;
    let object_path = tmp_dir.join(format!("main.{}", cranelift_object_extension(target)));
    fs::write(&object_path, object_bytes)
        .map_err(|e| format!("failed to write temporary Cranelift object: {}", e))?;

    let output = PathBuf::from(output_path);
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create output directory: {}", e))?;
    }

    let mut link_config = LinkerConfig::new(output);
    link_config.object_files.push(object_path);
    link_config.target_triple = target.map(str::to_string);
    link_objects(&link_config).map_err(|e| e.to_string())
}

fn build_cranelift_library(
    object_bytes: &[u8],
    output_path: &str,
    library_kind: LibraryKind,
    target: Option<&str>,
) -> Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let tmp_dir = env::temp_dir().join(format!("sjulia-cranelift-lib-{}-{}", process::id(), stamp));
    let result =
        build_cranelift_library_in_tmp(object_bytes, output_path, library_kind, &tmp_dir, target);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

fn build_cranelift_library_in_tmp(
    object_bytes: &[u8],
    output_path: &str,
    library_kind: LibraryKind,
    tmp_dir: &Path,
    target: Option<&str>,
) -> Result<(), String> {
    fs::create_dir_all(tmp_dir)
        .map_err(|e| format!("failed to create Cranelift library temp directory: {}", e))?;
    let object_path = tmp_dir.join(format!("library.{}", cranelift_object_extension(target)));
    fs::write(&object_path, object_bytes)
        .map_err(|e| format!("failed to write temporary Cranelift object: {}", e))?;

    let output = PathBuf::from(output_path);
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create output directory: {}", e))?;
    }

    match library_kind {
        LibraryKind::Static => archive_static_library(&object_path, &output),
        LibraryKind::Shared => {
            let mut link_config = LinkerConfig::new(output);
            link_config.object_files.push(object_path);
            link_config.target_triple = target.map(str::to_string);
            link_config.output_kind = LinkOutputKind::SharedLibrary;
            link_objects(&link_config).map_err(|e| e.to_string())
        }
    }
}

fn archive_static_library(object_path: &Path, output_path: &Path) -> Result<(), String> {
    let status = Command::new("ar")
        .arg("crs")
        .arg(output_path)
        .arg(object_path)
        .status()
        .map_err(|e| format!("failed to run ar: {}", e))?;
    if !status.success() {
        return Err(format!("ar failed with status {}", status));
    }
    Ok(())
}

fn cranelift_object_extension(target: Option<&str>) -> &'static str {
    if target.is_some_and(|target| target.contains("windows-msvc")) {
        "obj"
    } else {
        "o"
    }
}

pub(crate) fn main() {
    process::exit(run());
}

#[cfg(test)]
mod tests {
    use super::*;
    use subset_julia_vm::aot::UnsupportedInstructionDiagnostic;

    #[test]
    fn format_parse_error_handles_unterminated_command_11657() {
        let error = subset_julia_vm_parser::ParseError::UnterminatedCommand {
            span: subset_julia_vm_parser::Span::new(4, 5, 2, 2, 3, 4),
        };

        assert_eq!(
            format_parse_error(&error),
            "unterminated command literal starting at line 2, column 3"
        );
    }

    #[test]
    fn parse_rejects_unknown_option() {
        let result = Args::parse_from(["juliars", "--frobnicate", "input.jl"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_accepts_dump_aot_stage_option() {
        let result = Args::parse_from([
            "juliars",
            "--dump-aot-stage=BeforeBackendCodegen",
            "input.jl",
        ])
        .unwrap();
        assert_eq!(
            result.dump_aot_stage.as_deref(),
            Some("BeforeBackendCodegen")
        );
    }

    #[test]
    fn parse_rejects_conflicting_inputs() {
        // -e and a file together (Issue #6929)
        assert!(Args::parse_from(["juliars", "-e", "1+2", "input.jl"]).is_err());
        assert!(Args::parse_from(["juliars", "--ir", "p.sjir", "-e", "1+2"]).is_err());
    }

    #[test]
    fn parse_accepts_single_input() {
        assert!(Args::parse_from(["juliars", "input.jl"]).is_ok());
        assert!(Args::parse_from(["juliars", "-e", "1+2"]).is_ok());
        assert!(Args::parse_from(["juliars", "--ir", "p.sjir"]).is_ok());
    }

    #[test]
    fn parse_opt_levels() {
        assert_eq!(
            Args::parse_from(["juliars", "-O0", "in.jl"])
                .unwrap()
                .opt_level,
            OptLevel::O0
        );
        assert_eq!(
            Args::parse_from(["juliars", "-O3", "in.jl"])
                .unwrap()
                .opt_level,
            OptLevel::O3
        );
        assert_eq!(
            Args::parse_from(["juliars", "--opt-level=1", "in.jl"])
                .unwrap()
                .opt_level,
            OptLevel::O1
        );
        assert_eq!(
            Args::parse_from(["juliars", "in.jl"]).unwrap().opt_level,
            OptLevel::O2
        );
        assert!(Args::parse_from(["juliars", "-O9", "in.jl"]).is_err());
    }

    #[test]
    fn parse_stdin_dash_input() {
        let args = Args::parse_from(["juliars", "-", "-o", "-"]).unwrap();
        assert_eq!(args.input_file.as_deref(), Some("-"));
        assert_eq!(args.output_file.as_deref(), Some("-"));
    }

    #[test]
    fn parse_backend_selection() {
        assert!(Args::parse_from(["juliars", "--backend=cranelift", "in.jl"]).is_ok());
        assert!(Args::parse_from(["juliars", "--backend", "rust", "in.jl"]).is_ok());
        assert!(Args::parse_from(["juliars", "--backend", "llvm", "in.jl"]).is_err());
    }

    #[cfg(feature = "aot-wasm")]
    #[test]
    fn parse_accepts_wasm_backend_with_wasm_output() {
        // Given: the Todo 3b Wasm backend and required binary output path.
        // When: the typed CLI boundary parses the request.
        let args = Args::parse_from(["juliars", "--backend=wasm", "--emit-wasm=out.wasm", "in.jl"])
            .expect("aot-wasm builds should accept Wasm output");

        // Then: neither option can silently fall back to a native backend.
        assert_eq!(args.backend, Backend::Wasm);
        assert_eq!(args.emit_wasm.as_deref(), Some("out.wasm"));
    }

    #[cfg(feature = "aot-wasm")]
    #[test]
    fn parse_rejects_incomplete_or_conflicting_wasm_output() {
        // Given: Wasm requests missing their paired option or mixed with native output.
        let cases = [
            vec!["juliars", "--backend=wasm", "in.jl"],
            vec!["juliars", "--emit-wasm=out.wasm", "in.jl"],
            vec!["juliars", "in.jl", "--backend=wasm", "--emit-wasm"],
            vec!["juliars", "in.jl", "--backend=wasm", "--emit-wasm="],
            vec![
                "juliars",
                "--backend=wasm",
                "--emit-wasm=out.wasm",
                "-o",
                "out.rs",
                "in.jl",
            ],
        ];

        // When: each invalid boundary request is parsed.
        let errors = cases
            .into_iter()
            .map(Args::parse_from)
            .map(|result| result.expect_err("invalid Wasm request must fail"))
            .collect::<Vec<_>>();

        // Then: every request is rejected instead of selecting another backend.
        assert!(errors.iter().all(|error| error.contains("wasm")));
    }

    #[test]
    fn parse_cranelift_jit_run_option_issue_7131() {
        assert!(Args::parse_from(["juliars", "--backend=cranelift", "--jit-run", "in.jl"]).is_ok());
        assert!(Args::parse_from(["juliars", "--jit-run", "in.jl"]).is_err());
        assert!(Args::parse_from(["juliars", "--backend=rust", "--jit-run", "in.jl"]).is_err());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "--jit-run",
            "--check",
            "in.jl"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "--jit-run",
            "--emit-binary=out",
            "in.jl"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "--jit-run",
            "-o",
            "out.rs",
            "in.jl"
        ])
        .is_err());
    }

    #[test]
    fn parse_emit_binary_option() {
        let args = Args::parse_from(["juliars", "in.jl", "--emit-binary=out"]).unwrap();
        assert_eq!(args.emit_binary.as_deref(), Some("out"));
        assert!(args.output_file.is_none());

        let args = Args::parse_from(["juliars", "in.jl", "--emit-binary", "out"]).unwrap();
        assert_eq!(args.emit_binary.as_deref(), Some("out"));

        assert!(Args::parse_from(["juliars", "in.jl", "--emit-binary"]).is_err());
        assert!(Args::parse_from(["juliars", "in.jl", "--emit-binary="]).is_err());
    }

    #[test]
    fn parse_cranelift_emit_binary_option_issue_7083() {
        let args = Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-binary=out",
        ])
        .unwrap();
        assert_eq!(args.backend, Backend::Cranelift);
        assert_eq!(args.emit_binary.as_deref(), Some("out"));

        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-binary=out",
            "-o",
            "out.rs"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-binary=out",
            "--check"
        ])
        .is_err());
    }

    #[test]
    fn parse_cranelift_emit_object_option_issue_7082() {
        let args = Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "--emit-object=out.o",
            "in.jl",
        ])
        .unwrap();
        assert_eq!(args.emit_object.as_deref(), Some("out.o"));

        let args = Args::parse_from([
            "juliars",
            "--backend",
            "cranelift",
            "in.jl",
            "--emit-object",
            "out.o",
        ])
        .unwrap();
        assert_eq!(args.emit_object.as_deref(), Some("out.o"));

        assert!(Args::parse_from(["juliars", "in.jl", "--emit-object=out.o"]).is_err());
        assert!(
            Args::parse_from(["juliars", "--backend=cranelift", "in.jl", "--emit-object"]).is_err()
        );
        assert!(
            Args::parse_from(["juliars", "--backend=cranelift", "in.jl", "--emit-object="])
                .is_err()
        );
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-object=out.o",
            "--emit-binary=out"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-object=out.o",
            "--jit-run"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-object=out.o",
            "--export-c-abi=sjulia_add=add(Int64,Int64)"
        ])
        .is_ok());
        assert!(Args::parse_from([
            "juliars",
            "--backend=cranelift",
            "in.jl",
            "--emit-object=out.o",
            "--target=x86_64-unknown-linux-gnu"
        ])
        .is_ok());
    }

    #[test]
    fn parse_emit_binary_target_option() {
        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--emit-binary",
            "out",
            "--target=aarch64-apple-ios-sim",
        ])
        .unwrap();
        assert_eq!(args.target.as_deref(), Some("aarch64-apple-ios-sim"));

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--emit-binary",
            "out",
            "--target",
            "aarch64-apple-ios",
        ])
        .unwrap();
        assert_eq!(args.target.as_deref(), Some("aarch64-apple-ios"));

        assert!(
            Args::parse_from(["juliars", "in.jl", "--target=x86_64-unknown-linux-gnu"]).is_err()
        );
        assert!(
            Args::parse_from(["juliars", "in.jl", "--emit-binary", "out", "--target"]).is_err()
        );
        assert!(
            Args::parse_from(["juliars", "in.jl", "--emit-binary", "out", "--target="]).is_err()
        );
    }

    #[test]
    fn parse_cranelift_emit_library_option_issue_7085() {
        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
        ])
        .unwrap();
        assert_eq!(args.emit_library.as_deref(), Some("libout.a"));
        assert_eq!(args.library_kind, LibraryKind::Static);

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--backend",
            "cranelift",
            "--emit-library",
            "libout.dylib",
            "--library-kind=shared",
            "--target=x86_64-apple-darwin",
        ])
        .unwrap();
        assert_eq!(args.emit_library.as_deref(), Some("libout.dylib"));
        assert_eq!(args.library_kind, LibraryKind::Shared);
        assert_eq!(args.target.as_deref(), Some("x86_64-apple-darwin"));

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--library-kind",
            "static",
        ])
        .unwrap();
        assert_eq!(args.library_kind, LibraryKind::Static);
        assert!(args.library_kind_specified);

        assert!(Args::parse_from(["juliars", "in.jl", "--emit-library=libout.a"]).is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--emit-binary=out"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--emit-object=out.o"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--check"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "-o",
            "out.rs"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--jit-run"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--library-kind=static"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--library-kind=shared"
        ])
        .is_err());
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--library-kind=dylib"
        ])
        .is_err());
    }

    #[test]
    fn parse_cranelift_debug_info_option_issue_7090() {
        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-object=out.o",
            "--debug-info",
        ])
        .unwrap();
        assert!(args.debug_info);

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-binary=out",
            "--debug-info",
        ])
        .unwrap();
        assert!(args.debug_info);

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-library=libout.a",
            "--debug-info",
        ])
        .unwrap();
        assert!(args.debug_info);

        assert!(
            Args::parse_from(["juliars", "in.jl", "--emit-object=out.o", "--debug-info"]).is_err()
        );
        assert!(
            Args::parse_from(["juliars", "in.jl", "--backend=cranelift", "--debug-info"]).is_err()
        );
        assert!(Args::parse_from([
            "juliars",
            "in.jl",
            "--backend=cranelift",
            "--emit-object=out.o",
            "--debug-info",
            "--jit-run"
        ])
        .is_err());
    }

    #[test]
    fn parse_c_abi_export_option_issue_6990() {
        let args = Args::parse_from(["juliars", "in.jl", "--export-c-abi=square"]).unwrap();
        assert_eq!(args.c_abi_exports.len(), 1);
        assert_eq!(args.c_abi_exports[0].export_name, "square");
        assert_eq!(args.c_abi_exports[0].function_name, "square");
        assert_eq!(args.c_abi_exports[0].arg_types, None);

        let args =
            Args::parse_from(["juliars", "in.jl", "--export-c-abi", "sjulia_add=add"]).unwrap();
        assert_eq!(args.c_abi_exports[0].export_name, "sjulia_add");
        assert_eq!(args.c_abi_exports[0].function_name, "add");

        assert!(Args::parse_from(["juliars", "in.jl", "--export-c-abi"]).is_err());
        assert!(Args::parse_from(["juliars", "in.jl", "--export-c-abi="]).is_err());
        assert!(Args::parse_from(["juliars", "in.jl", "--export-c-abi=a="]).is_err());
        assert!(Args::parse_from(["juliars", "in.jl", "--export-c-abi==a"]).is_err());
    }

    #[test]
    fn parse_c_abi_export_signature_and_bulk_specs_issue_7078() {
        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--export-c-abi",
            "sjulia_add_i64=add(Int64, Int64),sjulia_add_f64=add(Float64,Float64)",
        ])
        .unwrap();

        assert_eq!(args.c_abi_exports.len(), 2);
        assert_eq!(args.c_abi_exports[0].export_name, "sjulia_add_i64");
        assert_eq!(args.c_abi_exports[0].function_name, "add");
        assert_eq!(
            args.c_abi_exports[0].arg_types,
            Some(vec![StaticType::I64, StaticType::I64])
        );
        assert_eq!(args.c_abi_exports[1].export_name, "sjulia_add_f64");
        assert_eq!(args.c_abi_exports[1].function_name, "add");
        assert_eq!(
            args.c_abi_exports[1].arg_types,
            Some(vec![StaticType::F64, StaticType::F64])
        );

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--export-c-abi=update=update!(Matrix{UInt8},Int64,Int64)",
        ])
        .unwrap();
        assert_eq!(
            args.c_abi_exports[0].arg_types,
            Some(vec![
                StaticType::Array {
                    element: Box::new(StaticType::U8),
                    ndims: Some(2),
                },
                StaticType::I64,
                StaticType::I64,
            ])
        );

        assert!(Args::parse_from(["juliars", "in.jl", "--export-c-abi=add(Int64)"]).is_err());
        assert!(
            Args::parse_from(["juliars", "in.jl", "--export-c-abi=sjulia_add=add(String)"])
                .is_err()
        );
        assert!(
            Args::parse_from(["juliars", "in.jl", "--export-c-abi=sjulia_add=add(Int64"]).is_err()
        );
    }

    #[test]
    fn parse_diagnostic_options_issue_6996() {
        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--diagnostic-format=json",
            "--color=always",
        ])
        .unwrap();
        assert_eq!(args.diagnostic_format, DiagnosticFormat::Json);
        assert_eq!(args.color, ColorChoice::Always);

        let args = Args::parse_from([
            "juliars",
            "in.jl",
            "--diagnostic-format",
            "human",
            "--color",
            "never",
        ])
        .unwrap();
        assert_eq!(args.diagnostic_format, DiagnosticFormat::Human);
        assert_eq!(args.color, ColorChoice::Never);

        assert!(Args::parse_from(["juliars", "in.jl", "--diagnostic-format=xml"]).is_err());
        assert!(Args::parse_from(["juliars", "in.jl", "--color=sometimes"]).is_err());
    }

    #[test]
    fn unsupported_diagnostic_renders_source_context_issue_6996() {
        let err = AotError::UnsupportedInstruction(
            UnsupportedInstructionDiagnostic::new("ccall is not supported")
                .with_span(Span::new(6, 11, 2, 2, 1, 6))
                .with_workaround("use a pure Julia wrapper"),
        );
        let diagnostic = SourceDiagnostic::from_error(err);

        let rendered = render_cli_diagnostic(
            &diagnostic,
            "<eval>",
            Some("x = 1\nccall(:puts, Cint, ())\n"),
            DiagnosticFormat::Human,
            ColorChoice::Never,
        );

        assert!(rendered.contains("error[unsupported]"));
        assert!(rendered.contains(" --> <eval>:2:1"));
        assert!(rendered.contains("2 | ccall(:puts, Cint, ())"));
        assert!(rendered.contains("^"));
        assert!(rendered.contains("Workaround: use a pure Julia wrapper"));
    }

    #[test]
    fn unsupported_diagnostic_can_render_color_and_json_issue_6996() {
        let err = AotError::UnsupportedInstruction(
            UnsupportedInstructionDiagnostic::new("ccall is not supported")
                .with_span(Span::new(6, 11, 2, 2, 1, 6))
                .with_workaround("use a pure Julia wrapper"),
        );
        let diagnostic = SourceDiagnostic::from_error(err);

        let colored = render_cli_diagnostic(
            &diagnostic,
            "<eval>",
            Some("x = 1\nccall(:puts, Cint, ())\n"),
            DiagnosticFormat::Human,
            ColorChoice::Always,
        );
        assert!(colored.contains("\x1b[31merror\x1b[0m[unsupported]"));

        let json = render_cli_diagnostic(
            &diagnostic,
            "<eval>",
            None,
            DiagnosticFormat::Json,
            ColorChoice::Never,
        );
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["kind"], "unsupported");
        assert_eq!(value["source"], "<eval>");
        assert_eq!(value["message"], "ccall is not supported");
        assert_eq!(value["span"]["start_line"], 2);
        assert_eq!(value["workaround"], "use a pure Julia wrapper");
    }

    #[test]
    fn build_program_localizes_timing_macro_main_block_issue_7059() {
        let source = "elapsed = @elapsed 1 + 2\nprintln(elapsed >= 0.0)\n";
        let program = match build_program(source, true) {
            Ok(program) => program,
            Err(err) => panic!("@elapsed source should lower: {err:?}"),
        };
        let config = CompileConfig {
            source_name: "<test>".to_string(),
            backend: AotBackend::Rust,
            emit_comments: false,
            debug_info: false,
            pure_rust: false,
            opt_level: OptLevel::O2,
            dump_stage: None,
            c_abi_exports: Vec::new(),
        };

        let result = match compile_program(program, &config) {
            Ok(result) => result,
            Err(err) => panic!("@elapsed should compile through CLI path: {err:?}"),
        };

        assert!(
            result
                .output
                .rust_code
                .contains("std::time::SystemTime::now()"),
            "CLI path should lower time_ns() to the AoT timing builtin"
        );
    }

    #[test]
    fn parse_error_keeps_source_context() {
        let source = "x = (";
        let diagnostic = match parse_source_with_diagnostics(source) {
            Err(diagnostic) => diagnostic,
            Ok(_) => panic!("expected parse error"),
        };
        assert!(matches!(&diagnostic.error, AotError::ParseError { .. }));
        assert!(!diagnostic.message.contains("ParseFailed"));
        assert!(!diagnostic.message.contains("Span {"));
        assert!(diagnostic
            .message
            .contains("unexpected end of input, expected expression at line 1, column 6"));

        let rendered = render_cli_diagnostic(
            &diagnostic,
            "<eval>",
            Some(source),
            DiagnosticFormat::Human,
            ColorChoice::Never,
        );
        assert!(!rendered.contains("Span {"));
        assert!(rendered.contains(" --> <eval>:1:6"));
        assert!(rendered.contains("1 | x = ("));
        assert!(rendered.contains("^"));
    }

    #[test]
    fn lowering_error_keeps_span_context() {
        let source = "1 = 2";
        let err = UnsupportedFeature::new(
            subset_julia_vm::error::UnsupportedFeatureKind::UnsupportedAssignmentTarget,
            Span::new(0, 1, 1, 1, 1, 2),
        );
        let message = format_lowering_error(&err);
        assert!(!message.contains("UnsupportedFeature"));
        assert!(message.contains("Unsupported feature: unsupported assignment target at 1:1"));

        let diagnostic = SourceDiagnostic::new(
            AotError::LoweringError {
                message: message.clone(),
                span: Some(err.span),
            },
            Some(err.span),
        );
        let rendered = render_cli_diagnostic(
            &diagnostic,
            "<eval>",
            Some(source),
            DiagnosticFormat::Human,
            ColorChoice::Never,
        );
        assert!(rendered.contains(" --> <eval>:1:1"));
        assert!(rendered.contains("1 | 1 = 2"));
        assert!(rendered.contains("^"));
    }

    #[test]
    fn exit_codes_are_classified() {
        assert_eq!(
            exit_code_for(&AotError::ParseError {
                message: "x".into(),
                span: None,
            }),
            exit_code::PARSE
        );
        assert_eq!(
            exit_code_for(&AotError::UnsupportedInstruction(
                subset_julia_vm::aot::UnsupportedInstructionDiagnostic::new("x")
            )),
            exit_code::UNSUPPORTED
        );
        assert_eq!(
            exit_code_for(&AotError::CodegenError("x".into())),
            exit_code::CODEGEN
        );
        assert_eq!(
            exit_code_for(&AotError::InternalError("x".into())),
            exit_code::INTERNAL
        );
    }
}
