//! REPL session FFI functions.
//!
//! These functions provide a C ABI for interactive REPL sessions.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use super::format::format_value;
use crate::repl::REPLSession;

/// C-compatible REPL evaluation result
#[repr(C)]
pub struct CREPLResult {
    pub success: bool,
    pub output: *mut c_char, // println/print output only
    pub value: *mut c_char,  // formatted result value (separate from output)
    pub error: *mut c_char,
}

impl CREPLResult {
    fn success_with_value(output: String, value: Option<String>) -> Self {
        CREPLResult {
            success: true,
            output: CString::new(output)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            value: value
                .and_then(|v| CString::new(v).ok())
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            error: std::ptr::null_mut(),
        }
    }

    fn error(output: String, error_msg: String) -> Self {
        CREPLResult {
            success: false,
            output: CString::new(output)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            value: std::ptr::null_mut(),
            error: CString::new(error_msg)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
        }
    }
}

/// Create a new REPL session.
/// Returns an opaque pointer to the session, or null on failure.
#[no_mangle]
pub extern "C" fn repl_session_new(seed: u64) -> *mut REPLSession {
    let session = Box::new(REPLSession::new(seed));
    Box::into_raw(session)
}

/// Evaluate code in a REPL session.
/// Returns a heap-allocated CREPLResult that must be freed with free_repl_result.
#[no_mangle]
pub extern "C" fn repl_session_eval(
    session: *mut REPLSession,
    src: *const c_char,
) -> *mut CREPLResult {
    if session.is_null() {
        let result = CREPLResult::error(String::new(), "Session is null".to_string());
        return Box::into_raw(Box::new(result));
    }
    if src.is_null() {
        let result = CREPLResult::error(String::new(), "Source is null".to_string());
        return Box::into_raw(Box::new(result));
    }

    let src = match unsafe { CStr::from_ptr(src) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            let result = CREPLResult::error(String::new(), "Invalid UTF-8 in source".to_string());
            return Box::into_raw(Box::new(result));
        }
    };

    let session = unsafe { &mut *session };
    let eval_result = session.eval(src);

    let result = if eval_result.success {
        // Keep output (println) and value separate
        let value_str = eval_result.value.as_ref().map(format_value);
        CREPLResult::success_with_value(eval_result.output, value_str)
    } else {
        CREPLResult::error(eval_result.output, eval_result.error.unwrap_or_default())
    };

    Box::into_raw(Box::new(result))
}

/// Reset a REPL session, clearing all variables and definitions.
#[no_mangle]
pub extern "C" fn repl_session_reset(session: *mut REPLSession) {
    if session.is_null() {
        return;
    }
    let session = unsafe { &mut *session };
    session.reset();
}

/// Free a REPL session.
#[no_mangle]
pub extern "C" fn repl_session_free(session: *mut REPLSession) {
    if !session.is_null() {
        unsafe {
            drop(Box::from_raw(session));
        }
    }
}

/// Free a CREPLResult.
#[no_mangle]
pub extern "C" fn free_repl_result(result: *mut CREPLResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let res = Box::from_raw(result);
        if !res.output.is_null() {
            drop(CString::from_raw(res.output));
        }
        if !res.value.is_null() {
            drop(CString::from_raw(res.value));
        }
        if !res.error.is_null() {
            drop(CString::from_raw(res.error));
        }
    }
}

/// Check if a Julia expression is complete or needs more input.
/// Returns 1 if the expression is complete (can be evaluated),
/// 0 if it appears incomplete (e.g., unclosed brackets, unfinished blocks).
/// Uses heuristic-based detection for unclosed brackets and blocks.
#[no_mangle]
pub extern "C" fn is_expression_complete(src: *const c_char) -> i32 {
    if src.is_null() {
        return 1;
    }
    let src = match unsafe { CStr::from_ptr(src) }.to_str() {
        Ok(s) => s,
        Err(_) => return 1, // Invalid UTF-8 = treat as complete
    };

    let trimmed = src.trim();
    if trimmed.is_empty() {
        return 1; // Empty is complete
    }

    // Check for unclosed brackets
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
        return 0; // Incomplete
    }

    // Check for unbalanced block keywords
    let keywords_open = [
        "function", "if", "for", "while", "try", "begin", "module", "struct", "macro", "let", "do",
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

    if depth > 0 {
        0
    } else {
        1
    }
}

/// Split Julia source code into top-level expressions.
/// Returns a JSON array of expression strings, or null on error.
/// The result must be freed with `free_string`.
#[no_mangle]
pub extern "C" fn split_expressions(src: *const c_char) -> *mut c_char {
    if src.is_null() {
        return std::ptr::null_mut();
    }
    let src = match unsafe { CStr::from_ptr(src) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    // Empty input returns empty array
    if src.trim().is_empty() {
        return CString::new("[]")
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut());
    }

    // Use the repl module's split logic (seed=0 for deterministic behavior)
    let session = REPLSession::new(0);
    let expressions: Vec<String> = match session.split_expressions(src) {
        Some(exprs) => exprs.into_iter().map(|(_, _, text)| text).collect(),
        None => {
            // No split needed, return the whole input as single expression
            vec![src.trim().to_string()]
        }
    };

    let json = match serde_json::to_string(&expressions) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    CString::new(json)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
