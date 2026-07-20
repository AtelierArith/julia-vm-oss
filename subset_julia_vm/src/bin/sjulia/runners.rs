use super::*;

fn vm_profile_enabled() -> bool {
    env::var("SJULIA_VM_PROFILE")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(false)
}

fn begin_vm_profile_if_requested() -> bool {
    if vm_profile_enabled() {
        subset_julia_vm::vm::profiler::clear();
        subset_julia_vm::vm::profiler::enable();
        true
    } else {
        false
    }
}

fn finish_vm_profile_if_enabled(enabled: bool) {
    if enabled {
        subset_julia_vm::vm::profiler::disable();
        subset_julia_vm::vm::profiler::print_results();
    }
}

fn parse_cli_program(
    source: &str,
    base_dir: Option<std::path::PathBuf>,
    script_path: Option<&str>,
) -> subset_julia_vm::ir::core::Program {
    // Deserialize the Base cache on this thread NOW, while the background
    // prefetch thread (spawned at the top of main) is still loading the
    // prelude Program that parse_and_lower blocks on — the two largest
    // warm-start deserializes overlap instead of running serially
    // (Issue #6348).
    subset_julia_vm::compile::host_support::warm_base_cache();

    // The CLI script path (`sjulia file.jl` / `-e` / piped stdin) is
    // non-interactive execution, so it uses strict file-mode soft scope
    // (Issue #9210): a top-level loop-body assignment to an existing global
    // binds a new local, matching `julia file.jl` / `julia -e`. The interactive
    // REPL keeps lenient soft scope (it lowers through `Lowering`, not here).
    // `script_path` (absolute, for `sjulia file.jl`) renders the soft-scope
    // warning location `└ @ /abs/path:<line>`; `-e` / stdin pass `None` so it
    // reads `└ @ none:<line>`, matching `julia -e` (Issue #9283).
    subset_julia_vm::pipeline::parse_and_lower_with_base_dir_mode(
        source,
        base_dir,
        subset_julia_vm::pipeline::SoftScopeMode::Strict,
        script_path,
    )
    .unwrap_or_else(|e| {
        eprintln!("Pipeline error: {e}");
        std::process::exit(1);
    })
}

fn flush_vm_output<R: subset_julia_vm::rng::RngLike>(vm: &Vm<R>) {
    let output = vm.get_output();
    if !output.is_empty() {
        print!("{output}");
    }
    let stderr_output = vm.get_stderr_output();
    if !stderr_output.is_empty() {
        eprint!("{stderr_output}");
    }
}

fn compile_and_run_program(program: subset_julia_vm::ir::core::Program) {
    let compiled = subset_julia_vm::compile::host_support::compile_with_cache(&program)
        .unwrap_or_else(|e| {
            eprintln!("Compilation error: {e:?}");
            std::process::exit(1);
        });
    // Opt-in inference budget metrics report (Issue #8546): inference is done
    // once compilation succeeds, and the success path below leaves via
    // `fast_exit` without returning to `main`, so report here.
    subset_julia_vm::compile::budget_metrics::report_to_stderr_if_enabled();
    run_compiled_program(compiled);
}

fn print_runtime_error_with_trace<R: subset_julia_vm::rng::RngLike>(
    vm: &Vm<R>,
    error: subset_julia_vm::vm::VmError,
) {
    eprintln!("Runtime error: {}", vm.spanned_error(error));
    let frames = vm.runtime_stack_trace();
    if frames.is_empty() {
        return;
    }
    eprintln!("Stacktrace:");
    for (idx, frame) in frames.iter().enumerate() {
        eprintln!(" [{}] {}()", idx + 1, frame.function);
        if let Some(span) = frame.span {
            eprintln!("   @ Main ./none:{}", span.start_line);
        }
    }
}

fn run_compiled_program(compiled: CompiledProgram) {
    const SEED: u64 = 42;

    let profile_run = std::env::var_os("SJULIA_COMPILE_PROFILE").is_some();
    let vm_start = std::time::Instant::now();
    let mut vm = Vm::new_program(compiled, StableRng::new(SEED));
    if profile_run {
        eprintln!(
            "[CompileProfile]   cli.vm_new                              {:>9.3} ms (immediate)",
            vm_start.elapsed().as_secs_f64() * 1000.0
        );
    }
    let vm_profile = begin_vm_profile_if_requested();
    let run_start = std::time::Instant::now();

    match vm.run() {
        Ok(_) => {
            if profile_run {
                eprintln!(
                    "[CompileProfile]   cli.vm_run                              {:>9.3} ms (immediate)",
                    run_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            // Match official Julia: scripts and `julia -e <code>` keep trailing
            // values silent; only explicit prints reach stdout.
            flush_vm_output(&vm);
            finish_vm_profile_if_enabled(vm_profile);
            // Match upstream Julia's exit code: a failing top-level `@testset`
            // (or any failing `@test`) makes the process exit non-zero, so CI /
            // `run.jl`-style scripts detect test failures (Issue #8191). sjulia
            // records failures without throwing, so check the sticky flag here.
            let exit_code = if vm.any_test_failed() { 1 } else { 0 };
            // One-shot CLI runs exit here anyway; dropping the VM walks the
            // multi-MB CompiledProgram + struct heap and costs several ms of
            // warm-start latency. Flush std streams and exit without running
            // destructors (Issue #6348).
            fast_exit(exit_code);
        }
        Err(e) => {
            flush_vm_output(&vm);
            print_runtime_error_with_trace(&vm, e);
            finish_vm_profile_if_enabled(vm_profile);
            std::process::exit(1);
        }
    }
}

/// Flush stdout/stderr and terminate without running destructors.
///
/// `std::process::exit` does not flush Rust's buffered stdout, so flush
/// explicitly first. Skipping destructors avoids paying the deallocation walk
/// of the ~14MB `CompiledProgram`/VM state on every one-shot script run
/// (Issue #6348).
fn fast_exit(code: i32) -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code);
}

pub(super) fn run_file(file_path: &str) {
    // Check if file exists
    if !Path::new(file_path).exists() {
        eprintln!("Error: File '{}' not found", file_path);
        std::process::exit(1);
    }

    let source = fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("Error reading file '{}': {}", file_path, e);
        std::process::exit(1);
    });

    let base_dir = Path::new(file_path).parent().map(Path::to_path_buf);
    // Absolute path for the soft-scope warning location, matching upstream
    // `julia file.jl` which reports `abspath(file)` (Issue #9283). `std::path::
    // absolute` normalizes against the physical cwd (like Julia's `abspath`)
    // without resolving symlinks; fall back to the raw argument if it fails.
    let abs_path = std::path::absolute(file_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file_path.to_string());
    let program = parse_cli_program(&source, base_dir, Some(&abs_path));
    compile_and_run_program(program);
}

pub(super) fn run_ir_file(file_path: &str) {
    if !Path::new(file_path).exists() {
        eprintln!("Error: Core IR file '{}' not found", file_path);
        std::process::exit(1);
    }

    let program = core_ir_file::load(file_path).unwrap_or_else(|e| {
        eprintln!("Error loading Core IR file '{}': {}", file_path, e);
        std::process::exit(1);
    });
    compile_and_run_program(program);
}

pub(super) fn run_vm_bytecode_file(file_path: &str) {
    if !Path::new(file_path).exists() {
        eprintln!("Error: VM bytecode file '{}' not found", file_path);
        std::process::exit(1);
    }

    let compiled = vm_bytecode_file::load(file_path).unwrap_or_else(|e| {
        eprintln!("Error loading VM bytecode file '{}': {}", file_path, e);
        std::process::exit(1);
    });
    run_compiled_program(compiled);
}

pub(super) fn run_code(source: &str) {
    // `-e '<code>'` has no backing file, so the soft-scope warning locates at
    // `none:<line>` (upstream `julia -e` does the same); pass no script path.
    let program = parse_cli_program(source, None, None);
    compile_and_run_program(program);
}

pub(super) fn run_repl() {
    const SEED: u64 = 42;

    print_logo();
    println!("  SubsetJuliaVM v{} - Julia Subset REPL", VERSION);
    println!("  Type \"?\" for help, \"exit()\" to exit.\n");

    let session = Rc::new(RefCell::new(REPLSession::new(SEED)));

    let config = Config::builder().bracketed_paste(true).build();

    let helper = JuliaHelper::new(Rc::clone(&session));
    let mut rl: Editor<JuliaHelper, DefaultHistory> =
        Editor::with_config(config).unwrap_or_else(|e| {
            eprintln!("Error: failed to create REPL editor: {}", e);
            std::process::exit(1);
        });
    rl.set_helper(Some(helper));

    let history_path = dirs_path().map(|p| p.join("history.txt"));
    if let Some(ref path) = history_path {
        let _ = rl.load_history(path);
    }

    loop {
        let prompt = "julia> ";

        match rl.readline(prompt) {
            Ok(input) => {
                let trimmed = input.trim();

                match trimmed {
                    "" => continue,
                    "exit()" | "quit()" => break,
                    "?" | "help" | "help()" => {
                        print_help();
                        continue;
                    }
                    "reset()" => {
                        session.borrow_mut().reset();
                        println!("Session reset.\n");
                        continue;
                    }
                    "vars()" | "whos()" => {
                        let session = session.borrow();
                        print_variables(&session);
                        continue;
                    }
                    _ => {}
                }

                let _ = rl.add_history_entry(&input);

                let split_exprs = {
                    let session = session.borrow();
                    session.split_expressions(&input)
                };
                if let Some(exprs) = split_exprs {
                    let is_single_line = input.lines().count() == 1;
                    let suppress_final = input.trim_end().ends_with(';');

                    if is_single_line {
                        let highlighter = JuliaHighlighter;
                        let highlighted = highlighter.highlight_line(input.trim());
                        print!("\x1b[A\x1b[2K\r");
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                        println!("{}julia>{} {}", colors::PROMPT, colors::RESET, highlighted);

                        let mut last_result = None;
                        let mut all_output = String::new();
                        let mut had_error = false;

                        for (_, _, expr_text) in exprs.iter() {
                            let result = session.borrow_mut().eval(expr_text);

                            if !result.output.is_empty() {
                                all_output.push_str(&result.output);
                                if !result.output.ends_with('\n') {
                                    all_output.push('\n');
                                }
                            }

                            if !result.success {
                                if let Some(ref error) = result.error {
                                    eprintln!(
                                        "{}ERROR:{} {}",
                                        colors::KEYWORD,
                                        colors::RESET,
                                        error
                                    );
                                }
                                had_error = true;
                                break;
                            }

                            last_result = Some((result, expr_text.clone()));
                        }

                        if !all_output.is_empty() {
                            print!("{}", all_output);
                        }

                        if !had_error && !suppress_final {
                            if let Some((result, expr_text)) = last_result {
                                if let Some(func_name) = extract_function_name(&expr_text) {
                                    println!("{} (generic function with 1 method)", func_name);
                                } else if let Some(struct_name) = extract_struct_name(&expr_text) {
                                    println!("{}", struct_name);
                                } else if let Some(ref value) = result.value {
                                    let session = session.borrow();
                                    println!(
                                        "{}",
                                        format_value_with_vm(
                                            value,
                                            Some(session.get_struct_heap())
                                        )
                                    );
                                }
                            }
                        }
                        println!();
                    } else {
                        let line_count = input.lines().count();

                        for _ in 0..line_count {
                            print!("\x1b[A\x1b[2K\r");
                        }
                        use std::io::Write;
                        let _ = std::io::stdout().flush();

                        let highlighter = JuliaHighlighter;
                        let first_expr = &exprs[0].2;
                        for (line_idx, line) in first_expr.lines().enumerate() {
                            let highlighted = highlighter.highlight_line(line);
                            if line_idx == 0 {
                                println!(
                                    "{}julia>{} {}",
                                    colors::PROMPT,
                                    colors::RESET,
                                    highlighted
                                );
                            } else {
                                println!("       {}", highlighted);
                            }
                        }

                        for (i, (_, _, expr_text)) in exprs.iter().enumerate() {
                            let result = session.borrow_mut().eval(expr_text);
                            let session_ref = session.borrow();
                            print_result_with_context(&result, Some(expr_text), &session_ref);

                            if !result.success {
                                break;
                            }

                            if i + 1 < exprs.len() {
                                let next_expr = &exprs[i + 1].2;
                                let highlighted = highlighter.highlight_line(next_expr.trim());
                                println!(
                                    "{}julia>{} {}",
                                    colors::PROMPT,
                                    colors::RESET,
                                    highlighted
                                );
                            }
                        }
                    }
                } else {
                    let result = session.borrow_mut().eval(&input);
                    if input.trim_end().ends_with(';') {
                        if !result.output.is_empty() {
                            print!("{}", result.output);
                            if !result.output.ends_with('\n') {
                                println!();
                            }
                        }
                        if !result.success {
                            if let Some(ref error) = result.error {
                                eprintln!("{}ERROR:{} {}", colors::KEYWORD, colors::RESET, error);
                            }
                        }
                        println!();
                    } else {
                        let session = session.borrow();
                        print_result_with_context(&result, Some(&input), &session);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    if let Some(ref path) = history_path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = rl.save_history(path);
    }
}
