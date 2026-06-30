//! C-compatible error types for FFI.
//!
//! These structs provide detailed error information with source spans.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use subset_julia_vm::error::SyntaxError;
use subset_julia_vm::span::Span;
use subset_julia_vm::vm::Value;

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

/// Stable type tag for the structured JSON value attached to CExecutionResult.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CValueKind {
    Unknown = 0,
    Nothing = 1,
    Missing = 2,
    Bool = 3,
    Int = 4,
    UInt = 5,
    Float = 6,
    String = 7,
    Char = 8,
    Complex = 9,
    Array = 10,
    Dict = 11,
    Tuple = 12,
    NamedTuple = 13,
    Struct = 14,
    Symbol = 15,
    Range = 16,
    Enum = 17,
    Artifact = 18,
    Opaque = 19,
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
    // Display artifact (e.g. SVG plot). Both null when no artifact is produced.
    pub artifact_mime: *mut c_char,
    pub artifact_data: *mut c_char,
    // Structured typed VM value JSON. Owned by this result and freed with it.
    pub value_json: *mut c_char,
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
            artifact_mime: std::ptr::null_mut(),
            artifact_data: std::ptr::null_mut(),
            value_json: std::ptr::null_mut(),
        }
    }

    pub fn success_with_value(
        value: &Value,
        struct_heap: &[subset_julia_vm::vm::StructInstance],
        output: String,
    ) -> Self {
        let result_value = subset_julia_vm::ffi_support::legacy_numeric_result_value(value);
        let value_json = subset_julia_vm::ffi_support::typed_value_json_string(value, struct_heap);
        let mut result = Self::success(result_value, output);
        result.value_json = CString::new(value_json)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut());
        result
    }

    /// Attach a display artifact to a previously-built result. Replaces any
    /// existing artifact pointers (which the caller must have ownership of).
    pub fn with_artifact(
        mut self,
        artifact: Option<subset_julia_vm::plotting::DisplayArtifact>,
    ) -> Self {
        if let Some(a) = artifact {
            self.artifact_mime = CString::new(a.mime)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut());
            self.artifact_data = CString::new(a.data)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut());
        }
        self
    }

    pub fn failure(output: String, error: CError) -> Self {
        CExecutionResult {
            success: false,
            result_value: f64::NAN,
            output: CString::new(output)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut()),
            error,
            artifact_mime: std::ptr::null_mut(),
            artifact_data: std::ptr::null_mut(),
            value_json: std::ptr::null_mut(),
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
        if !res.artifact_mime.is_null() {
            drop(CString::from_raw(res.artifact_mime));
        }
        if !res.artifact_data.is_null() {
            drop(CString::from_raw(res.artifact_data));
        }
        if !res.value_json.is_null() {
            drop(CString::from_raw(res.value_json));
        }
    }
}

/// Return a borrowed pointer to the typed value JSON.
///
/// The pointer is owned by `result` and remains valid until
/// `free_execution_result(result)` is called. Do not pass it to free_string().
#[no_mangle]
pub extern "C" fn execution_result_value_json(result: *const CExecutionResult) -> *const c_char {
    if result.is_null() {
        return std::ptr::null();
    }
    unsafe { (*result).value_json as *const c_char }
}

#[no_mangle]
pub extern "C" fn execution_result_value_kind(result: *const CExecutionResult) -> CValueKind {
    value_json(result)
        .and_then(|value| {
            value
                .get("type")
                .and_then(|kind| kind.as_str())
                .map(kind_from_str)
        })
        .unwrap_or(CValueKind::Unknown)
}

#[no_mangle]
pub extern "C" fn execution_result_complex_real(result: *const CExecutionResult) -> f64 {
    number_field(result, "real")
}

#[no_mangle]
pub extern "C" fn execution_result_complex_imag(result: *const CExecutionResult) -> f64 {
    number_field(result, "imag")
}

#[no_mangle]
pub extern "C" fn execution_result_array_len(result: *const CExecutionResult) -> u64 {
    value_json(result)
        .and_then(|value| value.get("length").and_then(|length| length.as_u64()))
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn execution_result_array_element_kind(
    result: *const CExecutionResult,
    index: u64,
) -> CValueKind {
    array_element_json(result, index)
        .and_then(|value| {
            value
                .get("type")
                .and_then(|kind| kind.as_str())
                .map(kind_from_str)
        })
        .unwrap_or(CValueKind::Unknown)
}

#[no_mangle]
pub extern "C" fn execution_result_array_element_f64(
    result: *const CExecutionResult,
    index: u64,
) -> f64 {
    array_element_json(result, index)
        .and_then(json_numeric_value)
        .unwrap_or(f64::NAN)
}

/// Return an owned JSON string for an array element. Free with free_string().
#[no_mangle]
pub extern "C" fn execution_result_array_element_json(
    result: *const CExecutionResult,
    index: u64,
) -> *mut c_char {
    owned_json_string(array_element_json(result, index))
}

#[no_mangle]
pub extern "C" fn execution_result_dict_len(result: *const CExecutionResult) -> u64 {
    value_json(result)
        .and_then(|value| value.get("length").and_then(|length| length.as_u64()))
        .unwrap_or(0)
}

/// Return an owned JSON string for a dictionary key. Free with free_string().
#[no_mangle]
pub extern "C" fn execution_result_dict_key_json(
    result: *const CExecutionResult,
    index: u64,
) -> *mut c_char {
    owned_json_string(dict_entry_json(result, index).and_then(|entry| entry.get("key").cloned()))
}

/// Return an owned JSON string for a dictionary value. Free with free_string().
#[no_mangle]
pub extern "C" fn execution_result_dict_value_json(
    result: *const CExecutionResult,
    index: u64,
) -> *mut c_char {
    owned_json_string(dict_entry_json(result, index).and_then(|entry| entry.get("value").cloned()))
}

/// Return a borrowed artifact MIME pointer, or NULL when no artifact exists.
#[no_mangle]
pub extern "C" fn execution_result_artifact_mime(result: *const CExecutionResult) -> *const c_char {
    if result.is_null() {
        return std::ptr::null();
    }
    unsafe { (*result).artifact_mime as *const c_char }
}

/// Return a borrowed artifact data pointer, or NULL when no artifact exists.
#[no_mangle]
pub extern "C" fn execution_result_artifact_data(result: *const CExecutionResult) -> *const c_char {
    if result.is_null() {
        return std::ptr::null();
    }
    unsafe { (*result).artifact_data as *const c_char }
}

fn value_json(result: *const CExecutionResult) -> Option<serde_json::Value> {
    if result.is_null() {
        return None;
    }
    let ptr = unsafe { (*result).value_json };
    if ptr.is_null() {
        return None;
    }
    let json = unsafe { CStr::from_ptr(ptr) }.to_str().ok()?;
    serde_json::from_str(json).ok()
}

fn kind_from_str(kind: &str) -> CValueKind {
    match kind {
        "nothing" => CValueKind::Nothing,
        "missing" => CValueKind::Missing,
        "bool" => CValueKind::Bool,
        "int" => CValueKind::Int,
        "uint" => CValueKind::UInt,
        "float" => CValueKind::Float,
        "string" => CValueKind::String,
        "char" => CValueKind::Char,
        "complex" => CValueKind::Complex,
        "array" => CValueKind::Array,
        "dict" => CValueKind::Dict,
        "tuple" => CValueKind::Tuple,
        "named_tuple" => CValueKind::NamedTuple,
        "struct" => CValueKind::Struct,
        "symbol" => CValueKind::Symbol,
        "range" => CValueKind::Range,
        "enum" => CValueKind::Enum,
        "artifact" => CValueKind::Artifact,
        "opaque" => CValueKind::Opaque,
        _ => CValueKind::Unknown,
    }
}

fn number_field(result: *const CExecutionResult, field: &str) -> f64 {
    value_json(result)
        .and_then(|value| value.get(field).cloned())
        .and_then(json_number)
        .unwrap_or(f64::NAN)
}

fn array_element_json(result: *const CExecutionResult, index: u64) -> Option<serde_json::Value> {
    Some(
        value_json(result)?
            .get("elements")?
            .as_array()?
            .get(usize::try_from(index).ok()?)?
            .clone(),
    )
}

fn dict_entry_json(result: *const CExecutionResult, index: u64) -> Option<serde_json::Value> {
    Some(
        value_json(result)?
            .get("entries")?
            .as_array()?
            .get(usize::try_from(index).ok()?)?
            .clone(),
    )
}

fn json_numeric_value(value: serde_json::Value) -> Option<f64> {
    match value.get("type")?.as_str()? {
        "complex" => value.get("real").cloned().and_then(json_number),
        "int" | "uint" | "float" | "enum" => value.get("value").cloned().and_then(json_number),
        "bool" => value
            .get("value")
            .and_then(|v| v.as_bool())
            .map(|v| if v { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn json_number(value: serde_json::Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    match value.as_str()? {
        "NaN" => Some(f64::NAN),
        "Inf" => Some(f64::INFINITY),
        "-Inf" => Some(f64::NEG_INFINITY),
        other => other.parse::<f64>().ok(),
    }
}

fn owned_json_string(value: Option<serde_json::Value>) -> *mut c_char {
    match value.and_then(|value| CString::new(value.to_string()).ok()) {
        Some(value) => value.into_raw(),
        None => std::ptr::null_mut(),
    }
}
