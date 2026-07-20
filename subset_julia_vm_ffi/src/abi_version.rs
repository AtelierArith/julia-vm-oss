//! C ABI version export (Issue #9001).
//!
//! Hosts must call `subset_julia_vm_abi_version()` at startup and compare the
//! returned value to the `SUBSET_VM_ABI_VERSION` macro from `subset_vm.h`.
//! Abort on mismatch rather than proceeding with a potentially incompatible ABI.

/// The C ABI version baked into this library build.
///
/// Must be kept identical to `SUBSET_VM_ABI_VERSION` in
/// `subset_julia_vm_ffi/include/subset_vm.h`. The `check_ffi_abi_version.sh`
/// audit script enforces this invariant in CI.
///
/// Bump when ANY of the following change:
/// * struct field layout or padding (`CSpan`, `CError`, `CExecutionResult`, `CREPLResult`)
/// * enum discriminant values (`CErrorKind`, `CValueKind`)
/// * function signatures (parameter types, return types, calling conventions)
/// * ownership/lifetime contracts (who allocates, who frees)
///
/// Do NOT bump for purely additive changes that leave existing consumers unaffected
/// (new functions that do not alter existing signatures or struct layouts).
pub const SUBSET_VM_C_ABI_VERSION: u32 = 3;

/// Return the C ABI version baked into this build of the native library.
///
/// Hosts must compare this value against the `SUBSET_VM_ABI_VERSION` macro they
/// compiled against and abort with a descriptive error on mismatch.  A mismatch
/// means the xcframework (or shared library) being loaded was built from a
/// different header revision than the binary referencing it.
#[no_mangle]
pub extern "C" fn subset_julia_vm_abi_version() -> u32 {
    SUBSET_VM_C_ABI_VERSION
}
