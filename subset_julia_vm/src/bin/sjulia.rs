#![deny(clippy::expect_used)]
//! SubsetJuliaVM Command-Line Interface
//!
//! Usage:
//!   sjulia                          # Start interactive REPL
//!   sjulia file.jl                  # Execute Julia file
//!   sjulia -e "code"                # Execute code string
//!   sjulia --compile file.jl -o out # Compile to bytecode file (.sjbc)

use std::env;
use std::fs;
use std::path::Path;

use std::collections::HashSet;
use subset_julia_vm::base;
use subset_julia_vm::bytecode;
use subset_julia_vm::compile::compile_core_program;
use subset_julia_vm::loader;
use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;
use subset_julia_vm::repl::REPLSession;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::{Value, Vm};

// Import REPL dependencies
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::{CmdKind, Highlighter};
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Config, Context, Editor, Helper};
use std::borrow::Cow;
use subset_julia_vm::unicode::{completions_for_prefix, latex_to_unicode};

#[path = "sjulia/runners.rs"]
mod runners;

use runners::{run_code, run_file, run_repl};

const VERSION: &str = env!("CARGO_PKG_VERSION");

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
                    if (chars[i] == '=' && next == '=')
                        || (chars[i] == '!' && next == '=')
                        || (chars[i] == '<' && next == '=')
                        || (chars[i] == '>' && next == '=')
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
}

struct JuliaHelper {
    highlighter: JuliaHighlighter,
}

impl JuliaHelper {
    fn new() -> Self {
        Self {
            highlighter: JuliaHighlighter,
        }
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
        let before_cursor = &line[..pos];

        if let Some(backslash_pos) = before_cursor.rfind('\\') {
            let latex_prefix = &before_cursor[backslash_pos..];

            let is_valid_latex = latex_prefix.len() > 1
                && latex_prefix[1..]
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '^');

            if is_valid_latex {
                if let Some(unicode) = latex_to_unicode(latex_prefix) {
                    return Ok((
                        backslash_pos,
                        vec![Pair {
                            display: format!("{} → {}", latex_prefix, unicode),
                            replacement: unicode.to_string(),
                        }],
                    ));
                }

                let completions = completions_for_prefix(latex_prefix);
                if !completions.is_empty() {
                    let pairs: Vec<Pair> = completions
                        .into_iter()
                        .map(|(latex, unicode)| Pair {
                            display: format!("{} → {}", latex, unicode),
                            replacement: unicode.to_string(),
                        })
                        .collect();
                    return Ok((backslash_pos, pairs));
                }
            }
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
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        // No arguments - start REPL
        run_repl();
    } else if args[1] == "-e" {
        // -e option: execute code string
        if args.len() < 3 {
            eprintln!("Error: -e requires an argument");
            std::process::exit(1);
        }
        let code = &args[2];
        run_code(code);
    } else if args[1] == "--compile" || args[1] == "-c" {
        // --compile option: compile to bytecode file
        if args.len() < 3 {
            eprintln!("Error: --compile requires an input file");
            std::process::exit(1);
        }
        let input_file = &args[2];

        // Look for -o option
        let output_file = if args.len() >= 5 && (args[3] == "-o" || args[3] == "--output") {
            args[4].clone()
        } else {
            // Default output: input stem + .sjbc
            let path = Path::new(input_file);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            format!("{}.sjbc", stem)
        };

        compile_to_bytecode(input_file, &output_file);
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
    } else if args[1] == "-h" || args[1] == "--help" {
        print_usage();
    } else {
        // File path provided - execute file
        let file_path = &args[1];
        run_file(file_path);
    }
}

fn print_usage() {
    println!(
        r#"SubsetJuliaVM - Julia Subset Runtime

USAGE:
    sjulia                              Start interactive REPL
    sjulia <file.jl>                    Execute Julia file
    sjulia -e <code>                    Execute code string
    sjulia --compile <file.jl> -o <out> Compile to bytecode file (.sjbc)
    sjulia --type-stability <file.jl>   Analyze type stability
    sjulia --dump-ast <file.jl>         Dump AST structure for debugging
    sjulia --dump-ast -e <code>         Dump AST for code string
    sjulia --dump-ast --json <file.jl>  Dump AST in JSON format
    sjulia --precompile-base <out.bin> Generate Base cache for embedding

OPTIONS:
    -e <code>             Execute code string
    -c, --compile <file>  Compile source to bytecode file
    -o, --output <file>   Output file for --compile (default: <input>.sjbc)
    -t, --type-stability  Analyze type stability of functions
        --strict          Strict mode (exit code 1 if unstable functions found)
        --json            Output in JSON format (for --type-stability and --dump-ast)
        --dump-ast        Dump AST structure (useful for debugging parser tests)
        --precompile-base Generate Base bytecode cache for build-time embedding
    -h, --help            Show this help message

EXAMPLES:
    sjulia hello.jl
    sjulia -e "println(1 + 2)"
    sjulia --compile program.jl -o program.sjbc
    sjulia --type-stability program.jl
    sjulia --type-stability --strict --json program.jl
    sjulia --dump-ast -e "x = 1 + 2"
    sjulia --dump-ast --json -e "x = 1 + 2"
"#
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

fn compile_to_bytecode(input_file: &str, output_file: &str) {
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

    // Save to bytecode file
    if let Err(e) = bytecode::save(&program, output_file) {
        eprintln!("Error saving bytecode: {}", e);
        std::process::exit(1);
    }

    println!("Compiled: {} -> {}", input_file, output_file);
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
    let user_func_names: HashSet<_> = program.functions.iter().map(|f| f.name.as_str()).collect();
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
        .filter(|f| !user_func_names.contains(f.name.as_str()))
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
    let report = analyzer.analyze_program(&program);

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
            println!(
                "{}",
                format_value_with_vm(value, Some(session.get_struct_heap()))
            );
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
fn format_complex_repl(
    s: &subset_julia_vm::vm::StructInstance,
    struct_heap: Option<&[subset_julia_vm::vm::StructInstance]>,
) -> String {
    if s.values.len() != 2 {
        return "Complex(?, ?)".to_string();
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

fn format_value(value: &Value) -> String {
    format_value_with_vm(value, None)
}

fn format_value_with_vm(
    value: &Value,
    struct_heap: Option<&[subset_julia_vm::vm::StructInstance]>,
) -> String {
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
        Value::Array(arr) => {
            let arr_borrow = arr.borrow();
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
        Value::Range(r) => {
            if r.is_unit_range() {
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
        Value::Dict(d) => {
            use subset_julia_vm::vm::DictKey;
            let pairs: Vec<String> = d
                .iter()
                .map(|(k, v)| {
                    let key_str = match k {
                        DictKey::Str(s) => format!("\"{}\"", s),
                        DictKey::I64(i) => format!("{}", i),
                        DictKey::Symbol(s) => format!(":{}", s),
                    };
                    format!("{} => {}", key_str, format_value_with_vm(v, struct_heap))
                })
                .collect();
            format!("Dict({})", pairs.join(", "))
        }
        Value::Nothing => format!("{}nothing{}", colors::BOOL, colors::RESET),
        Value::Missing => format!("{}missing{}", colors::BOOL, colors::RESET),
        Value::Struct(s) if s.is_complex() => format_complex_repl(s, struct_heap),
        Value::Struct(s) => {
            // Special case: Rational - display as num//den like Julia
            if s.struct_name == "Rational" || s.struct_name.starts_with("Rational{") {
                if s.values.len() == 2 {
                    let num = format_value_with_vm(&s.values[0], struct_heap);
                    let den = format_value_with_vm(&s.values[1], struct_heap);
                    return format!("{}{}//{}{}", colors::NUMBER, num, den, colors::RESET);
                }
            }
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
                    // Check if it's a complex number
                    let struct_val = Value::Struct(struct_instance.clone());
                    if struct_val.is_complex() {
                        if let Some((re, im)) = struct_val.as_complex_parts() {
                            if im >= 0.0 {
                                return format!(
                                    "{}{} + {}im{}",
                                    colors::NUMBER,
                                    re,
                                    im,
                                    colors::RESET
                                );
                            } else {
                                return format!(
                                    "{}{} - {}im{}",
                                    colors::NUMBER,
                                    re,
                                    im.abs(),
                                    colors::RESET
                                );
                            }
                        }
                    }
                    // General struct display: StructName(field1, field2, ...)
                    // (No special treatment for Rational.)
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
        Value::Ref(inner) => format!("Ref({})", format_value_with_vm(inner, struct_heap)),
        Value::Char(c) => format!("'{}'", c),
        Value::Generator(_) => "<generator>".to_string(),
        Value::DataType(jt) => jt.to_string(), // DataType displays as type name
        Value::Module(m) => m.name.clone(),    // Module displays as module name
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
        Value::Set(s) => {
            use subset_julia_vm::vm::DictKey;
            let elements: Vec<String> = s
                .elements
                .iter()
                .map(|v| match v {
                    DictKey::Str(s) => format!("\"{}\"", s),
                    DictKey::I64(i) => format!("{}", i),
                    DictKey::Symbol(s) => format!(":{}", s),
                })
                .collect();
            format!("Set([{}])", elements.join(", "))
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
  Tab         Insert 4 spaces, or complete LaTeX (e.g., \alpha → α)

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
