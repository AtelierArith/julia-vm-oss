# Issue 9089 Shared Planned IR Design

## Goal

Resolve Issue #9089 by making the stack-VM and register-VM lower from one
shared backend-neutral IR instead of having the register VM translate from
stack bytecode.

## Current State

The durable SSA model lives in `subset_julia_vm_compile/src/compile/ssa_ir/`.
`build_function` produces `SsaFunction`, optimization passes mutate that SSA
form, and `lower.rs` currently plans and emits stack `Instr` in one module.
The register VM still memoizes `RegisterProgram::from_stack_function(&Instr,
&FunctionInfo)` behind `SJULIA_REGISTER_VM=1`, so its input is the stack
bytecode stream rather than the SSA pipeline.

## Chosen Approach

Use the existing SSA lowering plan as the shared backend IR:

```text
Core IR -> SsaFunction -> SharedFunctionPlan -> stack Instr
                                      \------> register bytecode
```

`SharedFunctionPlan` is the public form of the current internal `FunctionPlan`.
It preserves the hard lowering decisions that both backends need:

- block order and terminator targets,
- phi edge copies, temp staging, and phi-copy coalescing,
- root assignments/discards,
- reconstructed Core `Expr` payloads for expression-shaped operations.

This is intentionally narrower than making register VM consume raw
`SsaFunction` immediately. Raw SSA would force the register backend to
duplicate phi-copy scheduling, edge copy interference handling, and spill
naming rules. The planned IR lets the first #9089 PR satisfy the architectural
contract while keeping the stack path behavior unchanged.

## Backend Contract

The shared plan module owns planning only. It must not emit stack `Instr`, run
peepholes, execute VM code, or depend on `vm/`. Backends consume the plan:

- the stack backend keeps using legacy `CoreCompiler` emitters to turn each
  planned root and terminator into `Instr`;
- the register backend lowers the same planned roots and terminators into
  `RegisterInstr`;
- unsupported register operations fail the whole function before execution,
  preserving the current total-or-explicit register-VM gate behavior.

## Initial Register Backend Scope

The first implementation slice covers the same shapes that the current
register VM already supports through stack bytecode:

- constants, local slot loads/stores, simple numeric operations and branches,
- direct calls through the existing stack trampoline,
- explicit returns,
- planned phi-edge assignments and jumps.

If a planned expression lowers through a stack-only construct, the register
backend returns an explicit ineligible reason and the existing gate runs the
function on the stack VM.

## Tests

Add tests before implementation:

- a compile-level shape test proving an SSA-built function has a shared plan
  available without requiring stack `Instr` as the register input;
- a register-gate test proving memoized register programs are built from
  shared plan metadata, not from `RegisterProgram::from_stack_function`;
- parity coverage through existing register VM parity fixtures with
  `SJULIA_REGISTER_VM=1`.

## Documentation

Update `docs/vm/SSA_IR.md`, `docs/vm/REGISTER_VM.md`, and
`docs/vm/ARCHITECTURE_OVERVIEW.md` to state that the shared planned IR is the
common backend input for stack/register lowering under Issue #9089.
