//! `.sjvmbc` VM bytecode execution FFI (Issue #10171).
//!
//! `./build.sh` precompiles every bundled iOS sample `.jl` to a sibling
//! `.sjvmbc` (Issue #9945). These entry points let the host execute such a
//! payload directly — skipping parse/lower/compile of the source — while
//! returning the same detailed result struct as `compile_and_run_detailed` /
//! `compile_and_run_streaming`.
//!
//! Invalidation contract (docs/vm/CACHE_ARCHITECTURE.md): ANY load failure
//! (bad magic, unsupported version, fingerprint mismatch once Issue #10170
//! lands, deserialize error) is reported with the distinct
//! `CErrorKind::StaleBytecode` and the host MUST treat it as a cache miss,
//! silently falling back to compiling the `.jl` source. It must never be
//! surfaced to the user.

// FFI functions intentionally take raw pointers and are called from C/Swift code.
// The caller is responsible for ensuring pointer validity.
#![allow(clippy::not_unsafe_ptr_arg_deref)]
// Issue #10906 (Phase 1c of #10869): the `.sjvmbc` FFI cache-load boundary —
// zero real unwrap_used/expect_used sites in production code (every match is
// inside the cfg(test) module, which carries an explicit allow). Every load
// failure already collapses onto `CErrorKind::StaleBytecode`, per this
// file's own invalidation-contract doc comment above.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use subset_julia_vm::cancel;
use subset_julia_vm::rng::StableRng;
use subset_julia_vm::vm::Vm;
use subset_julia_vm::vm_bytecode_file;

use super::detailed::{panic_execution_result, run_vm_to_boxed_result, OutputCallback};
use super::error::{CError, CExecutionResult};
use crate::panic_boundary;

/// Execute a `.sjvmbc` VM bytecode payload with detailed error information.
///
/// `bytes`/`len` describe the full file content (header + payload) of a
/// `.sjvmbc` produced by `sjulia --compile-vm`. Returns a heap-allocated
/// CExecutionResult that must be freed with `free_execution_result`. A load
/// failure yields `success == false` with `error.kind ==
/// CErrorKind::StaleBytecode` — the caller must fall back to source
/// compilation.
#[no_mangle]
pub extern "C" fn run_vm_bytecode_detailed(
    bytes_ptr: *const u8,
    len: usize,
    seed: u64,
) -> *mut CExecutionResult {
    panic_boundary::catch_unwind_ffi(panic_execution_result, || {
        run_bytecode_impl(bytes_ptr, len, seed, None)
    })
}

/// Execute a `.sjvmbc` VM bytecode payload with streaming output via callback.
///
/// Same contract as `run_vm_bytecode_detailed`; the callback receives each
/// output chunk during execution, mirroring `compile_and_run_streaming`.
#[no_mangle]
pub extern "C" fn run_vm_bytecode_streaming(
    bytes_ptr: *const u8,
    len: usize,
    seed: u64,
    context: *mut std::os::raw::c_void,
    output_callback: OutputCallback,
) -> *mut CExecutionResult {
    panic_boundary::catch_unwind_ffi(panic_execution_result, || {
        run_bytecode_impl(bytes_ptr, len, seed, Some((context, output_callback)))
    })
}

fn run_bytecode_impl(
    bytes_ptr: *const u8,
    len: usize,
    seed: u64,
    callback: Option<(*mut std::os::raw::c_void, OutputCallback)>,
) -> *mut CExecutionResult {
    if bytes_ptr.is_null() || len == 0 {
        let result = CExecutionResult::failure(
            String::new(),
            CError::stale_bytecode("Null or empty VM bytecode payload".to_string()),
        );
        return Box::into_raw(Box::new(result));
    }
    cancel::reset();

    let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, len) };

    // ANY VmBytecodeFileError maps to the stale-cache status so the host
    // falls back to source compilation (Issue #10171). #10170/#10328 added
    // exact-version + 3-fingerprint header validation to the shared
    // `load_from_reader`, so `load_from_bytes` rejects a stale/tampered bytes
    // payload through the same path a `--run-vm-bytecode` file load uses,
    // surfacing `VersionMismatch` / `FingerprintMismatch` (its
    // `is_stale_cache()` class) as well as `InvalidMagic` / `CorruptHeader` /
    // truncation / bincode errors. At THIS boundary every failure is a cache
    // miss — the bundled `.jl` source is always available as fallback — so we
    // deliberately collapse the whole error class (not just `is_stale_cache()`)
    // onto `StaleBytecode`.
    let compiled = match vm_bytecode_file::load_from_bytes(bytes) {
        Ok(compiled) => compiled,
        Err(e) => {
            let result = CExecutionResult::failure(
                String::new(),
                CError::stale_bytecode(format!("Failed to load VM bytecode: {}", e)),
            );
            return Box::into_raw(Box::new(result));
        }
    };

    // Post-deserialize hydration (Issue #10339): a bare `.sjvmbc` execution
    // would otherwise leave the thread-local promotion registry empty — the
    // Base-cache hit path replays promotion rules during deserialize, so a
    // source-compiled run sees a populated registry while runtime reflection
    // (vm/builtins_reflection promote-family builtins) in a bytecode-only run
    // would see an empty one. Warm the Base cache on this thread — the same
    // call the source-compile entry points make — so both paths execute with
    // identical registry state.
    subset_julia_vm::compile::host_support::warm_base_cache();

    let rng = StableRng::new(seed);
    let mut vm = Vm::new_program(compiled, rng);
    if let Some((context, output_callback)) = callback {
        vm.set_output_callback(output_callback, context);
    }
    // Match the source-compile entry points: the native/iOS Editor host
    // renders display artifacts (Issue #9262).
    vm.enable_graphical_display();

    run_vm_to_boxed_result(vm)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::error::{free_execution_result, CErrorKind};
    use std::ffi::CStr;

    fn run_with_large_stack<F: FnOnce() + Send + 'static>(f: F) {
        let builder = std::thread::Builder::new()
            .name("ffi-bytecode-test".into())
            .stack_size(16 * 1024 * 1024);
        let handler = builder.spawn(f).unwrap();
        if let Err(e) = handler.join() {
            std::panic::resume_unwind(e);
        }
    }

    /// Compile a source snippet to `.sjvmbc` file bytes, mirroring what
    /// `sjulia --compile-vm` writes and `./build.sh` bundles (Issue #9945).
    fn compile_to_bytecode_bytes(src: &str) -> Vec<u8> {
        let program = subset_julia_vm::pipeline::parse_and_lower_strict(src)
            .expect("test source must parse and lower");
        let compiled = subset_julia_vm::compile::host_support::compile_with_cache(&program)
            .expect("test source must compile");
        let path = std::env::temp_dir().join(format!(
            "sjulia_ffi_bytecode_test_{}_{:?}.sjvmbc",
            std::process::id(),
            std::thread::current().id()
        ));
        vm_bytecode_file::save(&program, &compiled, &path).expect("save must succeed");
        let bytes = std::fs::read(&path).expect("read back saved bytecode");
        let _ = std::fs::remove_file(&path);
        bytes
    }

    /// Valid payload executes and produces the same output as the source path.
    #[test]
    fn test_run_vm_bytecode_detailed_executes_valid_payload() {
        run_with_large_stack(|| {
            let bytes = compile_to_bytecode_bytes("println(\"bytecode ok\")\n1 + 2\n");
            let raw = run_vm_bytecode_detailed(bytes.as_ptr(), bytes.len(), 0);
            assert!(!raw.is_null(), "FFI returned null result pointer");
            unsafe {
                let result = &*raw;
                if !result.success && !result.error.message.is_null() {
                    let msg = CStr::from_ptr(result.error.message).to_string_lossy();
                    panic!("expected success for valid bytecode, got: {}", msg);
                }
                assert!(result.success);
                let output = CStr::from_ptr(result.output).to_string_lossy();
                assert_eq!(output, "bytecode ok\n");
                assert_eq!(result.result_value, 3.0);
            }
            free_execution_result(raw);
        });
    }

    /// A mismatched format version must be reported as StaleBytecode, never as
    /// a user-visible error kind — the host falls back to source compilation
    /// (Issues #10171/#10170). #10328 requires an *exact* version match, so
    /// bumping the version field by one is a mismatch.
    #[test]
    fn test_run_vm_bytecode_detailed_version_mismatch_is_stale() {
        run_with_large_stack(|| {
            let mut bytes = compile_to_bytecode_bytes("1 + 1\n");
            // Header layout (v4, #10170): magic(4) | version(4 LE) | flags(4) |
            // 3 length-prefixed fingerprints | payload_len(4) | payload. The
            // version field is still at offset 4..8.
            bytes[4..8].copy_from_slice(&(vm_bytecode_file::VERSION + 1).to_le_bytes());
            let raw = run_vm_bytecode_detailed(bytes.as_ptr(), bytes.len(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(!result.success, "tampered version must not execute");
                assert_eq!(result.error.kind, CErrorKind::StaleBytecode);
                let msg = CStr::from_ptr(result.error.message).to_string_lossy();
                assert!(
                    msg.contains("version"),
                    "message should mention the version mismatch: {}",
                    msg
                );
            }
            free_execution_result(raw);
        });
    }

    /// The bytes loader shares #10328's fingerprint validation path: a tampered
    /// header fingerprint (a stale payload from a different compiler build) must
    /// surface as StaleBytecode so the host falls back to source (Issue
    /// #10171/#10170/#10328). This is the composition guarantee — the bytes
    /// path is not a validation-bypassing shortcut around `load`.
    #[test]
    fn test_run_vm_bytecode_detailed_fingerprint_mismatch_is_stale() {
        run_with_large_stack(|| {
            let bytes = compile_to_bytecode_bytes("1 + 1\n");
            // First fingerprint (schema) is length-prefixed right after the
            // 12-byte magic+version+flags prefix: [len:u32 LE][len bytes...].
            // Flip the low bit of one content byte so the schema fingerprint no
            // longer matches this binary's, without disturbing the length prefix
            // and while keeping the byte ASCII (a hex char stays valid UTF-8, so
            // this is a genuine FingerprintMismatch, not a CorruptHeader).
            let fp_len = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
            assert!(fp_len > 0 && fp_len < 256, "unexpected fingerprint length");
            let mut tampered = bytes.clone();
            let first_fp_byte = 16;
            tampered[first_fp_byte] ^= 0x01;
            let raw = run_vm_bytecode_detailed(tampered.as_ptr(), tampered.len(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(!result.success, "tampered fingerprint must not execute");
                assert_eq!(result.error.kind, CErrorKind::StaleBytecode);
                let msg = CStr::from_ptr(result.error.message).to_string_lossy();
                assert!(
                    msg.contains("fingerprint"),
                    "message should mention the fingerprint mismatch: {}",
                    msg
                );
            }
            free_execution_result(raw);
        });
    }

    /// Garbage bytes (bad magic) → StaleBytecode.
    #[test]
    fn test_run_vm_bytecode_detailed_garbage_is_stale() {
        let bytes = b"definitely not a sjvmbc payload";
        let raw = run_vm_bytecode_detailed(bytes.as_ptr(), bytes.len(), 0);
        assert!(!raw.is_null());
        unsafe {
            let result = &*raw;
            assert!(!result.success);
            assert_eq!(result.error.kind, CErrorKind::StaleBytecode);
        }
        free_execution_result(raw);
    }

    /// Truncated payload (valid header prefix, cut body) → StaleBytecode.
    #[test]
    fn test_run_vm_bytecode_detailed_truncated_is_stale() {
        run_with_large_stack(|| {
            let bytes = compile_to_bytecode_bytes("1 + 1\n");
            let truncated = &bytes[..bytes.len() / 2];
            let raw = run_vm_bytecode_detailed(truncated.as_ptr(), truncated.len(), 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(!result.success, "truncated payload must not execute");
                assert_eq!(result.error.kind, CErrorKind::StaleBytecode);
            }
            free_execution_result(raw);
        });
    }

    /// Bit-flipped payload bytes past an otherwise-valid header (Issue #10908
    /// Phase 3 of #10869): every byte after the header is XORed, so the
    /// magic/version/fingerprint checks all pass but the body decode fails.
    /// The real FFI C ABI entry (not the internal `vm_bytecode_file` API a
    /// prior in-crate test already covered) must still surface
    /// `CErrorKind::StaleBytecode` rather than let a deserialize panic
    /// escape the `extern "C"` boundary.
    #[test]
    fn test_run_vm_bytecode_detailed_bit_flipped_body_is_stale() {
        run_with_large_stack(|| {
            let bytes = compile_to_bytecode_bytes(
                "function f(x)\n    x * 2 + 1\nend\n[f(i) for i in 1:5]\n",
            );
            // Flip every body byte after a generous header allowance so the
            // fixed-size magic/version/fingerprint prefix survives intact and
            // only the variable-length payload is corrupted.
            let header_len = (bytes.len() / 4).max(32).min(bytes.len());
            let mut corrupted = bytes.clone();
            for b in &mut corrupted[header_len..] {
                *b ^= 0xFF;
            }
            let raw = run_vm_bytecode_detailed(corrupted.as_ptr(), corrupted.len(), 0);
            assert!(
                !raw.is_null(),
                "must return a result, not null, for bit-flipped payload"
            );
            unsafe {
                let result = &*raw;
                assert!(!result.success, "bit-flipped payload must not execute");
                assert_eq!(result.error.kind, CErrorKind::StaleBytecode);
            }
            free_execution_result(raw);
        });
    }

    /// Null / empty input → StaleBytecode (fallback trigger), not a crash.
    #[test]
    fn test_run_vm_bytecode_detailed_null_and_empty_are_stale() {
        for (ptr, len) in [(std::ptr::null(), 16usize), (b"x".as_ptr(), 0usize)] {
            let raw = run_vm_bytecode_detailed(ptr, len, 0);
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(!result.success);
                assert_eq!(result.error.kind, CErrorKind::StaleBytecode);
            }
            free_execution_result(raw);
        }
    }

    /// Promotion-registry hydration canary (Issue #10339): execute the
    /// bytecode on a FRESH thread whose thread-local promotion registry is
    /// empty. `run_vm_bytecode_detailed` must hydrate (warm the Base cache)
    /// so promote-family reflection behaves like a source-compiled run.
    #[test]
    fn test_run_vm_bytecode_detailed_hydrates_promotion_registry_10339() {
        run_with_large_stack(|| {
            let bytes = compile_to_bytecode_bytes(
                "a = 1\nb = 2.5\nprintln(promote_type(typeof(a), typeof(b)))\nprintln(promote(a, b))\n",
            );

            // Execute on a brand-new thread: thread-local caches/registries
            // start empty there, unlike the thread that just compiled above.
            let builder = std::thread::Builder::new()
                .name("ffi-bytecode-fresh-thread".into())
                .stack_size(16 * 1024 * 1024);
            let bytes_for_thread = bytes.clone();
            let handler = builder
                .spawn(move || {
                    let raw = run_vm_bytecode_detailed(
                        bytes_for_thread.as_ptr(),
                        bytes_for_thread.len(),
                        0,
                    );
                    assert!(!raw.is_null());
                    let output = unsafe {
                        let result = &*raw;
                        if !result.success && !result.error.message.is_null() {
                            let msg = CStr::from_ptr(result.error.message).to_string_lossy();
                            panic!("expected success on fresh thread, got: {}", msg);
                        }
                        assert!(result.success);
                        CStr::from_ptr(result.output).to_string_lossy().to_string()
                    };
                    free_execution_result(raw);
                    output
                })
                .unwrap();
            let output = match handler.join() {
                Ok(output) => output,
                Err(e) => std::panic::resume_unwind(e),
            };
            assert_eq!(output, "Float64\n(1.0, 2.5)\n");
        });
    }

    /// The streaming variant delivers output through the callback and matches
    /// the accumulated output field, like compile_and_run_streaming.
    #[test]
    fn test_run_vm_bytecode_streaming_delivers_output() {
        run_with_large_stack(|| {
            let bytes = compile_to_bytecode_bytes("println(\"a\")\nprintln(\"b\")\n");

            extern "C" fn collect(
                context: *mut std::os::raw::c_void,
                output: *const std::os::raw::c_char,
            ) {
                let collected = unsafe { &mut *(context as *mut String) };
                let chunk = unsafe { CStr::from_ptr(output) }.to_string_lossy();
                collected.push_str(&chunk);
            }

            let mut collected = String::new();
            let raw = run_vm_bytecode_streaming(
                bytes.as_ptr(),
                bytes.len(),
                0,
                &mut collected as *mut String as *mut std::os::raw::c_void,
                collect,
            );
            assert!(!raw.is_null());
            unsafe {
                let result = &*raw;
                assert!(result.success, "streaming bytecode run should succeed");
                let output = CStr::from_ptr(result.output).to_string_lossy();
                assert_eq!(output, "a\nb\n");
            }
            assert_eq!(collected, "a\nb\n");
            free_execution_result(raw);
        });
    }
}
