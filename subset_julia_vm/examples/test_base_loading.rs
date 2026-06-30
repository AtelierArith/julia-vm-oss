//! Test base loading with Pure Rust parser
//!
//! Run with: cargo run --example test_base_loading

use subset_julia_vm::lowering::Lowering;
use subset_julia_vm::parser::{ParseOutcome, RustParsedSource};

fn main() {
    let source = subset_julia_vm::base::get_base();
    println!("Base source length: {} bytes", source.len());

    // Find the problematic line (span 563-586)
    println!("\nContent around span 563-586:");
    if let Some(s) = source.get(550..600) {
        println!("'{}'", s);
    }

    // Parse using pure Rust parser
    println!("\nParsing base...");
    let cst = subset_julia_vm_parser::parse(&source).expect("parse error");
    println!("Parse successful");

    // Create ParseOutcome for the full Lowering pipeline
    let parse_outcome = ParseOutcome::Rust(RustParsedSource {
        cst,
        source: source.to_string(),
    });

    // Lower using full Lowering pipeline
    println!("Lowering...");
    let mut lowering = Lowering::new(&source);
    match lowering.lower(parse_outcome) {
        Ok(program) => println!("Lowering successful! {} functions", program.functions.len()),
        Err(e) => {
            println!("Lowering error: {:?}", e);
            // Find and print the error location
            if let Some(span) = source.get(e.span.start..e.span.end) {
                println!("Error at: '{}'", span);
            }
            // Print context around error
            let start = e.span.start.saturating_sub(50);
            let end = (e.span.end + 50).min(source.len());
            if let Some(context) = source.get(start..end) {
                println!("\nContext:\n{}", context);
            }
        }
    }
}
