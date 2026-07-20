//! Panic containment helpers for exported C ABI functions.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub fn catch_unwind_ffi<T, F, B>(fallback: F, body: B) -> T
where
    F: FnOnce(String) -> T,
    B: FnOnce() -> T,
{
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => fallback(panic_payload_message(payload)),
    }
}

pub fn panic_payload_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

pub fn ffi_panic_message(payload: String) -> String {
    format!("Rust panic caught at FFI boundary: {payload}")
}
