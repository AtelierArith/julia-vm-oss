//! Unicode completion FFI functions.
//!
//! These functions provide C ABI for LaTeX-to-Unicode conversion.
//! Note: These are for iOS. WASM uses separate bindings in subset_julia_vm_web.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::unicode;

/// Look up a LaTeX command and return its Unicode representation.
/// Returns a heap-allocated C string that must be freed with `free_string`, or null if not found.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn unicode_lookup(latex_ptr: *const c_char) -> *mut c_char {
    if latex_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let latex = match unsafe { CStr::from_ptr(latex_ptr) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match unicode::latex_to_unicode(latex) {
        Some(unicode) => CString::new(unicode)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// Get completions for a LaTeX prefix.
/// Returns a JSON array of [latex, unicode] pairs, or null on error.
/// The result must be freed with `free_string`.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn unicode_completions(prefix_ptr: *const c_char) -> *mut c_char {
    if prefix_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let prefix = match unsafe { CStr::from_ptr(prefix_ptr) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let completions = unicode::completions_for_prefix(prefix);
    let pairs: Vec<(&str, &str)> = completions.into_iter().collect();

    match serde_json::to_string(&pairs) {
        Ok(json) => CString::new(json)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Expand all LaTeX sequences in a string to their Unicode equivalents.
/// Returns a heap-allocated C string that must be freed with `free_string`.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn unicode_expand(input_ptr: *const c_char) -> *mut c_char {
    if input_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let input = match unsafe { CStr::from_ptr(input_ptr) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let expanded = unicode::expand_latex_in_string(input);
    CString::new(expanded)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Reverse lookup: get LaTeX for a Unicode character.
/// Returns a heap-allocated C string that must be freed with `free_string`, or null if not found.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn unicode_reverse_lookup(unicode_ptr: *const c_char) -> *mut c_char {
    if unicode_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let unicode = match unsafe { CStr::from_ptr(unicode_ptr) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match unicode::unicode_to_latex(unicode) {
        Some(latex) => CString::new(latex)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}
