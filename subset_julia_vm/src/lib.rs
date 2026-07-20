// Prevent accidental debug output in library code (Issue #2888).
// CLI binaries (bin/) may use eprintln!() for user-facing error messages.
#![deny(clippy::print_stderr)]

// Core modules
pub mod builtins;
pub mod cancel;
pub mod compile {
    pub use subset_julia_vm_compile::compile::*;

    pub mod host_support {
        use crate::ir::core::Program;
        use subset_julia_vm_bytecode::CompiledProgram;
        use subset_julia_vm_compile::compile::{integration_support as raw, CResult};

        fn install() {
            crate::macro_runtime::install();
        }

        pub fn compile_core_program(program: &Program) -> CResult<CompiledProgram> {
            install();
            raw::compile_core_program(program)
        }

        pub fn compile_with_cache(program: &Program) -> CResult<CompiledProgram> {
            install();
            raw::compile_with_cache(program)
        }

        pub fn begin_warm_start_prefetch() {
            install();
            raw::begin_warm_start_prefetch();
        }

        pub fn warm_base_cache() {
            install();
            raw::warm_base_cache();
        }

        pub fn warm_start_compile_cache() {
            install();
            raw::warm_start_compile_cache();
        }

        pub fn base_cache_debug_status() -> super::cache::BaseCacheDebugStatus {
            install();
            raw::base_cache_debug_status()
        }

        pub fn is_compile_cache_initialized() -> bool {
            raw::is_compile_cache_initialized()
        }

        pub fn clear_compile_cache() {
            raw::clear_compile_cache();
        }

        pub fn clear_non_base_compile_cache() {
            raw::clear_non_base_compile_cache();
        }

        pub fn clear_program_compile_cache() {
            raw::clear_program_compile_cache();
        }
    }
}
// `error` and `span` are now owned by `subset_julia_vm_ir` (Issue #8655).
// Re-exports preserve `crate::error::*` / `crate::span::*` paths for all
// existing callers during the migration window (CRATE_SPLIT.md §8 risk 6).
pub use subset_julia_vm_ir::error;
pub use subset_julia_vm_ir::span;
pub(crate) use subset_julia_vm_lowering::expr_heads;
pub use subset_julia_vm_lowering::include;
// `inference_core` and `types` are now owned by `subset_julia_vm_types`
// (Issue #8655).  Re-exports below preserve `crate::inference_core::*` and
// `crate::types::*` paths for existing callers.
pub use subset_julia_vm_types::inference_core;
pub use subset_julia_vm_types::types;
pub mod intrinsics;
pub mod ir;
pub mod julia;
pub use julia::{base, packages, stdlib}; // Re-export for backwards compatibility
pub mod base_loader;
pub mod loader;
pub mod promotion;
pub use subset_julia_vm_vm::register_vm;
pub mod rng;
pub(crate) mod runtime_types;
pub mod unicode;
pub use subset_julia_vm_vm::vm;

// Parser/lowering facade owned by the shared lowering crate.
pub use subset_julia_vm_lowering::parser;

// Pure Rust stdlib loader
pub mod stdlib_loader;

// Lowering: CST -> Core IR (physically owned by the shared lowering crate).
pub use subset_julia_vm_lowering::lowering;

// VM-backed macro expansion (Issue #8656, CRATE_SPLIT.md §4.4): executes
// macro bodies on `vm::Vm` after `compile_with_cache`, so it lives at the
// integration root and is injected into lowering via the
// `lowering::macro_expander::MacroExpander` seam.
// `pub` (not `pub(crate)`) because the `bin/` targets are separate crates in
// the same package and construct `Lowering` directly, so they must install
// the expander themselves.
pub mod macro_runtime;

// Persisted Core IR and VM bytecode file formats
pub mod core_ir_file;
pub mod vm_bytecode_file;

// REPL session management
pub mod repl;

// Plot rendering utilities (SVG artifact generation)
pub use subset_julia_vm_vm::plotting;

// AoT (Ahead-of-Time) compiler module
#[cfg(feature = "aot")]
pub mod aot;

// Pipeline: parse and lower Julia source
pub mod pipeline;
pub use pipeline::get_prelude_program;

// Rust API for programmatic use
pub mod api;
pub use api::{
    compile_and_run_auto_str, compile_and_run_str, compile_and_run_str_file_mode,
    compile_and_run_value, compile_and_run_value_file_mode, compile_to_ir_str, run_ir_json_str,
};

// Shared with `subset_julia_vm_ffi` (C ABI crate).
#[doc(hidden)]
pub mod ffi_support;
