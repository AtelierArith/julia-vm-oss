//! Basic FFI functions for running Julia code.
//!
//! These functions provide the core C ABI for compiling and executing Julia programs.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::cancel;
use crate::compile::compile_with_cache;
use crate::ir::core::Program;
use crate::pipeline::parse_and_lower;
use crate::rng::StableRng;
use crate::vm::{Value, Vm};

/// Request cancellation of the current VM execution.
#[no_mangle]
pub extern "C" fn vm_request_cancel() {
    cancel::request();
}

/// Reset the cancellation flag.
#[no_mangle]
pub extern "C" fn vm_reset_cancel() {
    cancel::reset();
}

/// Run a JSON Core IR program.
/// Returns cnt as i64. Negative values indicate parse/compile errors.
#[no_mangle]
pub extern "C" fn run_ir_json_f_N_seed(json_ptr: *const c_char, _n: i64, seed: u64) -> i64 {
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
}

/// Run a JSON Core IR program, returning f64.
#[no_mangle]
pub extern "C" fn run_ir_json_f64_N_seed(json_ptr: *const c_char, _n: i64, seed: u64) -> f64 {
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
}

/// Run a JSON Core IR program (convenience wrapper).
#[no_mangle]
pub extern "C" fn run_ir_json_f64(json_ptr: *const c_char) -> f64 {
    run_ir_json_f64_N_seed(json_ptr, 0, 0)
}

/// Compile Julia subset source to Core IR JSON.
/// Returns a heap-allocated C string that must be freed with `free_string`.
/// Returns null on error.
#[no_mangle]
pub extern "C" fn compile_to_ir(src_ptr: *const c_char) -> *mut c_char {
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
}

/// Free a string allocated by `compile_to_ir`.
#[no_mangle]
pub extern "C" fn free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

/// Compile and run Julia subset source with function definition and call.
/// e.g., "function f(N) ... end\nf(1000)"
/// Returns the result as f64. Returns NaN on error.
#[no_mangle]
pub extern "C" fn compile_and_run(src_ptr: *const c_char, seed: u64) -> f64 {
    compile_and_run_auto(src_ptr, seed)
}

/// Compile and run Julia subset source (auto-detect function or simple program).
/// Supports both:
/// - "function f(N) ... end\nf(1000)"
/// - "println(\"Hello world\")"
///
/// Returns the result as f64. Returns NaN on error or for void results.
#[no_mangle]
pub extern "C" fn compile_and_run_auto(src_ptr: *const c_char, seed: u64) -> f64 {
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

    match vm.run() {
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
        Ok(Value::Array(_)) => f64::NAN, // Arrays can't be returned as f64
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
        Ok(Value::Dict(_)) => f64::NAN, // Dict can't be returned as f64
        Ok(Value::Set(_)) => f64::NAN, // Set can't be returned as f64
        Ok(Value::Range(_)) => f64::NAN, // Range can't be returned as f64
        Ok(Value::Ref(inner)) => {
            // Unwrap Ref and return numeric value
            match inner.as_ref() {
                Value::I64(x) => *x as f64,
                Value::F64(x) => *x,
                _ => f64::NAN,
            }
        }
        Ok(Value::Generator(_)) => f64::NAN, // Generator can't be returned as f64
        Ok(Value::Char(_)) => f64::NAN,      // Char cannot be returned as f64
        Ok(Value::DataType(_)) => f64::NAN,  // DataType cannot be returned as f64
        Ok(Value::Module(_)) => f64::NAN,    // Module cannot be returned as f64
        Ok(Value::Function(_)) => f64::NAN,  // Function cannot be returned as f64
        Ok(Value::Closure(_)) => f64::NAN,   // Closure cannot be returned as f64
        Ok(Value::ComposedFunction(_)) => f64::NAN, // ComposedFunction cannot be returned as f64
        Ok(Value::BigInt(_)) => f64::NAN,    // BigInt cannot be losslessly returned as f64
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
        Err(_) => f64::NAN,
    }
}

/// Compile and run Julia subset source, returning output as a string.
/// Returns a heap-allocated C string that must be freed with `free_string`.
/// The output includes both println output and the result value.
/// Returns null on error.
#[no_mangle]
pub extern "C" fn compile_and_run_with_output(src_ptr: *const c_char, seed: u64) -> *mut c_char {
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

    // Append result
    match result {
        Ok(Value::I64(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::F64(x)) => output.push_str(&format!("[result] {}\n", x)),
        // New numeric types
        Ok(Value::I8(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::I16(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::I32(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::I128(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::U8(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::U16(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::U32(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::U64(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::U128(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::F16(x)) => output.push_str(&format!("[result] Float16({})\n", x.to_f32())),
        Ok(Value::F32(x)) => output.push_str(&format!("[result] {}\n", x)),
        Ok(Value::Bool(b)) => {
            output.push_str(&format!("[result] {}\n", if b { "true" } else { "false" }))
        }
        Ok(Value::Nothing) => {} // No result to show
        Ok(Value::Missing) => output.push_str("[result] missing\n"),
        Ok(Value::Str(s)) => output.push_str(&format!("[result] \"{}\"\n", s)),
        Ok(Value::Array(arr)) => output.push_str(&format!("[result] {:?}\n", arr.borrow())),
        Ok(ref val @ Value::Struct(_)) if val.is_complex() => {
            if let Some((re, im)) = val.as_complex_parts() {
                output.push_str(&format!("[result] {} + {}im\n", re, im));
            }
        }
        Ok(Value::Struct(_)) => output.push_str("[result] <struct>\n"),
        Ok(Value::StructRef(_)) => output.push_str("[result] <struct ref>\n"),
        Ok(Value::SliceAll) => {}
        Ok(Value::Rng(_)) => output.push_str("[result] <RNG>\n"),
        Ok(Value::Tuple(t)) => output.push_str(&format!("[result] ({:?})\n", t.elements)),
        Ok(Value::NamedTuple(nt)) => {
            output.push_str(&format!("[result] <NamedTuple {:?}>\n", nt.names))
        }
        Ok(Value::Dict(d)) => {
            output.push_str(&format!("[result] Dict({} pairs)\n", d.len()))
        }
        Ok(Value::Set(s)) => {
            output.push_str(&format!("[result] Set({} elements)\n", s.elements.len()))
        }
        Ok(Value::Range(r)) => {
            if r.is_unit_range() {
                output.push_str(&format!("[result] {:.0}:{:.0}\n", r.start, r.stop));
            } else {
                output.push_str(&format!(
                    "[result] {:.0}:{:.0}:{:.0}\n",
                    r.start, r.step, r.stop
                ));
            }
        }
        Ok(Value::Ref(inner)) => output.push_str(&format!("[result] Ref({:?})\n", inner)),
        Ok(Value::Generator(_)) => output.push_str("[result] <Generator>\n"),
        Ok(Value::Char(c)) => output.push_str(&format!("[result] '{}'\n", c)),
        Ok(Value::DataType(jt)) => output.push_str(&format!("[result] {}\n", jt)),
        Ok(Value::Module(m)) => output.push_str(&format!("[result] Module({})\n", m.name)),
        Ok(Value::Function(f)) => output.push_str(&format!("[result] function {}\n", f.name)),
        Ok(Value::Closure(c)) => {
            if c.captures.is_empty() {
                output.push_str(&format!("[result] closure {}\n", c.name));
            } else {
                let caps: Vec<String> = c.captures.iter().map(|(n, _)| n.clone()).collect();
                output.push_str(&format!(
                    "[result] closure {} [captures: {}]\n",
                    c.name,
                    caps.join(", ")
                ));
            }
        }
        Ok(Value::ComposedFunction(cf)) => {
            output.push_str(&format!("[result] {:?} ∘ {:?}\n", cf.outer, cf.inner));
        }
        Ok(Value::BigInt(n)) => output.push_str(&format!("[result] {}\n", n)),
        Ok(Value::BigFloat(bf)) => output.push_str(&format!("[result] {}\n", bf)),
        Ok(Value::Undef) => output.push_str("[result] #undef\n"),
        Ok(Value::IO(io_ref)) => {
            if io_ref.borrow().is_stdout() {
                output.push_str("[result] stdout\n");
            } else {
                output.push_str("[result] IOBuffer(...)\n");
            }
        }
        // Macro system types
        Ok(Value::Symbol(s)) => output.push_str(&format!("[result] :{}\n", s.as_str())),
        Ok(Value::Expr(e)) => output.push_str(&format!("[result] {}\n", e)),
        Ok(Value::QuoteNode(v)) => output.push_str(&format!("[result] QuoteNode({:?})\n", v)),
        Ok(Value::LineNumberNode(ln)) => output.push_str(&format!("[result] {}\n", ln)),
        Ok(Value::GlobalRef(gr)) => output.push_str(&format!("[result] {}\n", gr)),
        // Base.Pairs type
        Ok(Value::Pairs(p)) => {
            let pairs_str: Vec<String> = p
                .data
                .names
                .iter()
                .zip(p.data.values.iter())
                .map(|(n, v)| format!(":{} => {:?}", n, v))
                .collect();
            output.push_str(&format!("[result] pairs({})\n", pairs_str.join(", ")));
        }
        // Regex types
        Ok(Value::Regex(r)) => {
            if r.flags.is_empty() {
                output.push_str(&format!("[result] r\"{}\"\n", r.pattern));
            } else {
                output.push_str(&format!("[result] r\"{}\"{}\n", r.pattern, r.flags));
            }
        }
        Ok(Value::RegexMatch(m)) => {
            output.push_str(&format!("[result] RegexMatch(\"{}\")\n", m.match_str));
        }
        // Enum type
        Ok(Value::Enum { type_name, value }) => {
            output.push_str(&format!("[result] {}({})\n", type_name, value));
        }
        // Memory type
        Ok(Value::Memory(mem)) => {
            let mem = mem.borrow();
            let type_name = mem.element_type().julia_type_name();
            output.push_str(&format!(
                "[result] {}-element Memory{{{}}}\n",
                mem.len(),
                type_name
            ));
        }
        Err(e) => output.push_str(&format!("[error] {}\n", e)),
    }

    match CString::new(output) {
        Ok(cstr) => cstr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
