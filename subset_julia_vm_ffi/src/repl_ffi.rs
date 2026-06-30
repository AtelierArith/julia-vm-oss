//! REPL session FFI functions.
//!
//! These functions provide a C ABI for interactive REPL sessions.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use super::format::format_value_with_struct_heap;
use subset_julia_vm::repl::REPLSession;

/// C-compatible REPL evaluation result
#[repr(C)]
pub struct CREPLResult {
    pub success: bool,
    pub output: *mut c_char, // println/print output only
    pub value: *mut c_char,  // formatted result value (separate from output)
    pub error: *mut c_char,
    pub artifact_mime: *mut c_char, // MIME type of display artifact, or null
    pub artifact_data: *mut c_char, // display artifact data (e.g., SVG string), or null
}

fn str_to_raw(s: String) -> *mut c_char {
    CString::new(s)
        .map(|cs| cs.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

impl CREPLResult {
    fn success_with_value(
        output: String,
        value: Option<String>,
        artifact: Option<subset_julia_vm::plotting::DisplayArtifact>,
    ) -> Self {
        let (mime_ptr, data_ptr) = match artifact {
            Some(a) => (str_to_raw(a.mime), str_to_raw(a.data)),
            None => (std::ptr::null_mut(), std::ptr::null_mut()),
        };
        CREPLResult {
            success: true,
            output: str_to_raw(output),
            value: value.map(str_to_raw).unwrap_or(std::ptr::null_mut()),
            error: std::ptr::null_mut(),
            artifact_mime: mime_ptr,
            artifact_data: data_ptr,
        }
    }

    fn error(output: String, error_msg: String) -> Self {
        CREPLResult {
            success: false,
            output: str_to_raw(output),
            value: std::ptr::null_mut(),
            error: str_to_raw(error_msg),
            artifact_mime: std::ptr::null_mut(),
            artifact_data: std::ptr::null_mut(),
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
        // When a display artifact (e.g. SVG plot) is attached, the chart already
        // carries the meaning of the return value; surfacing the textual struct
        // form alongside it (e.g. `<struct ref>` or
        // `Plots.Plot([StructRef(heap_idx=N)], :text)`) is noise. Issue #4369.
        let value_str = if eval_result.display_artifact.is_some() {
            None
        } else if let Some(display) = eval_result.value_display.clone() {
            // Prefer the result's user-defined `show` rendering (Issue #7168).
            Some(display)
        } else {
            eval_result
                .value
                .as_ref()
                .map(|value| format_value_with_struct_heap(value, session.get_struct_heap()))
        };
        CREPLResult::success_with_value(eval_result.output, value_str, eval_result.display_artifact)
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
        if !res.artifact_mime.is_null() {
            drop(CString::from_raw(res.artifact_mime));
        }
        if !res.artifact_data.is_null() {
            drop(CString::from_raw(res.artifact_data));
        }
    }
}

/// Return a borrowed REPL artifact MIME pointer, or NULL when no artifact exists.
#[no_mangle]
pub extern "C" fn repl_result_artifact_mime(result: *const CREPLResult) -> *const c_char {
    if result.is_null() {
        return std::ptr::null();
    }
    unsafe { (*result).artifact_mime as *const c_char }
}

/// Return borrowed REPL artifact data, or NULL when no artifact exists.
#[no_mangle]
pub extern "C" fn repl_result_artifact_data(result: *const CREPLResult) -> *const c_char {
    if result.is_null() {
        return std::ptr::null();
    }
    unsafe { (*result).artifact_data as *const c_char }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        let builder = std::thread::Builder::new()
            .name("repl-ffi-test".into())
            .stack_size(16 * 1024 * 1024);
        let handler = builder.spawn(f).unwrap();
        if let Err(e) = handler.join() {
            std::panic::resume_unwind(e);
        }
    }

    /// Issue #4369: when a Plot SVG artifact is attached, the textual
    /// representation of the underlying StructRef (`<struct ref>` or
    /// `Plots.Plot([StructRef(heap_idx=N)], :text)`) is redundant noise.
    /// `CREPLResult.value` must be null whenever `artifact_data` is set.
    #[test]
    fn test_repl_eval_suppresses_value_text_when_plot_artifact_present() {
        run_with_large_stack(|| {
            unsafe {
                let session = repl_session_new(0);
                assert!(!session.is_null());

                // Setup
                let setup = CString::new("using Plots").unwrap();
                let setup_res = repl_session_eval(session, setup.as_ptr());
                assert!((*setup_res).success);
                free_repl_result(setup_res);

                // plot(sin) — should produce SVG artifact and null textual value
                let src = CString::new("plot(sin)").unwrap();
                let res = repl_session_eval(session, src.as_ptr());
                assert!((*res).success);
                assert!(
                    !(*res).artifact_data.is_null(),
                    "expected SVG artifact for plot(sin)"
                );
                assert!(
                    (*res).value.is_null(),
                    "CREPLResult.value should be null when an SVG artifact is attached, got: {}",
                    CStr::from_ptr((*res).value).to_string_lossy()
                );
                free_repl_result(res);

                repl_session_free(session);
            }
        });
    }

    #[test]
    fn test_repl_eval_import_only_inputs_return_no_value_6000() {
        run_with_large_stack(|| unsafe {
            let session = repl_session_new(0);
            assert!(!session.is_null());

            for src in ["using LinearAlgebra", "using Plots"] {
                let code = CString::new(src).unwrap();
                let res = repl_session_eval(session, code.as_ptr());
                assert!((*res).success, "`{src}` failed");
                assert!(
                    (*res).value.is_null(),
                    "`{src}` should not surface a formatted value through FFI"
                );
                assert!(
                    (*res).artifact_data.is_null(),
                    "`{src}` should not attach a display artifact"
                );
                free_repl_result(res);
            }

            let src = CString::new("norm([3.0, 4.0])").unwrap();
            let res = repl_session_eval(session, src.as_ptr());
            assert!((*res).success);
            assert!(!(*res).value.is_null());
            let value = CStr::from_ptr((*res).value).to_string_lossy();
            assert_eq!(value, "5.0");
            free_repl_result(res);

            repl_session_free(session);
        });
    }

    #[test]
    fn test_repl_eval_surface_matrix_returns_plotly_artifact_5987() {
        run_with_large_stack(|| unsafe {
            let session = repl_session_new(0);
            assert!(!session.is_null());

            for src in [
                "using LinearAlgebra",
                "using Plots",
                "x = y = range(-3, stop = 3, length = 4)",
                "z = [sinc(norm([xi, yi])) for yi in y, xi in x]",
            ] {
                let code = CString::new(src).unwrap();
                let res = repl_session_eval(session, code.as_ptr());
                assert!((*res).success, "`{src}` failed");
                free_repl_result(res);
            }

            let src = CString::new("surface(x, y, z)").unwrap();
            let res = repl_session_eval(session, src.as_ptr());
            assert!((*res).success);
            assert!(
                !(*res).artifact_mime.is_null(),
                "expected Plotly MIME for surface artifact"
            );
            assert!(
                !(*res).artifact_data.is_null(),
                "expected Plotly JSON for surface artifact"
            );
            assert!(
                (*res).value.is_null(),
                "CREPLResult.value should be null when a surface artifact is attached, got: {}",
                CStr::from_ptr((*res).value).to_string_lossy()
            );

            let mime = CStr::from_ptr((*res).artifact_mime).to_string_lossy();
            assert_eq!(mime, "application/vnd.plotly+json");
            let data = CStr::from_ptr((*res).artifact_data).to_string_lossy();
            assert!(
                data.contains(r#""type":"surface""#),
                "surface should emit a Plotly surface trace, got: {data}"
            );
            free_repl_result(res);

            repl_session_free(session);
        });
    }

    #[test]
    fn test_repl_eval_linrange_struct_ref_formats_range_6123() {
        run_with_large_stack(|| unsafe {
            let session = repl_session_new(0);
            assert!(!session.is_null());

            let src = CString::new("x = y = range(-3, stop = 3, length = 100)").unwrap();
            let res = repl_session_eval(session, src.as_ptr());
            assert!((*res).success);
            assert!(
                !(*res).value.is_null(),
                "expected formatted range value for LinRange assignment"
            );

            let value = CStr::from_ptr((*res).value).to_string_lossy();
            assert_eq!(value, "-3.0:0.06060606060606061:3.0");
            assert!(
                !value.contains("struct ref"),
                "REPL should not leak StructRef internals, got: {value}"
            );

            free_repl_result(res);
            repl_session_free(session);
        });
    }

    /// Non-plot results must still surface the formatted textual value.
    #[test]
    fn test_repl_eval_keeps_value_text_for_plain_results() {
        run_with_large_stack(|| unsafe {
            let session = repl_session_new(0);
            let src = CString::new("1 + 2").unwrap();
            let res = repl_session_eval(session, src.as_ptr());
            assert!((*res).success);
            assert!((*res).artifact_data.is_null());
            assert!(
                !(*res).value.is_null(),
                "value text should remain for non-plot results"
            );
            let value = CStr::from_ptr((*res).value).to_string_lossy();
            assert_eq!(value, "3");
            free_repl_result(res);
            repl_session_free(session);
        });
    }
}
