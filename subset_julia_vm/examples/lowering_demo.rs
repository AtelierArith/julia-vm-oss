//! Lowering (CST → Core IR) Demonstration
//!
//! This example shows how CST is converted to Core IR.
//!
//! Key design principle: **Lowering decides what runs**
//! - Supported syntax → Core IR nodes
//! - Unsupported syntax → UnsupportedFeature error with span + hint
//! - All errors include location information for editor highlighting
//!
//! Core IR is minimal:
//! - Control: Block, If, While, For, Break, Continue, Return
//! - Data: Assign, Var, Literal
//! - Operations: BinaryOp, UnaryOp, Call
//! - Functions: Function
//!
//! Run with: `cargo run --example lowering_demo`

use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::Parser;

fn main() {
    println!("=== Lowering (CST → Core IR) Demo ===\n");

    let examples = vec![
        // Supported examples
        ("Arithmetic expression", "result = 1 + 2 * 3", true),
        ("println with string", r#"println("Hello, World!")"#, true),
        (
            "If/else statement",
            r#"x = 10
if x > 5
    println("big")
else
    println("small")
end"#,
            true,
        ),
        (
            "For loop",
            r#"sum = 0
for i in 1:10
    sum = sum + i
end
println(sum)"#,
            true,
        ),
        (
            "Function definition",
            r#"function add(a, b)
    return a + b
end
add(1, 2)"#,
            true,
        ),
        ("Array comprehension", "squares = [x^2 for x in 1:5]", true),
        ("Lambda expression", "f = x -> x^2\nf(3)", true),
        (
            "Higher-order function (map)",
            "map(x -> x * 2, [1, 2, 3])",
            true,
        ),
        // Unsupported examples (will show UnsupportedFeature error)
        ("Macro call (unsupported)", "@info \"hello\"", false),
        (
            "Using statement (unsupported)",
            "using LinearAlgebra",
            false,
        ),
    ];

    for (name, source, expected_success) in examples {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("【{}】", name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        println!("Source:");
        for (i, line) in source.lines().enumerate() {
            println!("  {:2}│ {}", i + 1, line);
        }
        println!();

        // Parse
        let mut parser = Parser::new().expect("Failed to create parser");
        let outcome = match parser.parse(source) {
            Ok(o) => o,
            Err(e) => {
                println!("✗ Parse error: {:?}", e);
                println!();
                continue;
            }
        };

        // Lower to Core IR
        let mut lowering = Lowering::new(source);
        match lowering.lower(outcome) {
            Ok(program) => {
                if !expected_success {
                    println!("⚠ Expected error but lowering succeeded!");
                } else {
                    println!("✓ Lowering successful!");
                }

                // Show program structure
                println!("\nProgram structure:");
                println!("  - structs: {} 個", program.structs.len());
                println!("  - functions: {} 個", program.functions.len());
                println!("  - modules: {} 個", program.modules.len());
                println!("  - main statements: {} 個", program.main.stmts.len());

                // Show Core IR JSON (truncated for readability)
                let ir_json = serde_json::to_string_pretty(&program).unwrap();
                println!("\nCore IR (JSON):");
                for line in ir_json.lines().take(30) {
                    println!("  {}", line);
                }
                if ir_json.lines().count() > 30 {
                    println!("  ... (truncated)");
                }
            }
            Err(e) => {
                if expected_success {
                    println!("⚠ Expected success but lowering failed!");
                } else {
                    println!("✗ UnsupportedFeature error (expected):");
                }

                // Show error details with span information
                println!("\nError kind: {:?}", e.kind);
                println!(
                    "Span: line {}:{} - {}:{}",
                    e.span.start_line, e.span.start_column, e.span.end_line, e.span.end_column
                );
                println!("Byte range: {} - {}", e.span.start, e.span.end);

                if let Some(hint) = &e.hint {
                    println!("Hint: {}", hint);
                }

                // Show the problematic code with position marker
                println!("\nProblematic code:");
                let lines: Vec<&str> = source.lines().collect();
                let start_line = e.span.start_line.saturating_sub(1);
                let end_line = e.span.end_line.min(lines.len());
                for i in start_line..end_line {
                    println!("  {:2}│ {}", i + 1, lines.get(i).unwrap_or(&""));
                    // Show marker for error position
                    if i == e.span.start_line.saturating_sub(1) {
                        let marker_pos = e.span.start_column.saturating_sub(1);
                        let marker_len = if e.span.start_line == e.span.end_line {
                            (e.span.end_column - e.span.start_column).max(1)
                        } else {
                            lines
                                .get(i)
                                .map(|l| l.len() - marker_pos)
                                .unwrap_or(1)
                                .max(1)
                        };
                        println!("    │ {}{}", " ".repeat(marker_pos), "^".repeat(marker_len));
                    }
                }
            }
        }

        println!();
    }

    println!("=== Lowering Demo Complete ===");
}
