// Prevent accidental debug output in library code (Issue #2888).
// CLI binaries (bin/) may use eprintln!() for user-facing error messages.
#![deny(clippy::print_stderr)]

// Core modules
pub mod builtins;
pub mod cancel;
pub mod compile;
pub mod error;
pub(crate) mod expr_heads;
pub mod include;
pub mod inference_core;
pub mod intrinsics;
pub mod ir;
pub mod julia;
pub use julia::{base, packages, stdlib}; // Re-export for backwards compatibility
pub mod base_loader;
pub mod loader;
pub mod rng;
pub mod span;
pub mod types;
pub mod unicode;
pub mod vm;

// Parser module
pub mod parser;

// Pure Rust stdlib loader
pub mod stdlib_loader;

// Lowering: CST -> Core IR
pub mod lowering;

// Persisted Core IR and VM bytecode file formats
pub mod core_ir_file;
pub mod vm_bytecode_file;

// REPL session management
pub mod repl;

// Plot rendering utilities (SVG artifact generation)
pub mod plotting;

// AoT (Ahead-of-Time) compiler module
#[cfg(feature = "aot")]
pub mod aot;

// Pipeline: parse and lower Julia source
pub mod pipeline;
pub use pipeline::get_prelude_program;

// Rust API for programmatic use
pub mod api;
pub use api::{
    compile_and_run_auto_str, compile_and_run_str, compile_and_run_value, compile_to_ir_str,
    run_ir_json_str,
};

// Shared with `subset_julia_vm_ffi` (C ABI crate).
#[doc(hidden)]
pub mod ffi_support;
