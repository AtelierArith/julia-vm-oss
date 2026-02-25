use super::*;

pub(super) fn run_file(file_path: &str) {
    const SEED: u64 = 42;

    // Check if file exists
    if !Path::new(file_path).exists() {
        eprintln!("Error: File '{}' not found", file_path);
        std::process::exit(1);
    }

    let source = fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("Error reading file '{}': {}", file_path, e);
        std::process::exit(1);
    });

    // Parse using tree-sitter
    let mut parser = Parser::new().unwrap_or_else(|e| {
        eprintln!("Error: failed to create parser: {}", e);
        std::process::exit(1);
    });

    // Parse and lower prelude (base functions)
    let prelude_src = base::get_prelude();
    let prelude_outcome = parser.parse(&prelude_src).unwrap_or_else(|e| {
        eprintln!("Error: failed to parse prelude: {:?}", e);
        std::process::exit(1);
    });
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering.lower(prelude_outcome).unwrap_or_else(|e| {
        eprintln!("Prelude lowering error: {:?}", e);
        std::process::exit(1);
    });

    // Parse user source
    let outcome = parser.parse(&source).unwrap_or_else(|e| {
        eprintln!("Error: failed to parse source: {:?}", e);
        std::process::exit(1);
    });

    // Lower to Core IR
    let mut lowering = Lowering::new(&source);
    let mut program = lowering.lower(outcome).unwrap_or_else(|e| {
        eprintln!("Lowering error: {:?}", e);
        std::process::exit(1);
    });

    // Merge prelude with user program
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    // Merge structs (prelude first, skip if user defines same name)
    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.append(&mut program.structs);
    program.structs = all_structs;

    // Merge abstract types (prelude first, skip if user defines same name)
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

    // Merge functions (prelude first, skip if user defines same name)
    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_func_names.contains(f.name.as_str()))
        .collect();
    // Track base function count BEFORE adding user functions
    let base_function_count = all_functions.len();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;
    program.base_function_count = base_function_count;

    // Merge main blocks: prelude main block first (defines globals like RoundNearest, etc.)
    // then user program main block follows.
    // This ensures prelude const definitions are available to all functions.
    let mut merged_main_stmts = prelude_program.main.stmts;
    merged_main_stmts.extend(program.main.stmts);
    program.main = subset_julia_vm::ir::core::Block {
        stmts: merged_main_stmts,
        span: program.main.span,
    };

    let existing_modules: HashSet<String> =
        program.modules.iter().map(|m| m.name.clone()).collect();
    // Skip relative imports (using .Module) - they refer to user-defined modules
    // already in program.modules, not external packages
    let usings_to_load: Vec<subset_julia_vm::ir::core::UsingImport> = program
        .usings
        .iter()
        .filter(|u| !u.is_relative && !existing_modules.contains(&u.module))
        .cloned()
        .collect();

    if !usings_to_load.is_empty() {
        let mut package_loader = loader::PackageLoader::new(loader::LoaderConfig::from_env());
        let loaded_modules = package_loader
            .load_for_usings(&usings_to_load)
            .unwrap_or_else(|e| {
                eprintln!("Load error: {}", e);
                std::process::exit(1);
            });

        for module in loaded_modules {
            if !existing_modules.contains(&module.name) {
                program.modules.push(module);
            }
        }
    }

    // Compile to bytecode
    let compiled = compile_core_program(&program).unwrap_or_else(|e| {
        eprintln!("Compilation error: {:?}", e);
        std::process::exit(1);
    });

    // Run in VM
    let mut vm = Vm::new_program(compiled, StableRng::new(SEED));

    match vm.run() {
        Ok(Value::I64(x)) => println!("result i64 = {}", x),
        Ok(Value::F64(x)) => println!("result f64 = {:.17}", x),
        // New numeric types
        Ok(Value::I8(x)) => println!("result i8 = {}", x),
        Ok(Value::I16(x)) => println!("result i16 = {}", x),
        Ok(Value::I32(x)) => println!("result i32 = {}", x),
        Ok(Value::I128(x)) => println!("result i128 = {}", x),
        Ok(Value::U8(x)) => println!("result u8 = {}", x),
        Ok(Value::U16(x)) => println!("result u16 = {}", x),
        Ok(Value::U32(x)) => println!("result u32 = {}", x),
        Ok(Value::U64(x)) => println!("result u64 = {}", x),
        Ok(Value::U128(x)) => println!("result u128 = {}", x),
        Ok(Value::F16(x)) => println!("result f16 = {}", x),
        Ok(Value::F32(x)) => println!("result f32 = {}", x),
        Ok(Value::Str(s)) => println!("result str = {}", s),
        Ok(Value::Nothing) => println!("result nothing"),
        Ok(Value::Missing) => println!("result missing"),
        Ok(Value::Array(arr)) => println!("result array = {:?}", arr.borrow()),
        Ok(ref val @ Value::Struct(_)) if val.is_complex() => {
            if let Some((re, im)) = val.as_complex_parts() {
                println!("result complex = {} + {}im", re, im);
            }
        }
        Ok(Value::Struct(_)) => println!("result struct"),
        Ok(Value::StructRef(_)) => println!("result struct_ref"),
        Ok(Value::SliceAll) => println!("result slice_all"),
        Ok(Value::Rng(_)) => println!("result rng"),
        Ok(Value::Tuple(t)) => println!("result tuple = {:?}", t.elements),
        Ok(Value::NamedTuple(nt)) => println!("result named_tuple = {:?}", nt.names),
        Ok(Value::Dict(d)) => println!("result dict = {} pairs", d.len()),
        Ok(Value::Range(r)) => {
            if r.is_float {
                if r.is_unit_range() {
                    println!("result range = {}:{}", format_range_float(r.start), format_range_float(r.stop));
                } else {
                    println!("result range = {}:{}:{}", format_range_float(r.start), format_range_float(r.step), format_range_float(r.stop));
                }
            } else if r.is_unit_range() {
                println!("result range = {:.0}:{:.0}", r.start, r.stop);
            } else {
                println!("result range = {:.0}:{:.0}:{:.0}", r.start, r.step, r.stop);
            }
        }
        Ok(Value::Ref(inner)) => println!("result ref = {:?}", inner),
        Ok(Value::Char(c)) => println!("result char = '{}'", c),
        Ok(Value::Generator(_)) => println!("result generator"),
        Ok(Value::DataType(jt)) => println!("result datatype = {}", jt),
        Ok(Value::Module(m)) => println!("result module = {}", m.name),
        Ok(Value::Function(f)) => println!("result function = {}", f.name),
        Ok(Value::BigInt(b)) => println!("result bigint = {}", b),
        Ok(Value::BigFloat(b)) => println!("result bigfloat = {}", b),
        Ok(Value::IO(_)) => println!("result io"),
        Ok(Value::Undef) => println!("result #undef"),
        Ok(Value::Bool(b)) => println!("result bool = {}", b),
        Ok(Value::Symbol(s)) => println!("result symbol = :{}", s.as_str()),
        Ok(Value::Expr(e)) => println!("result expr = Expr(:{}, ...)", e.head.as_str()),
        Ok(Value::QuoteNode(_)) => println!("result quotenode"),
        Ok(Value::LineNumberNode(ln)) => println!("result linenumber = {}", ln.line),
        Ok(Value::GlobalRef(gr)) => {
            println!("result globalref = {}:{}", gr.module, gr.name.as_str())
        }
        Ok(Value::ComposedFunction(cf)) => {
            println!("result composed function = {:?} ∘ {:?}", cf.outer, cf.inner)
        }
        Ok(Value::Pairs(p)) => println!("result pairs = {} pairs", p.data.names.len()),
        Ok(Value::Set(s)) => println!("result set = {} elements", s.elements.len()),
        Ok(Value::Regex(r)) => println!("result regex = r\"{}\"", r.pattern),
        Ok(Value::RegexMatch(m)) => println!("result regexmatch = RegexMatch(\"{}\")", m.match_str),
        Ok(Value::Enum { type_name, value }) => println!("result enum = {}({})", type_name, value),
        Ok(Value::Closure(c)) => println!("result closure = {}", c.name),
        Ok(Value::Memory(mem)) => {
            let mem = mem.borrow();
            let type_name = mem.element_type().julia_type_name();
            println!(
                "result memory = {}-element Memory{{{}}}",
                mem.len(),
                type_name
            );
        }
        Err(e) => {
            eprintln!("Runtime error: {}", e);
            std::process::exit(1);
        }
    }

    // Print output (if any)
    let output = vm.get_output();
    if !output.is_empty() {
        print!("{}", output);
    }
}

pub(super) fn run_code(source: &str) {
    const SEED: u64 = 42;

    // Parse using tree-sitter
    let mut parser = Parser::new().unwrap_or_else(|e| {
        eprintln!("Error: failed to create parser: {}", e);
        std::process::exit(1);
    });

    // Parse and lower prelude (base functions)
    let prelude_src = base::get_prelude();
    let prelude_outcome = parser.parse(&prelude_src).unwrap_or_else(|e| {
        eprintln!("Error: failed to parse prelude: {:?}", e);
        std::process::exit(1);
    });
    let mut prelude_lowering = Lowering::new(&prelude_src);
    let prelude_program = prelude_lowering.lower(prelude_outcome).unwrap_or_else(|e| {
        eprintln!("Prelude lowering error: {:?}", e);
        std::process::exit(1);
    });

    // Parse user source
    let outcome = parser.parse(source).unwrap_or_else(|e| {
        eprintln!("Error: failed to parse source: {:?}", e);
        std::process::exit(1);
    });

    // Lower to Core IR
    let mut lowering = Lowering::new(source);
    let mut program = lowering.lower(outcome).unwrap_or_else(|e| {
        eprintln!("Lowering error: {:?}", e);
        std::process::exit(1);
    });

    // Merge prelude with user program
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
    let user_struct_names: HashSet<_> = program.structs.iter().map(|s| s.name.as_str()).collect();

    // Merge structs (prelude first, skip if user defines same name)
    let mut all_structs: Vec<_> = prelude_program
        .structs
        .into_iter()
        .filter(|s| !user_struct_names.contains(s.name.as_str()))
        .collect();
    all_structs.append(&mut program.structs);
    program.structs = all_structs;

    // Merge abstract types (prelude first, skip if user defines same name)
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

    // Merge functions (prelude first, skip if user defines same name)
    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_func_names.contains(f.name.as_str()))
        .collect();
    // Track base function count BEFORE adding user functions
    let base_function_count = all_functions.len();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;
    program.base_function_count = base_function_count;

    // Merge main blocks: prelude main block first (defines globals like RoundNearest, etc.)
    // then user program main block follows.
    // This ensures prelude const definitions are available to all functions.
    let mut merged_main_stmts = prelude_program.main.stmts;
    merged_main_stmts.extend(program.main.stmts);
    program.main = subset_julia_vm::ir::core::Block {
        stmts: merged_main_stmts,
        span: program.main.span,
    };

    let existing_modules: HashSet<String> =
        program.modules.iter().map(|m| m.name.clone()).collect();
    // Skip relative imports (using .Module) - they refer to user-defined modules
    // already in program.modules, not external packages
    let usings_to_load: Vec<subset_julia_vm::ir::core::UsingImport> = program
        .usings
        .iter()
        .filter(|u| !u.is_relative && !existing_modules.contains(&u.module))
        .cloned()
        .collect();

    if !usings_to_load.is_empty() {
        let mut package_loader = loader::PackageLoader::new(loader::LoaderConfig::from_env());
        let loaded_modules = package_loader
            .load_for_usings(&usings_to_load)
            .unwrap_or_else(|e| {
                eprintln!("Load error: {}", e);
                std::process::exit(1);
            });

        for module in loaded_modules {
            if !existing_modules.contains(&module.name) {
                program.modules.push(module);
            }
        }
    }

    // Compile to bytecode
    let compiled = compile_core_program(&program).unwrap_or_else(|e| {
        eprintln!("Compilation error: {:?}", e);
        std::process::exit(1);
    });

    // Run in VM
    let mut vm = Vm::new_program(compiled, StableRng::new(SEED));

    match vm.run() {
        Ok(value) => {
            // Print output first (if any)
            let output = vm.get_output();
            if !output.is_empty() {
                print!("{}", output);
            }
            // Print result value
            println!("{}", format_value(&value));
        }
        Err(e) => {
            eprintln!("Runtime error: {}", e);
            std::process::exit(1);
        }
    }
}

pub(super) fn run_repl() {
    const SEED: u64 = 42;

    print_logo();
    println!("  SubsetJuliaVM v{} - Julia Subset REPL", VERSION);
    println!("  Type \"?\" for help, \"exit()\" to exit.\n");

    let mut session = REPLSession::new(SEED);

    let config = Config::builder().bracketed_paste(true).build();

    let helper = JuliaHelper::new();
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
                        session.reset();
                        println!("Session reset.\n");
                        continue;
                    }
                    "vars()" | "whos()" => {
                        print_variables(&session);
                        continue;
                    }
                    _ => {}
                }

                let _ = rl.add_history_entry(&input);

                if let Some(exprs) = session.split_expressions(&input) {
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
                            let result = session.eval(expr_text);

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
                            let result = session.eval(expr_text);
                            print_result_with_context(&result, Some(expr_text), &session);

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
                    let result = session.eval(&input);
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
