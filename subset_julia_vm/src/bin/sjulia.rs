#![deny(clippy::expect_used)]
//! SubsetJuliaVM Command-Line Interface
//!
//! Usage:
//!   sjulia                          # Start interactive REPL
//!   sjulia file.jl                  # Execute Julia file
//!   sjulia -e "code"                # Execute code string
//!   sjulia --compile file.jl -o out # Compile to Core IR file (.sjir)
//!   sjulia --run-ir file.sjir # Execute Core IR file
//!   sjulia --compile-vm file.jl -o out # Compile to VM bytecode file (.sjvmbc)
//!   sjulia --run-vm-bytecode file.sjvmbc # Execute VM bytecode file
//!   sjulia --dump-bytecode file.jl  # Dump compiled VM bytecode

use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

use std::cell::RefCell;
use std::collections::HashSet;
use subset_julia_vm::base;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::loader;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::repl::completions::{
    complete as complete_repl, CompletionContext, CompletionItem,
};
use subset_julia_vm::repl::REPLSession;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{CompiledProgram, FunctionInfo, Instr, Value, VarTypeTag, Vm};
use subset_julia_vm::{core_ir_file, vm_bytecode_file};

// Import REPL dependencies
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Config, Context, Editor, Helper};
use std::borrow::Cow;
use std::rc::Rc;

#[path = "sjulia/runners.rs"]
mod runners;

use runners::{run_code, run_file, run_ir_file, run_repl, run_vm_bytecode_file};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_MAIN_BYTECODE_TAIL: usize = 40;

fn get_method_signature(func: &subset_julia_vm::ir::core::Function) -> String {
    let param_types: Vec<String> = func
        .params
        .iter()
        .map(|p| p.effective_type().to_string())
        .collect();
    format!("{}({})", func.name, param_types.join(", "))
}

// ANSI color codes for Monokai theme
mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const KEYWORD: &str = "\x1b[38;2;249;38;114m"; // #F92672 (pink)
    pub const STRING: &str = "\x1b[38;2;230;219;116m"; // #E6DB74 (yellow)
    pub const NUMBER: &str = "\x1b[38;2;174;129;255m"; // #AE81FF (purple)
    pub const COMMENT: &str = "\x1b[38;2;117;113;94m"; // #75715E (gray)
    pub const FUNCTION: &str = "\x1b[38;2;166;226;46m"; // #A6E22E (green)
    pub const FUNC_CALL: &str = "\x1b[38;2;102;217;239m"; // #66D9EF (cyan)
    pub const MACRO: &str = "\x1b[38;2;253;151;31m"; // #FD971F (orange)
    pub const OPERATOR: &str = "\x1b[38;2;249;38;114m"; // #F92672 (pink)
    pub const BOOL: &str = "\x1b[38;2;174;129;255m"; // #AE81FF (purple)
    pub const PROMPT: &str = "\x1b[32m"; // Green
    pub const HINT: &str = "\x1b[90m"; // Light black / gray (matches Julia upstream :light_black)
}

const KEYWORDS: &[&str] = &[
    "abstract",
    "baremodule",
    "begin",
    "break",
    "catch",
    "const",
    "continue",
    "do",
    "else",
    "elseif",
    "end",
    "export",
    "finally",
    "for",
    "function",
    "global",
    "if",
    "import",
    "let",
    "local",
    "macro",
    "module",
    "mutable",
    "primitive",
    "quote",
    "return",
    "struct",
    "try",
    "using",
    "while",
];

const BOOL_LITERALS: &[&str] = &["true", "false", "nothing"];

/// Julia syntax highlighter for rustyline
struct JuliaHighlighter;

impl JuliaHighlighter {
    fn highlight_line(&self, line: &str) -> String {
        let mut result = String::with_capacity(line.len() * 2);
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Check for block comment #= ... =#
            if i + 1 < len && chars[i] == '#' && chars[i + 1] == '=' {
                result.push_str(colors::COMMENT);
                result.push('#');
                result.push('=');
                i += 2;
                while i + 1 < len && !(chars[i] == '=' && chars[i + 1] == '#') {
                    result.push(chars[i]);
                    i += 1;
                }
                if i + 1 < len {
                    result.push('=');
                    result.push('#');
                    i += 2;
                }
                result.push_str(colors::RESET);
                continue;
            }

            // Check for line comment
            if chars[i] == '#' {
                result.push_str(colors::COMMENT);
                while i < len && chars[i] != '\n' {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push_str(colors::RESET);
                continue;
            }

            // Check for string
            if chars[i] == '"' {
                result.push_str(colors::STRING);
                result.push(chars[i]);
                i += 1;
                // Check for triple-quoted string
                if i + 1 < len && chars[i] == '"' && chars[i + 1] == '"' {
                    result.push(chars[i]);
                    result.push(chars[i + 1]);
                    i += 2;
                    // Find closing """
                    while i + 2 < len
                        && !(chars[i] == '"' && chars[i + 1] == '"' && chars[i + 2] == '"')
                    {
                        if chars[i] == '\\' && i + 1 < len {
                            result.push(chars[i]);
                            result.push(chars[i + 1]);
                            i += 2;
                        } else {
                            result.push(chars[i]);
                            i += 1;
                        }
                    }
                    if i + 2 < len {
                        result.push('"');
                        result.push('"');
                        result.push('"');
                        i += 3;
                    }
                } else {
                    // Regular string
                    while i < len && chars[i] != '"' {
                        if chars[i] == '\\' && i + 1 < len {
                            result.push(chars[i]);
                            result.push(chars[i + 1]);
                            i += 2;
                        } else {
                            result.push(chars[i]);
                            i += 1;
                        }
                    }
                    if i < len {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                result.push_str(colors::RESET);
                continue;
            }

            // Check for macro @xxx
            if chars[i] == '@' {
                result.push_str(colors::MACRO);
                result.push(chars[i]);
                i += 1;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '!')
                {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push_str(colors::RESET);
                continue;
            }

            // Check for number
            if chars[i].is_ascii_digit()
                || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit())
            {
                result.push_str(colors::NUMBER);
                while i < len
                    && (chars[i].is_ascii_digit()
                        || chars[i] == '.'
                        || chars[i] == 'e'
                        || chars[i] == 'E'
                        || chars[i] == '+'
                        || chars[i] == '-'
                        || chars[i] == '_')
                {
                    if (chars[i] == '+' || chars[i] == '-') && i > 0 {
                        let prev = chars[i - 1];
                        if prev != 'e' && prev != 'E' {
                            break;
                        }
                    }
                    result.push(chars[i]);
                    i += 1;
                }
                // Check for 'im' suffix
                if i + 1 < len && chars[i] == 'i' && chars[i + 1] == 'm' {
                    result.push_str("im");
                    i += 2;
                }
                result.push_str(colors::RESET);
                continue;
            }

            // Check for identifier/keyword
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < len
                    && (chars[i].is_alphanumeric()
                        || chars[i] == '_'
                        || chars[i] == '!'
                        || chars[i] == '?')
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();

                let is_func_call = i < len && chars[i] == '(';
                let trimmed = result.trim_end();
                let is_func_def = trimmed.ends_with("function");

                if KEYWORDS.contains(&word.as_str()) {
                    result.push_str(colors::KEYWORD);
                    result.push_str(&word);
                    result.push_str(colors::RESET);
                } else if BOOL_LITERALS.contains(&word.as_str()) {
                    result.push_str(colors::BOOL);
                    result.push_str(&word);
                    result.push_str(colors::RESET);
                } else if is_func_def {
                    result.push_str(colors::FUNCTION);
                    result.push_str(&word);
                    result.push_str(colors::RESET);
                } else if is_func_call {
                    result.push_str(colors::FUNC_CALL);
                    result.push_str(&word);
                    result.push_str(colors::RESET);
                } else {
                    result.push_str(&word);
                }
                continue;
            }

            // Check for operators
            if "+-*/%^<>=!&|".contains(chars[i]) {
                result.push_str(colors::OPERATOR);
                result.push(chars[i]);
                if i + 1 < len {
                    let next = chars[i + 1];
                    if (next == '=' && matches!(chars[i], '=' | '!' | '<' | '>'))
                        || (chars[i] == '&' && next == '&')
                        || (chars[i] == '|' && next == '|')
                        || (chars[i] == '-' && next == '>')
                        || (chars[i] == '.' && "+-*/^".contains(next))
                    {
                        i += 1;
                        result.push(chars[i]);
                    }
                }
                result.push_str(colors::RESET);
                i += 1;
                continue;
            }

            result.push(chars[i]);
            i += 1;
        }

        result
    }
}

impl Highlighter for JuliaHighlighter {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(self.highlight_line(line))
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        if prompt.contains("julia>") {
            Cow::Owned(format!("{}julia>{} ", colors::PROMPT, colors::RESET))
        } else {
            Cow::Borrowed(prompt)
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        true
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("{}{}{}", colors::HINT, hint, colors::RESET))
    }
}

struct JuliaHelper {
    highlighter: JuliaHighlighter,
    session: Rc<RefCell<REPLSession>>,
}

impl JuliaHelper {
    fn new(session: Rc<RefCell<REPLSession>>) -> Self {
        Self {
            highlighter: JuliaHighlighter,
            session,
        }
    }

    fn completion_items(&self, line: &str, pos: usize) -> (usize, Vec<CompletionItem>) {
        let session = self.session.borrow();
        let variable_names = session.variable_names();
        let function_names = session.function_names();
        let imported_module_names = session.imported_module_names();
        let field_names_by_object = session.field_names_by_object();
        let completion_ctx = CompletionContext {
            variable_names: &variable_names,
            function_names: &function_names,
            imported_module_names: &imported_module_names,
            field_names_by_object: &field_names_by_object,
        };
        complete_repl(line, pos, &completion_ctx)
    }
}

impl Helper for JuliaHelper {}

impl Completer for JuliaHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let (start, completions) = self.completion_items(line, pos);
        if !completions.is_empty() {
            let pairs = completions
                .into_iter()
                .map(|item| Pair {
                    display: item.display,
                    replacement: item.text,
                })
                .collect();
            return Ok((start, pairs));
        }

        Ok((
            pos,
            vec![Pair {
                display: "    ".to_string(),
                replacement: "    ".to_string(),
            }],
        ))
    }
}

impl Hinter for JuliaHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if pos != line.len() {
            return None;
        }

        let (start, completions) = self.completion_items(line, pos);
        let [item] = completions.as_slice() else {
            return None;
        };
        let prefix = line.get(start..pos)?;
        let suffix = item.text.strip_prefix(prefix)?;
        if suffix.is_empty() {
            None
        } else {
            Some(suffix.to_string())
        }
    }
}

impl Validator for JuliaHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        if input.trim().is_empty() {
            return Ok(ValidationResult::Valid(None));
        }

        if is_incomplete(input) {
            Ok(ValidationResult::Incomplete)
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }
}

impl Highlighter for JuliaHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        self.highlighter.highlight_prompt(prompt, default)
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        self.highlighter.highlight_char(line, pos, kind)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        self.highlighter.highlight_hint(hint)
    }
}

fn main() {
    // Overlap the Base-cache deserialize and Base IR clones with the
    // prelude-load/merge window on the main thread (Issue #6348). Harmless
    // for non-run subcommands: unconsumed prefetch results are dropped.
    subset_julia_vm::compile::cache::begin_warm_start_prefetch();

    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        // No arguments. Match official Julia behavior: when stdin is a TTY,
        // start the interactive REPL; when stdin is piped/redirected (not a
        // TTY), read the entire stdin as a script and execute it. (Issue #3560)
        if std::io::stdin().is_terminal() {
            run_repl();
        } else {
            run_stdin_script();
        }
    } else if args[1] == "-" {
        // Explicit `-` argument: always read script from stdin (Julia parity).
        run_stdin_script();
    } else if args[1] == "-e" {
        // -e option: execute code string
        if args.len() < 3 {
            eprintln!("Error: -e requires an argument");
            std::process::exit(1);
        }
        let code = &args[2];
        run_code(code);
    } else if args[1] == "--compile" || args[1] == "-c" {
        // --compile option: compile to Core IR file
        if args.len() < 3 {
            eprintln!("Error: --compile requires an input file");
            std::process::exit(1);
        }
        let input_file = &args[2];

        // Look for -o option
        let output_file = if args.len() >= 5 && (args[3] == "-o" || args[3] == "--output") {
            args[4].clone()
        } else {
            // Default output: input stem + .sjir
            let path = Path::new(input_file);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            format!("{}.sjir", stem)
        };

        compile_to_ir(input_file, &output_file);
    } else if args[1] == "--compile-vm" {
        if args.len() < 3 {
            eprintln!("Error: --compile-vm requires an input file");
            std::process::exit(1);
        }
        let input_file = &args[2];

        let output_file = if args.len() >= 5 && (args[3] == "-o" || args[3] == "--output") {
            args[4].clone()
        } else {
            let path = Path::new(input_file);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            format!("{}.sjvmbc", stem)
        };

        compile_to_vm_bytecode(input_file, &output_file);
    } else if args[1] == "--run-ir" {
        if args.len() < 3 {
            eprintln!("Error: --run-ir requires a Core IR file");
            std::process::exit(1);
        }
        run_ir_file(&args[2]);
    } else if args[1] == "--run-vm-bytecode" {
        if args.len() < 3 {
            eprintln!("Error: --run-vm-bytecode requires a VM bytecode file");
            std::process::exit(1);
        }
        run_vm_bytecode_file(&args[2]);
    } else if args[1] == "--type-stability" || args[1] == "-t" {
        // --type-stability option: analyze type stability
        // Check for additional flags
        let strict_mode = args.iter().any(|a| a == "--strict");
        let json_output = args.iter().any(|a| a == "--json");

        // Find the input file (first argument that's not a flag)
        let input_file = args
            .iter()
            .skip(2) // Skip program name and --type-stability
            .find(|a| !a.starts_with('-'))
            .cloned();

        let input_file = match input_file {
            Some(f) => f,
            None => {
                eprintln!("Error: --type-stability requires an input file");
                std::process::exit(1);
            }
        };

        run_type_stability_analysis(&input_file, strict_mode, json_output);
    } else if args[1] == "--dump-bytecode" {
        // --dump-bytecode option: compile and print final VM bytecode
        // Supports: --dump-bytecode [--all] <file.jl>
        //           --dump-bytecode [--all] -e <code>
        //           --dump-bytecode [--all] -
        let include_all = args.iter().any(|a| a == "--all");
        let args_filtered: Vec<&String> = args
            .iter()
            .filter(|a| *a != "--dump-bytecode" && *a != "--all")
            .collect();

        if args_filtered.len() < 2 {
            eprintln!("Error: --dump-bytecode requires an input file, '-' or -e 'code'");
            std::process::exit(1);
        }

        if args_filtered[1] == "-e" {
            if args_filtered.len() < 3 {
                eprintln!("Error: -e requires a code argument");
                std::process::exit(1);
            }
            dump_bytecode_for_code(args_filtered[2], include_all);
        } else if args_filtered[1] == "-" {
            let mut source = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut source) {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(1);
            }
            dump_bytecode_for_code(&source, include_all);
        } else {
            dump_bytecode_for_file(args_filtered[1], include_all);
        }
    } else if args[1] == "--dump-ast" {
        // --dump-ast option: show AST structure for debugging parser tests
        // Supports: --dump-ast [--json] <file.jl>
        //           --dump-ast [--json] -e <code>
        let json_output = args.contains(&"--json".to_string());
        let args_filtered: Vec<&String> = args
            .iter()
            .filter(|a| *a != "--dump-ast" && *a != "--json")
            .collect();

        if args_filtered.len() < 2 {
            eprintln!("Error: --dump-ast requires an input file or -e 'code'");
            std::process::exit(1);
        }

        if args_filtered[1] == "-e" {
            // Dump AST for code string
            if args_filtered.len() < 3 {
                eprintln!("Error: -e requires a code argument");
                std::process::exit(1);
            }
            dump_ast_for_code(args_filtered[2], json_output);
        } else {
            // Dump AST for file
            dump_ast_for_file(args_filtered[1], json_output);
        }
    } else if args[1] == "--precompile-base" {
        // --precompile-base: generate Base cache file for embedding
        if args.len() < 3 {
            eprintln!("Error: --precompile-base requires an output file path");
            std::process::exit(1);
        }
        precompile_base(&args[2]);
    } else if args[1] == "--precompile-prelude" {
        // --precompile-prelude: generate parsed/lowered prelude Program cache for embedding
        if args.len() < 3 {
            eprintln!("Error: --precompile-prelude requires an output file path");
            std::process::exit(1);
        }
        precompile_prelude(&args[2]);
    } else if args[1] == "-h" || args[1] == "--help" {
        print_usage();
    } else {
        // File path provided - execute file
        let file_path = &args[1];
        match Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
        {
            Some("sjir") => run_ir_file(file_path),
            Some("sjvmbc") => run_vm_bytecode_file(file_path),
            _ => run_file(file_path),
        }
    }
}

/// Read the entire stdin as a script source and execute it via `run_code`.
///
/// Mirrors official `julia` behavior: when stdin is not a TTY (e.g. a pipe or
/// redirected file) and no script argument is given, treat the piped content
/// as the program to run instead of starting an interactive REPL. See Issue #3560.
fn run_stdin_script() {
    let mut source = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut source) {
        eprintln!("Error reading stdin: {}", e);
        std::process::exit(1);
    }
    run_code(&source);
}

fn print_usage() {
    println!(
        r#"SubsetJuliaVM - Julia Subset Runtime

USAGE:
    sjulia                              Start interactive REPL (or execute piped stdin)
    sjulia -                            Read and execute script from stdin
    sjulia <file.jl>                    Execute Julia file
    sjulia <file.sjir>                  Execute Core IR file
    sjulia <file.sjvmbc>                Execute VM bytecode file
    sjulia -e <code>                    Execute code string
    sjulia --compile <file.jl> -o <out> Compile to Core IR file (.sjir)
    sjulia --run-ir <file.sjir>         Execute Core IR file
    sjulia --compile-vm <file.jl> -o <out> Compile to VM bytecode file (.sjvmbc)
    sjulia --run-vm-bytecode <file.sjvmbc> Execute VM bytecode file
    sjulia --type-stability <file.jl>   Analyze type stability
    sjulia --dump-bytecode <file.jl>    Dump compiled VM bytecode
    sjulia --dump-ast <file.jl>         Dump AST structure for debugging
    sjulia --dump-ast -e <code>         Dump AST for code string
    sjulia --dump-bytecode -e <code>    Dump bytecode for code string
    sjulia --dump-ast --json <file.jl>  Dump AST in JSON format
    sjulia --precompile-base <out.bin> Generate Base cache for embedding
    sjulia --precompile-prelude <out.bin> Generate prelude Program cache for embedding

OPTIONS:
    -e <code>             Execute code string
    -c, --compile <file>  Compile source to Core IR file
        --run-ir          Execute a Core IR file produced by --compile
        --compile-vm      Compile source to VM bytecode file
        --run-vm-bytecode Execute a VM bytecode file produced by --compile-vm
    -o, --output <file>   Output file for --compile or --compile-vm
    -t, --type-stability  Analyze type stability of functions
        --strict          Strict mode (exit code 1 if unstable functions found)
        --json            Output in JSON format (for --type-stability and --dump-ast)
        --dump-ast        Dump AST structure (useful for debugging parser tests)
        --dump-bytecode   Dump compiled VM bytecode (user functions + main by default)
        --all             Include Base/prelude functions in --dump-bytecode output
        --precompile-base Generate Base bytecode cache for build-time embedding
        --precompile-prelude
                          Generate parsed/lowered prelude Program cache for build-time embedding
    -h, --help            Show this help message

EXAMPLES:
    sjulia hello.jl
    sjulia -e "println(1 + 2)"
    sjulia --compile program.jl -o program.sjir
    sjulia --run-ir program.sjir
    sjulia --compile-vm program.jl -o program.sjvmbc
    sjulia --run-vm-bytecode program.sjvmbc
    sjulia --type-stability program.jl
    sjulia --type-stability --strict --json program.jl
    sjulia --dump-ast -e "x = 1 + 2"
    sjulia --dump-ast --json -e "x = 1 + 2"
    sjulia --dump-bytecode -e "f(x)=x+1; f(41)"
"#
    );
}

fn precompile_prelude(output_path: &str) {
    eprintln!("Precompiling prelude Program...");
    let start = std::time::Instant::now();

    let bytes = subset_julia_vm::pipeline::generate_prelude_program_cache().unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });

    std::fs::write(output_path, &bytes).unwrap_or_else(|e| {
        eprintln!("Error: Failed to write prelude cache file: {}", e);
        std::process::exit(1);
    });

    let elapsed = start.elapsed();
    eprintln!(
        "Prelude Program cache written to {} ({} bytes, {:.1}ms)",
        output_path,
        bytes.len(),
        elapsed.as_secs_f64() * 1000.0,
    );
}

fn precompile_base(output_path: &str) {
    eprintln!("Precompiling Base functions...");
    let start = std::time::Instant::now();

    let bytes = subset_julia_vm::compile::precompile::generate_base_cache().unwrap_or_else(|e| {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    });

    std::fs::write(output_path, &bytes).unwrap_or_else(|e| {
        eprintln!("Error: Failed to write cache file: {}", e);
        std::process::exit(1);
    });

    let elapsed = start.elapsed();
    eprintln!(
        "Base cache written to {} ({} bytes, {:.1}ms)",
        output_path,
        bytes.len(),
        elapsed.as_secs_f64() * 1000.0,
    );
}

fn dump_bytecode_for_file(file_path: &str, include_all: bool) {
    if !Path::new(file_path).exists() {
        eprintln!("Error: File '{}' not found", file_path);
        std::process::exit(1);
    }

    let source = fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("Error reading file '{}': {}", file_path, e);
        std::process::exit(1);
    });

    let (compiled, user_function_names) = compile_source_for_bytecode_dump(&source);
    emit_bytecode_dump(&compiled, include_all, file_path, &user_function_names);
}

fn dump_bytecode_for_code(source: &str, include_all: bool) {
    let (compiled, user_function_names) = compile_source_for_bytecode_dump(source);
    emit_bytecode_dump(&compiled, include_all, "<eval>", &user_function_names);
}

fn compile_source_for_bytecode_dump(source: &str) -> (CompiledProgram, HashSet<String>) {
    let mut parser = Parser::new().unwrap_or_else(|e| {
        eprintln!("Error: failed to create parser: {}", e);
        std::process::exit(1);
    });

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

    let outcome = parser.parse(source).unwrap_or_else(|e| {
        eprintln!("Error: failed to parse source: {:?}", e);
        std::process::exit(1);
    });

    let mut lowering = Lowering::new(source);
    let mut program = lowering.lower(outcome).unwrap_or_else(|e| {
        eprintln!("Lowering error: {:?}", e);
        std::process::exit(1);
    });

    let user_function_names = collect_dump_user_function_names(&program);
    let user_method_sigs: HashSet<_> = program.functions.iter().map(get_method_signature).collect();
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
        .collect();
    let base_function_count = all_functions.len();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;
    program.base_function_count = base_function_count;

    let mut merged_main_stmts = prelude_program.main.stmts;
    merged_main_stmts.push(subset_julia_vm::ir::core::Stmt::Meta {
        annotation: subset_julia_vm::ir::core::MetaAnnotation {
            name: subset_julia_vm::ir::core::BASE_USER_MAIN_BOUNDARY_META.to_string(),
            args: Vec::new(),
        },
        span: subset_julia_vm::span::Span::new(0, 0, 0, 0, 0, 0),
    });
    merged_main_stmts.extend(program.main.stmts);
    program.main = subset_julia_vm::ir::core::Block {
        stmts: merged_main_stmts,
        span: program.main.span,
    };

    let existing_modules: HashSet<String> =
        program.modules.iter().map(|m| m.name.clone()).collect();
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

    let compiled = compile_core_program(&program).unwrap_or_else(|e| {
        eprintln!("Compilation error: {:?}", e);
        std::process::exit(1);
    });

    (compiled, user_function_names)
}

fn collect_dump_user_function_names(
    program: &subset_julia_vm::ir::core::Program,
) -> HashSet<String> {
    let mut names = HashSet::new();
    for func in &program.functions {
        names.insert(func.name.clone());
    }
    for module in &program.modules {
        for func in &module.functions {
            names.insert(func.name.clone());
            names.insert(format!("{}.{}", module.name, func.name));
        }
    }
    names
}

fn emit_bytecode_dump(
    compiled: &CompiledProgram,
    include_all: bool,
    source_name: &str,
    user_function_names: &HashSet<String>,
) {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let result = write_bytecode_dump(
        &mut out,
        compiled,
        include_all,
        source_name,
        user_function_names,
    )
    .and_then(|_| out.flush());

    if let Err(e) = result {
        if e.kind() == io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("Error writing bytecode dump: {}", e);
        std::process::exit(1);
    }
}

fn write_bytecode_dump<W: Write>(
    out: &mut W,
    compiled: &CompiledProgram,
    include_all: bool,
    source_name: &str,
    user_function_names: &HashSet<String>,
) -> io::Result<()> {
    writeln!(out, "=== Bytecode Summary ===")?;
    writeln!(out, "source: {}", source_name)?;
    writeln!(out, "instructions: {}", compiled.code.len())?;
    writeln!(out, "entry: {}", compiled.entry)?;
    let default_function_count = compiled
        .functions
        .iter()
        .filter(|func| should_dump_function(false, func, user_function_names))
        .count();
    writeln!(
        out,
        "functions: {} (base/prelude: {}, default shown: {})",
        compiled.functions.len(),
        compiled.base_function_count,
        default_function_count
    )?;
    writeln!(out, "global slots: {}", compiled.global_slot_count)?;
    writeln!(
        out,
        "specializable functions: {}",
        compiled.specializable_functions.len()
    )?;
    if !include_all && compiled.base_function_count > 0 {
        writeln!(
            out,
            "base/prelude and generated helper functions omitted; pass --all to include them"
        )?;
    }
    writeln!(out)?;

    writeln!(out, "=== Functions ===")?;
    let mut dumped_functions = 0usize;
    for (idx, func) in compiled.functions.iter().enumerate() {
        if !should_dump_function(include_all, func, user_function_names) {
            continue;
        }
        write_function_bytecode(out, compiled, idx, func)?;
        dumped_functions += 1;
    }
    if dumped_functions == 0 {
        writeln!(out, "(no user-declared functions)")?;
        writeln!(out)?;
    }

    writeln!(out, "=== Main Bytecode ===")?;
    let main_len = compiled.code.len().saturating_sub(compiled.entry);
    let main_start = if include_all || main_len <= DEFAULT_MAIN_BYTECODE_TAIL {
        compiled.entry
    } else {
        compiled
            .code
            .len()
            .saturating_sub(DEFAULT_MAIN_BYTECODE_TAIL)
    };
    if include_all {
        write_slot_table(
            out,
            &compiled.global_slot_names,
            &compiled.global_slot_types,
            &[],
            &[],
        )?;
    } else {
        writeln!(
            out,
            "  global slots: omitted (slot comments are shown inline)"
        )?;
        if main_start > compiled.entry {
            writeln!(
                out,
                "  showing last {} of {} main instructions; pass --all for full main",
                compiled.code.len().saturating_sub(main_start),
                main_len
            )?;
        }
    }
    write_instruction_range(
        out,
        compiled,
        main_start,
        compiled.code.len(),
        &compiled.global_slot_names,
        &compiled.global_slot_types,
    )?;
    Ok(())
}

fn should_dump_function(
    include_all: bool,
    func: &FunctionInfo,
    user_function_names: &HashSet<String>,
) -> bool {
    if include_all {
        return true;
    }
    if user_function_names.contains(&func.name) {
        return true;
    }
    func.name
        .split_once('#')
        .is_some_and(|(parent, _)| user_function_names.contains(parent))
}

fn write_function_bytecode<W: Write>(
    out: &mut W,
    compiled: &CompiledProgram,
    index: usize,
    func: &FunctionInfo,
) -> io::Result<()> {
    let params = func
        .params
        .iter()
        .map(|(name, ty)| format!("{}::{:?}", name, ty))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        out,
        "[{}] {}({}) -> {:?} code={}..{} entry={}",
        index, func.name, params, func.return_type, func.code_start, func.code_end, func.entry
    )?;
    write_slot_table(
        out,
        &func.slot_names,
        &func.slot_types,
        &func.param_slots,
        &func.kwparams.iter().map(|kw| kw.slot).collect::<Vec<_>>(),
    )?;
    write_instruction_range(
        out,
        compiled,
        func.code_start,
        func.code_end,
        &func.slot_names,
        &func.slot_types,
    )?;
    writeln!(out)?;
    Ok(())
}

fn write_slot_table<W: Write>(
    out: &mut W,
    slot_names: &[String],
    slot_types: &[Option<VarTypeTag>],
    param_slots: &[usize],
    kwparam_slots: &[usize],
) -> io::Result<()> {
    if slot_names.is_empty() {
        writeln!(out, "  slots: (none)")?;
        return Ok(());
    }

    writeln!(out, "  slots:")?;
    for (idx, name) in slot_names.iter().enumerate() {
        let role = if param_slots.contains(&idx) {
            " param"
        } else if kwparam_slots.contains(&idx) {
            " kw"
        } else {
            ""
        };
        writeln!(
            out,
            "    #{:<3} {:<28} :: {}{}",
            idx,
            name,
            slot_tag_label(slot_types.get(idx).copied().flatten()),
            role
        )?;
    }
    Ok(())
}

fn write_instruction_range<W: Write>(
    out: &mut W,
    compiled: &CompiledProgram,
    start: usize,
    end: usize,
    slot_names: &[String],
    slot_types: &[Option<VarTypeTag>],
) -> io::Result<()> {
    if start >= end || start >= compiled.code.len() {
        writeln!(out, "  bytecode: (empty)")?;
        return Ok(());
    }

    let end = end.min(compiled.code.len());
    writeln!(out, "  bytecode:")?;
    for ip in start..end {
        let instr = &compiled.code[ip];
        let comment = instr_comment(compiled, instr, slot_names, slot_types)
            .map(|text| format!(" ; {}", text))
            .unwrap_or_default();
        writeln!(out, "    {:>6}: {:?}{}", ip, instr, comment)?;
    }
    Ok(())
}

fn slot_tag_label(tag: Option<VarTypeTag>) -> String {
    tag.map(|tag| format!("{:?}", tag))
        .unwrap_or_else(|| "unknown".to_string())
}

fn instr_comment(
    compiled: &CompiledProgram,
    instr: &Instr,
    slot_names: &[String],
    slot_types: &[Option<VarTypeTag>],
) -> Option<String> {
    if let Instr::JumpIfGtI64Slots(lhs_slot, rhs_slot, _) = instr {
        let lhs = slot_comment(*lhs_slot, slot_names, slot_types);
        let rhs = slot_comment(*rhs_slot, slot_names, slot_types);
        return Some(format!("lhs {}, rhs {}", lhs, rhs));
    }
    if let Instr::AddConstI64SlotAndJumpIfLe(slot, delta, stop_slot, _) = instr {
        let slot = slot_comment(*slot, slot_names, slot_types);
        let stop = slot_comment(*stop_slot, slot_names, slot_types);
        return Some(format!("slot {}, delta {}, stop {}", slot, delta, stop));
    }

    if let Some(slot) = instr_slot_operand(instr) {
        return Some(slot_comment(slot, slot_names, slot_types));
    }

    if let Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) = instr {
        let name = compiled
            .functions
            .get(operands.func_index)
            .map(|func| func.name.as_str())
            .unwrap_or("<missing>");
        let slots = operands
            .slots
            .iter()
            .map(|slot| slot_comment(*slot, slot_names, slot_types))
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!(
            "call #{} {} slots=[{}]",
            operands.func_index, name, slots
        ));
    }

    if let Some((func_idx, argc)) = instr_direct_call_operand(instr) {
        let name = compiled
            .functions
            .get(func_idx)
            .map(|func| func.name.as_str())
            .unwrap_or("<missing>");
        return Some(format!("call #{} {} argc={}", func_idx, name, argc));
    }

    if let Instr::CallSpecializeI64Slots(operands)
    | Instr::CallSpecializeInboundsI64Slots(operands) = instr
    {
        let name = compiled
            .specializable_functions
            .get(operands.spec_func_index)
            .map(|func| func.name.as_str())
            .unwrap_or("<missing>");
        let slots = operands
            .slots
            .iter()
            .map(|slot| slot_comment(*slot, slot_names, slot_types))
            .collect::<Vec<_>>()
            .join(", ");
        return Some(format!(
            "specialize #{} {} slots=[{}]",
            operands.spec_func_index, name, slots
        ));
    }

    if let Instr::CallSpecialize(spec_idx, argc) | Instr::CallSpecializeInbounds(spec_idx, argc) =
        instr
    {
        let name = compiled
            .specializable_functions
            .get(*spec_idx)
            .map(|func| func.name.as_str())
            .unwrap_or("<missing>");
        return Some(format!("specialize #{} {} argc={}", spec_idx, name, argc));
    }

    None
}

fn slot_comment(slot: usize, slot_names: &[String], slot_types: &[Option<VarTypeTag>]) -> String {
    let name = slot_names
        .get(slot)
        .map(String::as_str)
        .unwrap_or("<missing>");
    let tag = slot_tag_label(slot_types.get(slot).copied().flatten());
    format!("slot #{} {}::{}", slot, name, tag)
}

fn instr_direct_call_operand(instr: &Instr) -> Option<(usize, usize)> {
    match instr {
        Instr::Call(func_idx, argc)
        | Instr::CallInbounds(func_idx, argc)
        | Instr::CallResolved(func_idx, argc) => Some((*func_idx, *argc)),
        Instr::CallResolvedI64Slots(operands) | Instr::CallInboundsI64Slots(operands) => {
            Some((operands.func_index, operands.slots.len()))
        }
        Instr::CallWithKwargs(func_idx, argc, _)
        | Instr::CallWithKwargsSplat(func_idx, argc, _, _)
        | Instr::CallWithSplat(func_idx, argc, _) => Some((*func_idx, *argc)),
        _ => None,
    }
}

fn instr_slot_operand(instr: &Instr) -> Option<usize> {
    match instr {
        Instr::LoadSlot(slot)
        | Instr::StoreSlot(slot)
        | Instr::LoadSlotI64(slot)
        | Instr::LoadSlotI64ToF64(slot)
        | Instr::StoreSlotI64(slot)
        | Instr::LoadSlotF64(slot)
        | Instr::StoreSlotF64(slot)
        | Instr::LoadSlotBool(slot)
        | Instr::StoreSlotBool(slot)
        | Instr::LoadSlotF32(slot)
        | Instr::StoreSlotF32(slot)
        | Instr::LoadSlotF16(slot)
        | Instr::StoreSlotF16(slot)
        | Instr::LoadSlotStr(slot)
        | Instr::StoreSlotStr(slot)
        | Instr::LoadSlotChar(slot)
        | Instr::StoreSlotChar(slot)
        | Instr::LoadSlotNarrowInt(slot)
        | Instr::StoreSlotNarrowInt(slot)
        | Instr::LoadSlotNothing(slot)
        | Instr::StoreSlotNothing(slot)
        | Instr::LoadSlotArray(slot)
        | Instr::StoreSlotArray(slot)
        | Instr::LoadSlotTuple(slot)
        | Instr::StoreSlotTuple(slot)
        | Instr::LoadSlotNamedTuple(slot)
        | Instr::StoreSlotNamedTuple(slot)
        | Instr::LoadSlotDict(slot)
        | Instr::StoreSlotDict(slot)
        | Instr::LoadSlotSet(slot)
        | Instr::StoreSlotSet(slot)
        | Instr::LoadSlotStruct(slot)
        | Instr::StoreSlotStruct(slot)
        | Instr::LoadSlotRange(slot)
        | Instr::StoreSlotRange(slot)
        | Instr::LoadSlotRng(slot)
        | Instr::StoreSlotRng(slot)
        | Instr::LoadSlotGenerator(slot)
        | Instr::StoreSlotGenerator(slot)
        | Instr::LoadSlotSymbol(slot)
        | Instr::StoreSlotSymbol(slot)
        | Instr::LoadAddI64Slot(slot)
        | Instr::LoadSubI64Slot(slot)
        | Instr::LoadMulI64Slot(slot)
        | Instr::LoadModI64Slot(slot)
        | Instr::IncVarI64Slot(slot)
        | Instr::DecVarI64Slot(slot)
        | Instr::AddConstI64SlotAndJumpIfLe(slot, _, _, _)
        | Instr::LoadSquareF64Slot(slot)
        | Instr::LoadAddF64Slot(slot)
        | Instr::LoadSubF64Slot(slot)
        | Instr::LoadMulF64Slot(slot)
        | Instr::LoadDivF64Slot(slot) => Some(*slot),
        _ => None,
    }
}

#[cfg(test)]
mod bytecode_dump_tests {
    use super::*;

    #[test]
    fn dump_formatter_finds_slot_operands() {
        assert_eq!(instr_slot_operand(&Instr::LoadSlotF64(3)), Some(3));
        assert_eq!(instr_slot_operand(&Instr::LoadSlotI64ToF64(3)), Some(3));
        assert_eq!(instr_slot_operand(&Instr::StoreSlot(4)), Some(4));
        assert_eq!(instr_slot_operand(&Instr::LoadAddI64Slot(5)), Some(5));
        assert_eq!(instr_slot_operand(&Instr::IncVarI64Slot(6)), Some(6));
        assert_eq!(
            instr_slot_operand(&Instr::AddConstI64SlotAndJumpIfLe(6, 1, 7, 8)),
            Some(6)
        );
        assert_eq!(instr_slot_operand(&Instr::JumpIfGtI64Slots(1, 2, 3)), None);
        assert_eq!(
            instr_slot_operand(&Instr::CallSpecializeI64Slots(Box::new(
                subset_julia_vm::vm::CallSpecializeSlots {
                    spec_func_index: 1,
                    slots: vec![2, 3],
                }
            ))),
            None
        );
        assert_eq!(
            instr_slot_operand(&Instr::CallResolvedI64Slots(Box::new(
                subset_julia_vm::vm::CallDirectSlots {
                    func_index: 1,
                    slots: vec![2, 3],
                }
            ))),
            None
        );
        assert_eq!(instr_slot_operand(&Instr::PushI64(1)), None);
    }

    #[test]
    fn dump_formatter_finds_direct_call_operands() {
        assert_eq!(
            instr_direct_call_operand(&Instr::CallResolved(12, 3)),
            Some((12, 3))
        );
        assert_eq!(
            instr_direct_call_operand(&Instr::CallWithSplat(13, 2, vec![false, true])),
            Some((13, 2))
        );
        assert_eq!(
            instr_direct_call_operand(&Instr::CallResolvedI64Slots(Box::new(
                subset_julia_vm::vm::CallDirectSlots {
                    func_index: 14,
                    slots: vec![1, 2],
                }
            ))),
            Some((14, 2))
        );
        assert_eq!(instr_direct_call_operand(&Instr::PushI64(1)), None);
    }
}

fn dump_ast_for_file(file_path: &str, json_output: bool) {
    // Check if file exists
    if !Path::new(file_path).exists() {
        eprintln!("Error: File '{}' not found", file_path);
        std::process::exit(1);
    }

    let source = fs::read_to_string(file_path).unwrap_or_else(|e| {
        eprintln!("Error reading file '{}': {}", file_path, e);
        std::process::exit(1);
    });

    dump_ast_for_code(&source, json_output);
}

fn dump_ast_for_code(source: &str, json_output: bool) {
    use subset_julia_vm_parser::parse_with_errors;

    let (cst, errors) = parse_with_errors(source);

    if json_output {
        // JSON output mode
        let output = serde_json::json!({
            "ast": cst.to_json(),
            "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            "has_error": cst.has_error(),
            "source_lines": source.lines().enumerate().map(|(i, line)| {
                serde_json::json!({
                    "line": i + 1,
                    "content": line
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        // Human-readable output with source line annotations
        let source_lines: Vec<&str> = source.lines().collect();

        println!("=== Source Code ===\n");
        for (i, line) in source_lines.iter().enumerate() {
            println!("{:4} | {}", i + 1, line);
        }
        println!();

        println!("=== AST Structure ===\n");
        debug_ast_with_lines(&cst, 0, &source_lines);
        println!();

        if !errors.is_empty() {
            println!("=== Parse Errors ===");
            for error in &errors {
                println!("  {}", error);
            }
            println!();
        }

        if cst.has_error() {
            println!("=== Error Nodes in Tree ===");
            for error_node in cst.errors() {
                let line_content = source_lines
                    .get(error_node.span.start_line.saturating_sub(1))
                    .unwrap_or(&"");
                println!(
                    "  Error at {}:{} - {}:{}: {}",
                    error_node.span.start_line,
                    error_node.span.start_column,
                    error_node.span.end_line,
                    error_node.span.end_column,
                    line_content.trim()
                );
            }
            println!();
        }
    }
}

/// Print AST with source line annotations for better debugging
fn debug_ast_with_lines(
    node: &subset_julia_vm_parser::CstNode,
    indent: usize,
    _source_lines: &[&str],
) {
    let pad = "  ".repeat(indent);

    // Build the line: [field_name: ]NodeKind[ = "text"] [L:start_line]
    let field_prefix = match &node.field_name {
        Some(name) => format!("{}: ", name),
        None => String::new(),
    };

    let text_suffix = match &node.text {
        Some(t) => format!(" = {:?}", t),
        None => String::new(),
    };

    // Add line annotation for better navigation
    let line_annotation = format!(" [L{}:{}]", node.span.start_line, node.span.start_column);

    println!(
        "{}{}{:?}{}{}",
        pad, field_prefix, node.kind, text_suffix, line_annotation
    );

    for child in &node.children {
        debug_ast_with_lines(child, indent + 1, _source_lines);
    }
}

fn compile_to_ir(input_file: &str, output_file: &str) {
    // Check if file exists
    if !Path::new(input_file).exists() {
        eprintln!("Error: File '{}' not found", input_file);
        std::process::exit(1);
    }

    let source = fs::read_to_string(input_file).unwrap_or_else(|e| {
        eprintln!("Error reading file '{}': {}", input_file, e);
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
    let user_method_sigs: HashSet<_> = program.functions.iter().map(get_method_signature).collect();
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

    // Merge functions by exact signature so user methods extend Base overload sets.
    let mut all_functions: Vec<_> = prelude_program
        .functions
        .into_iter()
        .filter(|f| !user_method_sigs.contains(&get_method_signature(f)))
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
    merged_main_stmts.push(subset_julia_vm::ir::core::Stmt::Meta {
        annotation: subset_julia_vm::ir::core::MetaAnnotation {
            name: subset_julia_vm::ir::core::BASE_USER_MAIN_BOUNDARY_META.to_string(),
            args: Vec::new(),
        },
        span: subset_julia_vm::span::Span::new(0, 0, 0, 0, 0, 0),
    });
    merged_main_stmts.extend(program.main.stmts);
    program.main = subset_julia_vm::ir::core::Block {
        stmts: merged_main_stmts,
        span: program.main.span,
    };

    // Load external modules if needed
    let existing_modules: HashSet<String> =
        program.modules.iter().map(|m| m.name.clone()).collect();
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

    // Save to Core IR file
    if let Err(e) = core_ir_file::save(&program, output_file) {
        eprintln!("Error saving Core IR: {}", e);
        std::process::exit(1);
    }

    println!("Compiled: {} -> {}", input_file, output_file);
}

fn compile_to_vm_bytecode(input_file: &str, output_file: &str) {
    if !Path::new(input_file).exists() {
        eprintln!("Error: File '{}' not found", input_file);
        std::process::exit(1);
    }

    let source = fs::read_to_string(input_file).unwrap_or_else(|e| {
        eprintln!("Error reading file '{}': {}", input_file, e);
        std::process::exit(1);
    });

    let base_dir = Path::new(input_file).parent().map(Path::to_path_buf);
    let program = subset_julia_vm::pipeline::parse_and_lower_with_base_dir(&source, base_dir)
        .unwrap_or_else(|e| {
            eprintln!("Pipeline error: {e}");
            std::process::exit(1);
        });
    let compiled = subset_julia_vm::compile::compile_with_cache(&program).unwrap_or_else(|e| {
        eprintln!("Compilation error: {e:?}");
        std::process::exit(1);
    });

    if let Err(e) = vm_bytecode_file::save(&program, &compiled, output_file) {
        eprintln!("Error saving VM bytecode: {}", e);
        std::process::exit(1);
    }

    println!("Compiled VM bytecode: {} -> {}", input_file, output_file);
}

fn run_type_stability_analysis(file_path: &str, strict_mode: bool, json_output: bool) {
    use subset_julia_vm::compile::type_stability::{
        format_report, AnalysisConfig, OutputFormat, TypeStabilityAnalyzer,
    };

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

    // Merge prelude with user program (same as run_file)
    let user_method_sigs: HashSet<_> = program.functions.iter().map(get_method_signature).collect();
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
        .collect();
    let base_function_count = all_functions.len();
    all_functions.append(&mut program.functions);
    program.functions = all_functions;
    program.base_function_count = base_function_count;

    // Run type stability analysis
    let config = AnalysisConfig {
        include_base_functions: false,
        user_functions_only: true,
        strict_parameter_typing: strict_mode,
    };

    let mut analyzer = TypeStabilityAnalyzer::with_config(config);
    let report = analyzer.analyze_program_with_production_inference(&program);

    // Output the report
    let format = if json_output {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    };

    match format_report(&report, format) {
        Ok(output) => println!("{}", output),
        Err(e) => {
            eprintln!("Error formatting report: {}", e);
            std::process::exit(1);
        }
    }

    // Exit with code 1 if strict mode and unstable functions found
    if strict_mode && report.has_unstable() {
        std::process::exit(1);
    }
}

fn print_logo() {
    let pink = "\x1b[38;2;249;38;114m";
    let yellow = "\x1b[38;2;230;219;116m";
    let purple = "\x1b[38;2;174;129;255m";
    let green = "\x1b[38;2;166;226;46m";
    let cyan = "\x1b[38;2;102;217;239m";
    let orange = "\x1b[38;2;253;151;31m";
    let reset = "\x1b[0m";

    let line1 = format!(
        "   {}╔═╗{}╔═╗{}╔╦╗{}╔═╗{}╔═╗{}╔═╗{}╔╦╗{}╔═╗{}",
        pink, yellow, purple, green, cyan, orange, pink, yellow, reset
    );
    let line2 = format!(
        "   {}║ ╦{}║ ║{}║║║{}╠═╣{}║ ╦{}║ ║{}║║║{}╠═╣{}",
        pink, yellow, purple, green, cyan, orange, pink, yellow, reset
    );
    let line3 = format!(
        "   {}╚═╝{}╚═╝{}╩ ╩{}╩ ╩{}╚═╝{}╚═╝{}╩ ╩{}╩ ╩{}",
        pink, yellow, purple, green, cyan, orange, pink, yellow, reset
    );
    let line4 = format!(
        "   {}╦╔═{}╦ ╦{}╦ ╦{}╦╔═{}╦╔═{}╦ ╦{}╦ ╦{}",
        purple, green, cyan, orange, pink, yellow, purple, reset
    );
    let line5 = format!(
        "   {}╠╩╗{}╚╦╝{}║ ║{}╠╩╗{}╠╩╗{}╚╦╝{}║ ║{}",
        purple, green, cyan, orange, pink, yellow, purple, reset
    );
    let line6 = format!(
        "   {}╩ ╩{} ╩ {}╚═╝{}╩ ╩{}╩ ╩{} ╩ {}╚═╝{}",
        purple, green, cyan, orange, pink, yellow, purple, reset
    );

    println!();
    println!("{}", line1);
    println!("{}", line2);
    println!("{}", line3);
    println!("{}", line4);
    println!("{}", line5);
    println!("{}", line6);
}

fn is_incomplete(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for ch in trimmed.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }

        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            _ => {}
        }
    }

    if paren_depth > 0 || bracket_depth > 0 || brace_depth > 0 {
        return true;
    }

    let keywords_open = [
        "function", "if", "for", "while", "try", "begin", "module", "struct",
    ];
    let keyword_close = "end";

    let mut depth = 0i32;
    for line in trimmed.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let line = if let Some(idx) = line.find('#') {
            &line[..idx]
        } else {
            line
        };

        for word in line.split_whitespace() {
            let word_lower = word.to_lowercase();
            if keywords_open.iter().any(|k| word_lower == *k) {
                depth += 1;
            } else if word_lower == keyword_close {
                depth -= 1;
            }
        }
    }

    depth > 0
}

fn print_result_with_context(
    result: &subset_julia_vm::repl::REPLResult,
    source: Option<&str>,
    session: &subset_julia_vm::repl::REPLSession,
) {
    if !result.output.is_empty() {
        print!("{}", result.output);
        if !result.output.ends_with('\n') {
            println!();
        }
    }

    if result.success {
        if let Some(src) = source {
            if let Some(func_name) = extract_function_name(src) {
                println!("{} (generic function with 1 method)", func_name);
                println!();
                return;
            }
            if let Some(struct_name) = extract_struct_name(src) {
                println!("{}", struct_name);
                println!();
                return;
            }
        }

        if let Some(ref value) = result.value {
            // Prefer the result's user-defined `show` rendering (Issue #7168);
            // fall back to the default struct-field formatter otherwise.
            match &result.value_display {
                Some(display) => println!("{}", display),
                None => println!(
                    "{}",
                    format_value_with_vm(value, Some(session.get_struct_heap()))
                ),
            }
        }
    } else if let Some(ref error) = result.error {
        eprintln!("{}ERROR:{} {}", colors::KEYWORD, colors::RESET, error);
    }

    println!();
}

fn extract_function_name(src: &str) -> Option<String> {
    let trimmed = src.trim();
    if !trimmed.starts_with("function ") {
        return None;
    }

    let rest = trimmed.strip_prefix("function ")?.trim();
    let name_end = rest.find('(')?;
    let name = rest[..name_end].trim();

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn extract_struct_name(src: &str) -> Option<String> {
    let trimmed = src.trim();

    let rest = if trimmed.starts_with("mutable struct ") {
        trimmed.strip_prefix("mutable struct ")?
    } else if trimmed.starts_with("struct ") {
        trimmed.strip_prefix("struct ")?
    } else {
        return None;
    };

    let name = rest.split_whitespace().next()?;
    let name = name.split('{').next()?;

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn format_f64(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}.0", v as i64)
    } else {
        format!("{}", v)
    }
}

/// Format a Complex struct for REPL display, preserving type-correct formatting.
///
/// REPL bare-value display has no live VM at print time, so it cannot dispatch
/// to the pure-Julia `Base.show(io, ::Complex)`. This Rust fallback is kept
/// consistent with that method — including the `Complex{Bool}` / imaginary-unit
/// cases — so REPL output matches upstream Julia (Issue #5155). The numeric
/// arms additionally wrap the text in the REPL's NUMBER color.
fn format_complex_repl(
    s: &subset_julia_vm::vm::StructInstance,
    struct_heap: Option<&[subset_julia_vm::vm::StructInstance]>,
) -> String {
    if s.values.len() != 2 {
        return "Complex(?, ?)".to_string();
    }
    // Complex{Bool}: `im` for the imaginary unit, `Complex(re,im)` otherwise.
    if let (Value::Bool(re), Value::Bool(im)) = (&s.values[0], &s.values[1]) {
        let text = if !*re && *im {
            "im".to_string()
        } else {
            format!("Complex({},{})", re, im)
        };
        return format!("{}{}{}", colors::NUMBER, text, colors::RESET);
    }
    let re_str = format_value_with_vm(&s.values[0], struct_heap);
    let im_val = &s.values[1];
    let is_negative = match im_val {
        Value::F64(x) => *x < 0.0,
        Value::I64(x) => *x < 0,
        Value::F32(x) => *x < 0.0,
        _ => false,
    };
    if is_negative {
        let neg_im = match im_val {
            Value::F64(x) => format_value_with_vm(&Value::F64(-x), struct_heap),
            Value::I64(x) => format_value_with_vm(&Value::I64(-x), struct_heap),
            Value::F32(x) => format_value_with_vm(&Value::F32(-x), struct_heap),
            other => format_value_with_vm(other, struct_heap),
        };
        format!(
            "{}{} - {}im{}",
            colors::NUMBER,
            re_str,
            neg_im,
            colors::RESET
        )
    } else {
        let im_str = format_value_with_vm(im_val, struct_heap);
        format!(
            "{}{} + {}im{}",
            colors::NUMBER,
            re_str,
            im_str,
            colors::RESET
        )
    }
}

/// Format a Rational struct for REPL display as `num//den`, matching the
/// pure-Julia `Base.show(io, ::Rational)`.
///
/// Issue #5160: shared between the inline `Value::Struct` and heap
/// `Value::StructRef` display paths so a heap-resident Rational no longer
/// falls through to the generic `Rational(num, den)` rendering — the same
/// inline/heap display gap that Issue #5155 closed for Complex.
fn format_rational_repl(
    s: &subset_julia_vm::vm::StructInstance,
    struct_heap: Option<&[subset_julia_vm::vm::StructInstance]>,
) -> String {
    let fields: Vec<String> = s
        .values
        .iter()
        .map(|v| format_value_with_vm(v, struct_heap))
        .collect();
    if fields.len() == 2 {
        format!(
            "{}{}//{}{}",
            colors::NUMBER,
            fields[0],
            fields[1],
            colors::RESET
        )
    } else {
        // Malformed Rational (not exactly num/den): fall back to generic struct.
        format!("{}({})", s.struct_name, fields.join(", "))
    }
}

/// Format a float value for range display.
fn format_range_float(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

/// Format the legacy native-array carrier for REPL display. Extracted from
/// `format_value_with_vm` so the latter no longer needs a native-array
/// match arm; routed through the shared `native_array_value_ref` helper
/// (Issue #3908).
fn format_native_array_with_vm(
    arr_ref: &subset_julia_vm::vm::value::ArrayRef,
    struct_heap: Option<&[subset_julia_vm::vm::StructInstance]>,
) -> String {
    let arr_borrow = arr_ref.borrow();
    // Calculate total number of elements from shape
    let total_elements: usize = arr_borrow.shape.iter().product();

    if arr_borrow.shape.len() == 1 {
        // 1D array: use to_value_vec to handle all element types including Complex
        let values = arr_borrow.to_value_vec();
        // Ensure we only format the actual elements (not more than shape indicates)
        let elements: Vec<String> = values
            .iter()
            .take(total_elements)
            .map(|v| format_value_with_vm(v, struct_heap))
            .collect();
        format!("[{}]", elements.join(", "))
    } else if arr_borrow.shape.len() == 2 {
        // 2D matrix: special handling for F64, otherwise use to_value_vec
        match &arr_borrow.data {
            subset_julia_vm::vm::value::ArrayData::F64(data) => {
                let rows = arr_borrow.shape[0];
                let cols = arr_borrow.shape[1];
                let element_type = arr_borrow.element_type();
                let type_name = element_type.julia_type_name();
                let mut lines = Vec::new();
                for r in 0..rows {
                    let row: Vec<String> = (0..cols)
                        .map(|c| format_f64(data[r + c * rows])) // column-major
                        .collect();
                    lines.push(format!(" {}", row.join("  ")));
                }
                format!(
                    "{}×{} Matrix{{{}}}:\n{}",
                    rows,
                    cols,
                    type_name,
                    lines.join("\n")
                )
            }
            _ => {
                // For non-F64 2D arrays, convert to values and format
                let values = arr_borrow.to_value_vec();
                let rows = arr_borrow.shape[0];
                let cols = arr_borrow.shape[1];
                let element_type = arr_borrow.element_type();
                let type_name = element_type.julia_type_name();
                let mut lines = Vec::new();
                for r in 0..rows {
                    let row: Vec<String> = (0..cols)
                        .map(|c| format_value_with_vm(&values[r + c * rows], struct_heap)) // column-major
                        .collect();
                    lines.push(format!(" {}", row.join("  ")));
                }
                format!(
                    "{}×{} Matrix{{{}}}:\n{}",
                    rows,
                    cols,
                    type_name,
                    lines.join("\n")
                )
            }
        }
    } else {
        // Higher-dimensional arrays: use debug format
        format!("{:?}", arr_borrow)
    }
}

fn format_value_with_vm(
    value: &Value,
    struct_heap: Option<&[subset_julia_vm::vm::StructInstance]>,
) -> String {
    // Route the legacy native-array carrier through the shared
    // `native_array_value_ref` helper so the match below no longer holds a
    // native-array arm (Issue #3908).
    if let Some(arr_ref) = subset_julia_vm::vm::value::native_array_value_ref(value) {
        return format_native_array_with_vm(arr_ref, struct_heap);
    }
    match value {
        Value::I64(v) => format!("{}", v),
        Value::F64(v) => format_f64(*v),
        // New numeric types
        Value::I8(v) => format!("{}", v),
        Value::I16(v) => format!("{}", v),
        Value::I32(v) => format!("{}", v),
        Value::I128(v) => format!("{}", v),
        Value::U8(v) => format!("{}", v),
        Value::U16(v) => format!("{}", v),
        Value::U32(v) => format!("{}", v),
        Value::U64(v) => format!("{}", v),
        Value::U128(v) => format!("{}", v),
        Value::F16(v) => format!("Float16({})", v),
        Value::F32(v) => format!("{}", v),
        Value::Str(s) => format!("{}\"{}\"{}", colors::STRING, s, colors::RESET),
        Value::Range(r) => {
            if r.is_float {
                if r.is_unit_range() {
                    format!(
                        "{}:{}",
                        format_range_float(r.start),
                        format_range_float(r.stop)
                    )
                } else {
                    format!(
                        "{}:{}:{}",
                        format_range_float(r.start),
                        format_range_float(r.step),
                        format_range_float(r.stop)
                    )
                }
            } else if r.is_unit_range() {
                format!("{}:{}", r.start as i64, r.stop as i64)
            } else {
                format!("{}:{}:{}", r.start as i64, r.step as i64, r.stop as i64)
            }
        }
        Value::Tuple(t) => {
            let elements: Vec<String> = t
                .elements
                .iter()
                .map(|v| format_value_with_vm(v, struct_heap))
                .collect();
            format!("({})", elements.join(", "))
        }
        Value::NamedTuple(nt) => {
            let pairs: Vec<String> = nt
                .names
                .iter()
                .zip(nt.values.iter())
                .map(|(n, v)| format!("{} = {}", n, format_value_with_vm(v, struct_heap)))
                .collect();
            format!("({})", pairs.join(", "))
        }
        Value::Nothing => format!("{}nothing{}", colors::BOOL, colors::RESET),
        Value::Missing => format!("{}missing{}", colors::BOOL, colors::RESET),
        Value::Struct(s) if s.is_complex() => format_complex_repl(s, struct_heap),
        Value::Struct(s) if s.is_rational() => format_rational_repl(s, struct_heap),
        // Inline `Array{T,N}` wrapper (the host-return boundary's output since
        // #6864): materialize and reuse the native array formatter for the
        // shape-aware `[…]` display, not the generic `StructName(...)` form.
        Value::Struct(s) if s.array_wrapper_julia_type().is_some() => {
            match subset_julia_vm::vm::value::array_wrapper_value_to_array_value(
                value,
                struct_heap.unwrap_or(&[]),
            ) {
                Ok(Some(arr)) => format_native_array_with_vm(
                    &subset_julia_vm::vm::value::new_array_ref(arr),
                    struct_heap,
                ),
                _ => {
                    let fields: Vec<String> = s
                        .values
                        .iter()
                        .map(|v| format_value_with_vm(v, struct_heap))
                        .collect();
                    format!("{}({})", s.struct_name, fields.join(", "))
                }
            }
        }
        Value::Struct(s) => {
            // General case: StructName(field1, field2, ...)
            let fields: Vec<String> = s
                .values
                .iter()
                .map(|v| format_value_with_vm(v, struct_heap))
                .collect();
            format!("{}({})", s.struct_name, fields.join(", "))
        }
        Value::StructRef(id) => {
            // Try to resolve StructRef to actual struct if struct_heap is available
            if let Some(heap) = struct_heap {
                if let Some(struct_instance) = heap.get(*id) {
                    // Check if it's a complex number. Route through the shared
                    // `format_complex_repl` so the type-correct field display
                    // and `Complex{Bool}` / `im` special cases match the
                    // pure-Julia `Base.show(io, ::Complex)` (Issue #5155),
                    // rather than the lossy f64-projection used previously.
                    if struct_instance.is_complex() {
                        return format_complex_repl(struct_instance, struct_heap);
                    }
                    // Issue #5160: heap-resident Rationals display as `num//den`
                    // via the same shared formatter as the inline path, instead
                    // of falling through to the generic `Rational(num, den)`.
                    if struct_instance.is_rational() {
                        return format_rational_repl(struct_instance, struct_heap);
                    }
                    // General case: StructName(field1, field2, ...)
                    let fields: Vec<String> = struct_instance
                        .values
                        .iter()
                        .map(|v| format_value_with_vm(v, struct_heap))
                        .collect();
                    format!("{}({})", struct_instance.struct_name, fields.join(", "))
                } else {
                    format!("StructRef#{}", id)
                }
            } else {
                format!("StructRef#{}", id)
            }
        }
        Value::Rng(_) => "Random.default_rng()".to_string(),
        Value::SliceAll => ":".to_string(),
        Value::Ref(inner) => {
            // Base.RefValue{T}(value) (Issue #5130) - matches upstream display.
            let v = inner.borrow();
            format!(
                "Base.RefValue{{{}}}({})",
                v.runtime_type(),
                format_value_with_vm(&v, struct_heap)
            )
        }
        Value::Char(c) => format!("'{}'", c),
        Value::Generator(_) => "<generator>".to_string(),
        // DataType displays as type name, with Complex{FloatNN} → ComplexFNN alias (Issue #5704).
        Value::DataType(jt) => subset_julia_vm::vm::apply_complex_float_aliases(&jt.to_string()),
        Value::RuntimeTypeVar(tv) => format!("TypeVar(:{})", tv.name),
        Value::Module(m) => m.name.clone(), // Module displays as module name
        Value::Function(f) => format!("{} (generic function)", f.name),
        Value::BigInt(b) => format!("{}", b),
        Value::BigFloat(b) => format!("{}", b),
        Value::IO(_) => "<io>".to_string(),
        Value::Undef => "#undef".to_string(),
        Value::Bool(b) => format!("{}{}{}", colors::BOOL, b, colors::RESET),
        Value::Symbol(s) => format!(":{}", s.as_str()),
        Value::Expr(e) => format!("Expr(:{}, ...)", e.head.as_str()),
        Value::QuoteNode(inner) => {
            format!("QuoteNode({})", format_value_with_vm(inner, struct_heap))
        }
        Value::LineNumberNode(ln) => format!(":(#= line {} =#)", ln.line),
        Value::GlobalRef(gr) => format!("GlobalRef({},:{})", gr.module, gr.name.as_str()),
        Value::ComposedFunction(cf) => {
            let outer_str = format_value_with_vm(&cf.outer, struct_heap);
            let inner_str = format_value_with_vm(&cf.inner, struct_heap);
            format!("{} ∘ {}", outer_str, inner_str)
        }
        Value::Pairs(p) => {
            let pairs: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(k, v)| format!(":{} => {}", k, format_value_with_vm(v, struct_heap)))
                .collect();
            format!("pairs({})", pairs.join(", "))
        }
        Value::Regex(r) => format!("r\"{}\"", r.pattern),
        Value::RegexMatch(m) => format!("RegexMatch(\"{}\")", m.match_str),
        Value::Enum { type_name, value } => format!("{}({})", type_name, value),
        Value::Closure(c) => format!("{} (closure)", c.name),
        Value::Memory(mem) => {
            let mem = mem.borrow();
            let n = mem.len();
            let type_name = mem.element_type().julia_type_name();
            if n == 0 {
                format!("0-element Memory{{{}}}", type_name)
            } else {
                let mut parts = Vec::new();
                for i in 1..=n.min(10) {
                    if let Ok(v) = mem.get(i) {
                        parts.push(format_value_with_vm(&v, struct_heap));
                    }
                }
                if n > 10 {
                    format!("[{}, ...]", parts.join(", "))
                } else {
                    format!("[{}]", parts.join(", "))
                }
            }
        }
        Value::MemoryRef(memref) => format!(
            "{}(index={})",
            memref.julia_type_name(),
            memref.memory_index()
        ),
        // The legacy native-array carrier is filtered out by the
        // early-return above (Issue #3908). This wildcard satisfies Rust's
        // exhaustiveness checking and provides a safe default for any
        // future `Value` variant: fall back to the value's Debug
        // representation.
        _ => format!("{:?}", value),
    }
}

fn print_help() {
    println!(
        r#"
{}SubsetJuliaVM REPL Commands:{}
  help(), ?       Show this help message
  exit(), quit()  Exit the REPL
  reset()         Clear all variables and definitions
  vars(), whos()  Show defined variables

{}Keyboard Shortcuts:{}
  Ctrl-C      Cancel current input
  Ctrl-D      Exit the REPL
  Up/Down     Navigate history
  Tab         Complete names/LaTeX, or insert 4 spaces

{}Supported Julia Syntax:{}
  - Arithmetic: +, -, *, /, ^, %
  - Comparisons: <, >, <=, >=, ==, !=
  - Logical: &&, ||, !
  - Control: if/elseif/else, for, while, break, continue
  - Functions: function f(x) ... end, x -> x^2
  - Arrays: [1, 2, 3], zeros(n), ones(n), rand(n)
  - Matrix: A * B, A .* B
  - Strings: "hello $(name)"
  - And more...

{}Special Variables:{}
  ans         The result of the last evaluation
"#,
        colors::FUNC_CALL,
        colors::RESET,
        colors::FUNC_CALL,
        colors::RESET,
        colors::FUNC_CALL,
        colors::RESET,
        colors::FUNC_CALL,
        colors::RESET,
    );
}

fn print_variables(session: &REPLSession) {
    let names = session.variable_names();
    if names.is_empty() {
        println!("No variables defined.\n");
    } else {
        println!("{}Defined variables:{}", colors::FUNC_CALL, colors::RESET);
        for name in names {
            println!("  {}", name);
        }
        println!();
    }
}

fn dirs_path() -> Option<std::path::PathBuf> {
    env::var("HOME")
        .ok()
        .map(|home| std::path::PathBuf::from(home).join(".subset_julia_vm"))
}
