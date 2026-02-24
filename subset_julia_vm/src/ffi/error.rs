//! C-compatible error types for FFI.
//!
//! These structs provide detailed error information with source spans.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::CString;
use std::os::raw::c_char;

use crate::error::SyntaxError;
use crate::span::Span;

/// C-compatible span struct for error location
#[repr(C)]
pub struct CSpan {
    pub start: u32,
    pub end: u32,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
}

impl CSpan {
    pub fn from_span(span: &Span) -> Self {
        CSpan {
            start: span.start as u32,
            end: span.end as u32,
            start_line: span.start_line as u32,
            end_line: span.end_line as u32,
            start_column: span.start_column as u32,
            end_column: span.end_column as u32,
        }
    }

    pub fn empty() -> Self {
        CSpan {
            start: 0,
            end: 0,
            start_line: 0,
            end_line: 0,
            start_column: 0,
            end_column: 0,
        }
    }
}

pub fn syntax_error_span(error: &SyntaxError) -> Option<CSpan> {
    match error {
        SyntaxError::ErrorNodes(issues) => {
            issues.first().map(|issue| CSpan::from_span(&issue.span))
        }
        SyntaxError::ParseFailed(_) => None,
    }
}

/// Error kind enum for FFI
#[repr(C)]
pub enum CErrorKind {
    None = 0,
    Syntax = 1,
    Unsupported = 2,
    Runtime = 3,
    Compile = 4,
}

/// C-compatible error struct
#[repr(C)]
pub struct CError {
    pub kind: CErrorKind,
    pub span: CSpan,
    pub message: *mut c_char,
    pub hint: *mut c_char,
}

impl CError {
    pub fn none() -> Self {
        CError {
            kind: CErrorKind::None,
            span: CSpan::empty(),
            message: std::ptr::null_mut(),
            hint: std::ptr::null_mut(),
        }
    }

    pub fn syntax(message: String, span: Option<CSpan>) -> Self {
        CError {
            kind: CErrorKind::Syntax,
            span: span.unwrap_or_else(CSpan::empty),
            message: CString::new(message)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            hint: std::ptr::null_mut(),
        }
    }

    pub fn unsupported(message: String, span: CSpan, hint: Option<String>) -> Self {
        CError {
            kind: CErrorKind::Unsupported,
            span,
            message: CString::new(message)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            hint: hint
                .and_then(|h| CString::new(h).ok())
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
        }
    }

    pub fn runtime(message: String) -> Self {
        CError {
            kind: CErrorKind::Runtime,
            span: CSpan::empty(),
            message: CString::new(message)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            hint: std::ptr::null_mut(),
        }
    }

    pub fn compile(message: String) -> Self {
        CError {
            kind: CErrorKind::Compile,
            span: CSpan::empty(),
            message: CString::new(message)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            hint: std::ptr::null_mut(),
        }
    }
}

/// C-compatible execution result
#[repr(C)]
pub struct CExecutionResult {
    pub success: bool,
    pub result_value: f64,
    pub output: *mut c_char,
    pub error: CError,
}

impl CExecutionResult {
    pub fn success(value: f64, output: String) -> Self {
        CExecutionResult {
            success: true,
            result_value: value,
            output: CString::new(output)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            error: CError::none(),
        }
    }

    pub fn failure(output: String, error: CError) -> Self {
        CExecutionResult {
            success: false,
            result_value: f64::NAN,
            output: CString::new(output)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            error,
        }
    }
}

/// Free a CExecutionResult allocated by compile_and_run_detailed
#[no_mangle]
pub extern "C" fn free_execution_result(result: *mut CExecutionResult) {
    if result.is_null() {
        return;
    }
    unsafe {
        let res = Box::from_raw(result);
        if !res.output.is_null() {
            drop(CString::from_raw(res.output));
        }
        if !res.error.message.is_null() {
            drop(CString::from_raw(res.error.message));
        }
        if !res.error.hint.is_null() {
            drop(CString::from_raw(res.error.hint));
        }
    }
}
