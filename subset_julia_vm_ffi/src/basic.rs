//! Basic FFI functions for running Julia code.
//!
//! These functions provide the core C ABI for compiling and executing Julia programs.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use super::format::format_value;
use crate::panic_boundary;
use subset_julia_vm::cancel;
use subset_julia_vm::compile::host_support::compile_with_cache;
use subset_julia_vm::ffi_support::is_native_array_value;
use subset_julia_vm::ir::core::Program;
// The C ABI "run a whole buffer" / "compile a buffer to IR" host entries use
// strict file-mode soft scope (Issue #9283) so the iOS/native editor matches
// `julia file.jl`: a top-level loop assignment to an existing global binds a new
// local (Issue #9210). The interactive REPL (`repl_session_eval`) is separate and
// stays lenient.
use subset_julia_vm::pipeline::parse_and_lower_strict as parse_and_lower;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm_bytecode::Value;

/// Request cancellation of the current VM execution.
#[no_mangle]
pub extern "C" fn vm_request_cancel() {
    panic_boundary::catch_unwind_ffi(
        |_| (),
        || {
            cancel::request();
        },
    )
}

/// Reset the cancellation flag.
#[no_mangle]
pub extern "C" fn vm_reset_cancel() {
    panic_boundary::catch_unwind_ffi(
        |_| (),
        || {
            cancel::reset();
        },
    )
}

/// Run a JSON Core IR program.
/// Returns cnt as i64. Negative values indicate parse/compile errors.
#[no_mangle]
pub extern "C" fn run_ir_json_f_N_seed(json_ptr: *const c_char, _n: i64, seed: u64) -> i64 {
    panic_boundary::catch_unwind_ffi(
        |_| -6,
        || {
            if json_ptr.is_null() {
                return -1;
            }
            cancel::reset();
            let json = unsafe { CStr::from_ptr(json_ptr) }
                .to_string_lossy()
                .to_string();

            let program: Program = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(_) => return -2,
            };

            let compiled = match compile_with_cache(&program) {
                Ok(c) => c,
                Err(_) => return -3,
            };

            let rng = StableRng::new(seed);
            let mut vm = Vm::new_program(compiled, rng);

            match vm.run() {
                Ok(Value::I64(x)) => x,
                Ok(Value::F64(x)) => x as i64,
                Ok(_) => -4,
                Err(_) => -5, // Runtime error (e.g., assertion failed)
            }
        },
    )
}

/// Run a JSON Core IR program, returning f64.
#[no_mangle]
pub extern "C" fn run_ir_json_f64_N_seed(json_ptr: *const c_char, _n: i64, seed: u64) -> f64 {
    panic_boundary::catch_unwind_ffi(
        |_| f64::NAN,
        || {
            if json_ptr.is_null() {
                return f64::NAN;
            }
            cancel::reset();
            let json = unsafe { CStr::from_ptr(json_ptr) }
                .to_string_lossy()
                .to_string();

            let program: Program = match serde_json::from_str(&json) {
                Ok(v) => v,
                Err(_) => return f64::NAN,
            };

            let compiled = match compile_with_cache(&program) {
                Ok(c) => c,
                Err(_) => return f64::NAN,
            };

            let rng = StableRng::new(seed);
            let mut vm = Vm::new_program(compiled, rng);

            match vm.run() {
                Ok(Value::I64(x)) => x as f64,
                Ok(Value::F64(x)) => x,
                Ok(_) => f64::NAN,
                Err(_) => f64::NAN,
            }
        },
    )
}

/// Run a JSON Core IR program (convenience wrapper).
#[no_mangle]
pub extern "C" fn run_ir_json_f64(json_ptr: *const c_char) -> f64 {
    panic_boundary::catch_unwind_ffi(|_| f64::NAN, || run_ir_json_f64_N_seed(json_ptr, 0, 0))
}

/// Compile Julia subset source to Core IR JSON.
/// Returns a heap-allocated C string that must be freed with `free_string`.
/// Returns null on error.
///
/// # Safety
///
/// `src_ptr` must be null or point to a valid null-terminated C string that
/// stays alive for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn compile_to_ir(src_ptr: *const c_char) -> *mut c_char {
    panic_boundary::catch_unwind_ffi(
        |_| std::ptr::null_mut(),
        || {
            if src_ptr.is_null() {
                return std::ptr::null_mut();
            }

            let src = match unsafe { CStr::from_ptr(src_ptr) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            let program = match parse_and_lower(src) {
                Ok(p) => p,
                Err(_) => return std::ptr::null_mut(),
            };

            let json = match serde_json::to_string_pretty(&program) {
                Ok(j) => j,
                Err(_) => return std::ptr::null_mut(),
            };

            match CString::new(json) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}

/// Free a string allocated by `compile_to_ir`.
///
/// # Safety
///
/// `ptr` must be null or a pointer previously returned by a string-allocating
/// function of this library, and must not be freed more than once or used
/// after this call.
#[no_mangle]
pub unsafe extern "C" fn free_string(ptr: *mut c_char) {
    panic_boundary::catch_unwind_ffi(
        |_| (),
        || {
            if !ptr.is_null() {
                unsafe {
                    drop(CString::from_raw(ptr));
                }
            }
        },
    )
}

/// Compile and run Julia subset source with function definition and call.
/// e.g., "function f(N) ... end\nf(1000)"
/// Returns the result as f64. Returns NaN on error.
#[no_mangle]
pub extern "C" fn compile_and_run(src_ptr: *const c_char, seed: u64) -> f64 {
    panic_boundary::catch_unwind_ffi(|_| f64::NAN, || compile_and_run_auto(src_ptr, seed))
}

/// Compile and run Julia subset source (auto-detect function or simple program).
/// Supports both:
/// - "function f(N) ... end\nf(1000)"
/// - "println(\"Hello world\")"
///
/// Returns the result as f64. Returns NaN on error or for void results.
#[no_mangle]
pub extern "C" fn compile_and_run_auto(src_ptr: *const c_char, seed: u64) -> f64 {
    panic_boundary::catch_unwind_ffi(
        |_| f64::NAN,
        || {
            if src_ptr.is_null() {
                return f64::NAN;
            }
            cancel::reset();

            let src = match unsafe { CStr::from_ptr(src_ptr) }.to_str() {
                Ok(s) => s,
                Err(_) => return f64::NAN,
            };

            let program = match parse_and_lower(src) {
                Ok(p) => p,
                Err(_) => return f64::NAN,
            };

            let compiled = match compile_with_cache(&program) {
                Ok(c) => c,
                Err(_) => return f64::NAN,
            };

            let rng = StableRng::new(seed);
            let mut vm = Vm::new_program(compiled, rng);

            let result = vm.run();
            // The legacy native-array carrier cannot be returned as `f64`; route it
            // through the shared `native_array_value_ref` helper before the match
            // below so the match no longer needs a native-array arm (Issue #3908).
            // Matches the prior semantics: arrays return NaN.
            if let Ok(ref val) = result {
                if is_native_array_value(val) {
                    return f64::NAN;
                }
            }

            match result {
                Ok(Value::I64(x)) => x as f64,
                Ok(Value::F64(x)) => x,
                // New numeric types - convert to f64
                Ok(Value::I8(x)) => x as f64,
                Ok(Value::I16(x)) => x as f64,
                Ok(Value::I32(x)) => x as f64,
                Ok(Value::I128(x)) => x as f64,
                Ok(Value::U8(x)) => x as f64,
                Ok(Value::U16(x)) => x as f64,
                Ok(Value::U32(x)) => x as f64,
                Ok(Value::U64(x)) => x as f64,
                Ok(Value::U128(x)) => x as f64,
                Ok(Value::F16(x)) => x.to_f64(),
                Ok(Value::F32(x)) => x as f64,
                Ok(Value::Bool(b)) => {
                    if b {
                        1.0
                    } else {
                        0.0
                    }
                }
                Ok(Value::Nothing) => 0.0, // Void result (e.g., println returns nothing)
                Ok(Value::Missing) => f64::NAN, // Missing cannot be returned as f64
                Ok(Value::Str(_)) => f64::NAN,
                Ok(ref val @ Value::Struct(_)) if val.is_complex() => {
                    // Complex struct - return real part
                    val.as_complex_parts().map(|(re, _)| re).unwrap_or(f64::NAN)
                }
                Ok(Value::Struct(_)) => f64::NAN, // Structs can't be returned as f64
                Ok(Value::StructRef(_)) => f64::NAN, // StructRef can't be returned as f64
                Ok(Value::SliceAll) => f64::NAN,
                Ok(Value::Rng(_)) => f64::NAN, // RNG can't be returned as f64
                Ok(Value::Tuple(_)) => f64::NAN, // Tuple can't be returned as f64
                Ok(Value::NamedTuple(_)) => f64::NAN, // NamedTuple can't be returned as f64
                Ok(Value::Range(_)) => f64::NAN, // Range can't be returned as f64
                Ok(Value::Ref(inner)) => {
                    // Unwrap Ref and return numeric value
                    match &*inner.borrow() {
                        Value::I64(x) => *x as f64,
                        Value::F64(x) => *x,
                        _ => f64::NAN,
                    }
                }
                Ok(Value::Generator(_)) => f64::NAN, // Generator can't be returned as f64
                Ok(Value::Char(_)) => f64::NAN,      // Char cannot be returned as f64
                Ok(Value::DataType(_)) => f64::NAN,  // DataType cannot be returned as f64
                Ok(Value::RuntimeTypeVar(_)) => f64::NAN, // TypeVar cannot be returned as f64
                Ok(Value::Module(_)) => f64::NAN,    // Module cannot be returned as f64
                Ok(Value::Function(_)) => f64::NAN,  // Function cannot be returned as f64
                Ok(Value::Closure(_)) => f64::NAN,   // Closure cannot be returned as f64
                Ok(Value::ComposedFunction(_)) => f64::NAN, // ComposedFunction cannot be returned as f64
                Ok(Value::BigInt(_)) => f64::NAN, // BigInt cannot be losslessly returned as f64
                Ok(Value::BigFloat(ref bf)) => bf.to_string().parse::<f64>().unwrap_or(f64::NAN),
                Ok(Value::Undef) => f64::NAN, // #undef cannot be returned as f64
                Ok(Value::IO(_)) => f64::NAN, // IO cannot be returned as f64
                // Macro system types cannot be returned as f64
                Ok(Value::Symbol(_)) => f64::NAN,
                Ok(Value::Expr(_)) => f64::NAN,
                Ok(Value::QuoteNode(_)) => f64::NAN,
                Ok(Value::LineNumberNode(_)) => f64::NAN,
                Ok(Value::GlobalRef(_)) => f64::NAN,
                // Base.Pairs type cannot be returned as f64
                Ok(Value::Pairs(_)) => f64::NAN,
                // Regex types cannot be returned as f64
                Ok(Value::Regex(_)) => f64::NAN,
                Ok(Value::RegexMatch(_)) => f64::NAN,
                // Enum type - return the integer value
                Ok(Value::Enum { value, .. }) => value as f64,
                // Memory type cannot be returned as f64
                Ok(Value::Memory(_)) => f64::NAN,
                Ok(Value::MemoryRef(_)) => f64::NAN,
                Err(_) => f64::NAN,
                // The legacy native-array carrier is filtered out by the early-return
                // above (Issue #3908). This wildcard satisfies Rust's exhaustiveness
                // checking and provides a safe default for any future `Value` variant:
                // return NaN, matching the prior fallthrough semantics for
                // non-numeric types.
                _ => f64::NAN,
            }
        },
    )
}

/// Compile and run Julia subset source, returning output as a string.
/// Returns a heap-allocated C string that must be freed with `free_string`.
/// The output includes both println output and the result value.
/// Returns null on error.
#[no_mangle]
pub extern "C" fn compile_and_run_with_output(src_ptr: *const c_char, seed: u64) -> *mut c_char {
    panic_boundary::catch_unwind_ffi(
        |_| std::ptr::null_mut(),
        || {
            if src_ptr.is_null() {
                return std::ptr::null_mut();
            }
            cancel::reset();

            let src = match unsafe { CStr::from_ptr(src_ptr) }.to_str() {
                Ok(s) => s,
                Err(_) => return std::ptr::null_mut(),
            };

            let program = match parse_and_lower(src) {
                Ok(p) => p,
                Err(_) => return std::ptr::null_mut(),
            };

            let compiled = match compile_with_cache(&program) {
                Ok(c) => c,
                Err(_) => return std::ptr::null_mut(),
            };

            let rng = StableRng::new(seed);
            let mut vm = Vm::new_program(compiled, rng);
            let result = vm.run();
            let mut output = vm.get_output().to_string();

            match result {
                Ok(Value::Nothing) => {} // No result to show
                Ok(value) => output.push_str(&format!("[result] {}\n", format_value(&value))),
                Err(e) => output.push_str(&format!("[error] {}\n", e)),
            }

            match CString::new(output) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => std::ptr::null_mut(),
            }
        },
    )
}
