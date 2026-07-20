//! C ABI entry points for iOS and other native platforms.
//!
//! The core VM lives in the `subset_julia_vm` rlib; this crate links it into
//! `staticlib` / `cdylib` artifacts without forcing triple `crate-type` compiles
//! during everyday `cargo build` / `cargo test` on the VM crate.

mod abi_version;
mod basic;
mod bytecode;
mod demo;
mod detailed;
mod error;
mod format;
mod panic_boundary;
mod repl_ffi;
#[cfg(not(target_arch = "wasm32"))]
mod unicode_ffi;

pub use abi_version::{subset_julia_vm_abi_version, SUBSET_VM_C_ABI_VERSION};

pub use basic::{
    compile_and_run, compile_and_run_auto, compile_and_run_with_output, compile_to_ir, free_string,
    run_ir_json_f64, run_ir_json_f64_N_seed, run_ir_json_f_N_seed, vm_request_cancel,
    vm_reset_cancel,
};

pub use bytecode::{run_vm_bytecode_detailed, run_vm_bytecode_streaming};

pub use demo::subset_julia_vm_demo;

pub use detailed::{compile_and_run_detailed, compile_and_run_streaming, OutputCallback};

pub use error::{
    execution_result_array_element_f64, execution_result_array_element_json,
    execution_result_array_element_kind, execution_result_array_len,
    execution_result_artifact_count, execution_result_artifact_data,
    execution_result_artifact_data_at, execution_result_artifact_mime,
    execution_result_artifact_mime_at, execution_result_complex_imag,
    execution_result_complex_real, execution_result_dict_key_json, execution_result_dict_len,
    execution_result_dict_value_json, execution_result_value_json, execution_result_value_kind,
    free_execution_result, CError, CErrorKind, CExecutionResult, CSpan, CValueKind,
};

pub use format::{format_struct_instance, format_value};

pub use repl_ffi::{
    free_repl_result, is_expression_complete, repl_result_artifact_count,
    repl_result_artifact_data, repl_result_artifact_data_at, repl_result_artifact_mime,
    repl_result_artifact_mime_at, repl_session_eval, repl_session_free, repl_session_new,
    repl_session_reset, split_expressions, subset_julia_vm_set_memory_budget_bytes, CREPLResult,
};

#[cfg(not(target_arch = "wasm32"))]
pub use unicode_ffi::{
    unicode_completions, unicode_expand, unicode_lookup, unicode_reverse_lookup,
};
