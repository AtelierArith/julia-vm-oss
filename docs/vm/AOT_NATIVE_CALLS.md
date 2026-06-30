# AoT Native Call Boundary

**Last updated**: 2026-05-11 (Issue #3723)

Julia treats `ccall` and `Core.Intrinsics.llvmcall` as codegen-boundary
constructs. sjulia AoT does the same: these forms are classified explicitly and
must not pass through Rust or Cranelift codegen as ordinary function calls.

## Current Policy

- `ccall(...)` is rejected during AoT Core IR conversion.
- `llvmcall(...)` and `Core.Intrinsics.llvmcall(...)` are rejected during AoT
  Core IR conversion.
- The named pass verifier rejects any manually constructed AoT IR whose call
  target is `ccall`, `llvmcall`, or a qualified `*.ccall` / `*.llvmcall`.

This first step intentionally supports no `ccall` subset. That is safer than
letting an unsupported signature become a dynamic call or backend placeholder.

## Diagnostic Contract

Rejected boundary forms include the source span when Core IR has one, for
example `line 3, column 5`, and explain the missing boundary:

- `ccall`: static signature validation and native ABI lowering are not
  implemented yet.
- `llvmcall`: arbitrary LLVM IR is rejected unless a backend explicitly
  supports a safe subset.

## Future Supported Path

Any future accepted `ccall` path must carry a typed `AotCallAbi` through the
backend-neutral ABI boundary. Ad-hoc string signatures or raw backend placeholder
types are not acceptable.
