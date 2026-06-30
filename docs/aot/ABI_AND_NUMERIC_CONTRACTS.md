# AoT ABI and Runtime Numeric Contracts

**Last updated**: 2026-06-24 (Issues #7077, #7056)

This document defines AoT boundary contracts that must remain stable before the
Rust and Cranelift backends enable additional runtime-shaped values. It is a
design contract and gate policy, not a claim that all carriers below are enabled
in generated code today.

## C ABI Export Returns

Issue #7077 owns C-stable exports for values that are not plain scalars. The
current enabled export surface remains scalar and `Nothing`; non-scalar returns
must keep producing an explicit diagnostic until their runtime wrappers are
implemented.

The selected C ABI return shapes are:

| Julia return | C-stable shape | Ownership |
|---|---|---|
| `String` | `SjuliaStringView { data: *const u8, len: u64 }` for borrowed immutable data, or `SjuliaOwnedString*` for owned heap results | Borrowed views are valid only until the next runtime call on the same context. Owned handles are released by runtime API. |
| `Array{T,N}` / `Vector{T}` | `SjuliaArrayView { data: *const u8, len: u64, elem_tag: u32, ndims: u32, dims: *const u64 }` for borrowed immutable views, or `SjuliaArrayHandle*` for owned/mutable results | Borrowed views cannot cross safepoints. Owned handles are rooted in the runtime context. |
| scalar-field immutable `struct` | Caller-provided out-parameter with a generated `repr(C)` layout descriptor | Caller owns storage; callee writes exactly once. |
| heap/runtime `struct`, `Any`, multi-variant `Union` | Opaque `SjuliaValue*` runtime handle | Runtime owns object identity and rooting. |

Export wrappers must not expose Rust `String`, `Vec<T>`, Rust enum, or Rust
struct layout directly. Every exported symbol either uses the current scalar ABI
or one of the shapes above. If a return type would need a shape whose helper is
not implemented, AoT must reject it at export planning time rather than emit an
unstable ABI.

### Export Failure and Error Model

Export wrappers that can allocate or throw use the same status-bearing runtime
boundary as the Cranelift exception contract:

```text
SjuliaCallStatus = u32
SJULIA_CALL_OK = 0
SJULIA_CALL_EXCEPTION = 1
```

For Rust backend generated exports, this status may be represented by a wrapper
struct or an out-parameter plus status result. For Cranelift exports, the status
is the leading native result or C ABI status result described in
`CRANELIFT_GC_ROOTING_CONTRACT.md`.

## BigInt / BigFloat / Rational / Irrational

Issue #7056 owns AoT handling for arbitrary precision and symbolic numeric
families. The selected policy is explicit runtime-backed representation, not
lossy primitive lowering.

| Julia family | AoT policy |
|---|---|
| `BigInt` | Runtime numeric handle backed by arbitrary-precision integer storage. Never lower to `Int128` or host pointer-sized integer silently. |
| `BigFloat` | Runtime numeric handle with precision/rounding metadata. Never lower to `Float64` silently. |
| `Rational{T}` | Native two-field stack aggregate only when `T` is a supported native integer and the value never crosses a runtime/ABI boundary; otherwise runtime numeric handle. |
| `Irrational{:sym}` | Singleton/runtime numeric handle that may convert explicitly at arithmetic boundaries but is not silently materialized as `Float64`. |

The Rust backend may continue to use runtime `Value` operations for VM-compatible
dynamic numeric behavior. `--pure-rust` and C ABI export must reject these
families until the runtime handle helpers and C-stable ownership rules are
available.

### Required Gates

AoT must reject, with a span-bearing diagnostic, any use that would otherwise
silently narrow these values:

- `BigInt` to fixed-width integer;
- `BigFloat` to `Float64` / `Float32`;
- `Irrational` to a float literal without an explicit conversion boundary;
- `Rational` with non-native numerator/denominator carrier;
- export of any runtime numeric handle through scalar C ABI.

This keeps generated code from depending on host precision or Rust layout while
leaving a clear path for future runtime helper implementations.
