use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use subset_julia_vm::aot::{compile_wasm_source, AotBackend, CompileConfig};

use super::{
    exit_code, exit_code_for, print_stats, print_timings, read_source, render_cli_diagnostic, Args,
    SourceDiagnostic,
};

pub(super) fn run(args: &Args) -> i32 {
    let (source_name, source) = match source_input(args) {
        Ok(input) => input,
        Err((message, code)) => {
            eprintln!("Error: {message}");
            return code;
        }
    };
    let config = wasm_compile_config(args, &source_name);
    let result = match compile_wasm_source(&source, &config) {
        Ok(result) => result,
        Err(error) => {
            let diagnostic = SourceDiagnostic::from_error(error);
            eprintln!(
                "{}",
                render_cli_diagnostic(
                    &diagnostic,
                    &source_name,
                    Some(&source),
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
    let Some(output) = args.emit_wasm.as_deref() else {
        eprintln!("Error: --backend wasm requires --emit-wasm PATH");
        return exit_code::USAGE;
    };
    if let Err(error) = atomic_write(Path::new(output), &result.wasm_bytes) {
        eprintln!("Error writing Wasm file '{output}': {error}");
        return exit_code::IO;
    }
    println!("Generated Wasm: {output}");
    if args.show_stats {
        print_stats(&super::AotOutput::new(String::new(), result.stats));
    }
    if args.time_passes {
        print_timings(&result.timings);
    }
    exit_code::SUCCESS
}

fn wasm_compile_config(args: &Args, source_name: &str) -> CompileConfig {
    CompileConfig {
        source_name: source_name.to_string(),
        backend: AotBackend::Wasm,
        emit_comments: args.emit_comments,
        debug_info: args.debug_info,
        pure_rust: args.pure_rust,
        opt_level: args.opt_level,
        dump_stage: args.dump_aot_stage.clone(),
        c_abi_exports: args.c_abi_exports.clone(),
    }
}

fn source_input(args: &Args) -> Result<(String, String), (String, i32)> {
    if let Some(code) = &args.code {
        return Ok(("<eval>".to_string(), code.clone()));
    }
    let Some(input) = &args.input_file else {
        return Err((
            "No input file or code provided".to_string(),
            exit_code::USAGE,
        ));
    };
    read_source(input)
        .map(|source| (input.clone(), source))
        .map_err(|error| (error, exit_code::IO))
}

struct PendingFile {
    path: Option<PathBuf>,
}

impl PendingFile {
    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}

fn atomic_write(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .ok_or_else(|| "output path must name a file".to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock error: {error}"))?
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        name.to_string_lossy(),
        process::id(),
        stamp
    ));
    let mut pending = PendingFile {
        path: Some(temporary.clone()),
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("cannot create temporary sibling: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("cannot write temporary sibling: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("cannot flush temporary sibling: {error}"))?;
    drop(file);
    fs::rename(&temporary, destination)
        .map_err(|error| format!("cannot replace destination atomically: {error}"))?;
    pending.disarm();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{atomic_write, run, wasm_compile_config};
    use std::fs;

    use subset_julia_vm::aot::{compile_wasm_source, AotBackend, CompileConfig};

    use super::super::{exit_code, Args, Backend};

    #[test]
    fn atomic_write_replaces_existing_output() {
        // Given: a stale artifact at the requested destination.
        let dir = tempfile::tempdir().expect("create output directory");
        let output = dir.path().join("module.wasm");
        fs::write(&output, b"stale").expect("write stale artifact");

        // When: a complete Wasm payload is persisted.
        atomic_write(&output, b"\0asmfresh").expect("replace stale artifact");

        // Then: replacement is exact and no temporary sibling remains.
        assert_eq!(fs::read(&output).expect("read artifact"), b"\0asmfresh");
        assert_eq!(fs::read_dir(dir.path()).expect("read directory").count(), 1);
    }

    #[test]
    fn atomic_write_preserves_existing_output_on_error() {
        // Given: a stale artifact and a destination that cannot be renamed over a directory.
        let dir = tempfile::tempdir().expect("create output directory");
        let stale = dir.path().join("stale.wasm");
        let destination = dir.path().join("destination");
        fs::write(&stale, b"stale").expect("write stale artifact");
        fs::create_dir(&destination).expect("create invalid destination");

        // When: atomic replacement fails at rename.
        let error = atomic_write(&destination, b"\0asm").expect_err("directory replacement fails");

        // Then: the stale artifact survives and the temporary sibling is cleaned.
        assert!(error.contains("cannot replace destination atomically"));
        assert_eq!(fs::read(&stale).expect("read stale artifact"), b"stale");
        assert_eq!(fs::read_dir(dir.path()).expect("read directory").count(), 2);
    }

    #[test]
    fn cli_backend_bytes_match_direct_source_compilation() {
        // Given: one typed CLI request with explicit source identity, optimization, and export.
        let dir = tempfile::tempdir().expect("create parity directory");
        let output = dir.path().join("module.wasm");
        let source = "answer()::Int64 = 42";
        let output_arg = output.to_string_lossy().into_owned();
        let args = Args::parse_from([
            "juliars",
            "-e",
            source,
            "--backend=wasm",
            "--emit-wasm",
            &output_arg,
            "--export-c-abi=answer=answer",
            "-O3",
        ])
        .expect("parse parity request");
        assert_eq!(args.backend, Backend::Wasm);

        // When: the CLI backend and public library compile the identical request.
        assert_eq!(run(&args), exit_code::SUCCESS);
        let direct_config: CompileConfig = wasm_compile_config(&args, "<eval>");
        assert_eq!(direct_config.backend, AotBackend::Wasm);
        let direct = compile_wasm_source(source, &direct_config).expect("compile direct Wasm");

        // Then: the persisted artifact is byte-for-byte the canonical library result.
        assert_eq!(fs::read(output).expect("read CLI Wasm"), direct.wasm_bytes);
    }

    #[test]
    fn compiler_diagnostics_preserve_existing_output() {
        // Given: stale output and source failures from parse, lowering, and Wasm codegen.
        let cases = [
            ("x = (", None, exit_code::PARSE),
            ("1 = 2", None, exit_code::PARSE),
            (
                "answer()::String = \"no Wasm strings\"",
                Some("answer=answer"),
                exit_code::UNSUPPORTED,
            ),
        ];

        for (index, (source, export, expected_code)) in cases.into_iter().enumerate() {
            let dir = tempfile::tempdir().expect("create diagnostic directory");
            let output = dir.path().join(format!("module-{index}.wasm"));
            fs::write(&output, b"stale").expect("write stale artifact");
            let output_arg = output.to_string_lossy().into_owned();
            let mut argv = vec![
                "juliars".to_string(),
                "-e".to_string(),
                source.to_string(),
                "--backend=wasm".to_string(),
                "--emit-wasm".to_string(),
                output_arg,
            ];
            if let Some(export) = export {
                argv.push(format!("--export-c-abi={export}"));
            }
            let args = Args::parse_from(argv).expect("parse diagnostic request");

            // When: canonical source compilation fails before persistence.
            let code = run(&args);

            // Then: the typed exit classification is returned and stale bytes survive exactly.
            assert_eq!(code, expected_code, "unexpected exit code for `{source}`");
            assert_eq!(fs::read(output).expect("read stale artifact"), b"stale");
        }
    }
}
