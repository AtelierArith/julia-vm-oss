# AoT Generated-Rust Ownership Conventions Design

**Issue:** #11202

**Parent:** #10815

**Motivating bug:** #10663

## Goal

Make generated-Rust ownership choices reviewable before an incidental `rustc`
failure. Document when AoT codegen may borrow, clone, or move a Julia value and
prove that the existing generated-Rust compile harness detects #10663's
two-dynamic-call reuse pattern.

This issue does not fix #10663. It installs the convention and a negative
control that #10663 must convert into a positive compile regression when the
codegen defect is fixed.

## Chosen approach

Use a targeted compile negative control built from the real #10663 Julia
source. The test runs the normal AoT lowering/inference/codegen pipeline, writes
the generated Rust as a temporary Cargo crate, and asserts that `cargo check`
detects the current E0382 moved-value failure. It also checks that both
`dynamic_call("iterate", ...)` call sites are present, so an unrelated compile
failure cannot satisfy the test.

The existing generated-Rust helpers are refactored around one function that
returns `std::process::Output`. Existing positive compile and `-D warnings`
assertions keep their behavior; the new negative control can inspect the same
compiler result without duplicating temporary-crate setup.

When #10663 lands, its fix must replace the negative assertion with
`assert_generated_rust_compiles` (or the warnings-denied variant). Thereafter a
move regression makes the test red.

### Alternatives rejected

1. **Source grep for bare variables in template strings.** The templates do
   not contain the binding liveness facts needed to distinguish a safe last-use
   move from a later reuse. Such a grep would either miss composition or flag
   every dynamic call.
2. **Compile every AoT E2E source as generated Rust.** This is a useful future
   widening, but the current test file has hundreds of independent cases and no
   registry of source programs. Retrofitting all of them is larger than this
   documentation/audit issue. The targeted control closes the named #10663 gap
   without creating a second test orchestration system.

## Normative ownership model

The generated program must preserve Julia value availability and aliasing;
passing a value to a Julia call does not consume its source binding.

1. **Borrow** when the callee accepts a reference and does not need ownership.
   `dynamic_binop` is the model: it takes `&Value` operands, so a bare binding
   remains usable after the call.
2. **Clone** only when a callee requires owned input and the clone preserves
   Julia semantics. A runtime `Value` binding passed through an owned argument
   list must be cloned when it may be used later. `Value::Array` and
   `Value::Dict` clones preserve shared `Rc<RefCell<_>>` handles; immutable
   payloads may be copied structurally.
3. **Move** only for a freshly produced rvalue or a last use proven by a
   binding-identity-aware liveness analysis. Source order, variable spelling,
   or the fact that a helper currently returns an owned type is not proof of
   last use.
4. **Do not deep-clone mutable native representations.** A typed `Vec<T>` or a
   generated mutable struct may have a Rust `Clone` implementation whose copy
   does not preserve Julia aliasing. Such arguments need a borrowed/shared
   handle ABI or an explicit unsupported diagnostic; adding `.clone()` is not
   a general fix.
5. **Transfers are explicit.** Assignment into a new owner and function return
   may transfer a temporary. Mutation receivers, call arguments, and values
   read again after a call follow the rules above.

## Codegen review matrix

| Generated site | Current Rust API shape | Required review decision |
|---|---|---|
| `dynamic_binop(op, &lhs, &rhs)` | borrowed operands | Borrow; temporaries live through the call expression. |
| runtime `dynamic_call(name, &[...])` | slice elements are owned `Value`s | Clone reusable `Value` bindings; move only rvalues or proven last uses. #10663 is the known violation. |
| generated dispatch/static function call | parameters are passed by value today | Copy native scalars; otherwise require semantic-preserving clone, borrowed/shared ABI, or proven last use. |
| builtin/runtime helper | helper-specific | Read the helper signature and classify each argument; do not infer ownership from the template text alone. |
| assignment/return | ownership transfer | Move a temporary; preserve a source binding if it remains live. |

## Documentation surface

- Create `docs/vm/AOT_OWNERSHIP_CONVENTIONS.md` as the normative developer
  contract and known-debt record.
- Link it from the AoT row in `docs/vm/ARCHITECTURE_OVERVIEW.md`.
- Add an “Adding an AoT codegen template” checklist to
  `docs/vm/CHECKLISTS.md` covering signature inspection, binding reuse,
  alias-preserving clone checks, and generated-Rust compilation.
- Add dated `STATUS.md` and `DONE.md` entries for #11202.

## Verification

- TDD red: the new negative-control test initially references the missing
  compiler-output helper and fails to compile.
- TDD green: add the shared helper, run the targeted nextest filter, and verify
  the test observes E0382 from the actual two-call output.
- Run the existing warnings-denied generated-Rust test after the helper
  refactor.
- Because this touches AoT tests and conventions, run
  `bash scripts/test_aot.sh`, the AoT clippy lane through that gate,
  source-only audits, docs checks, and `cargo fmt --check`.

## Non-goals and follow-up

- Do not alter `CallDynamic` emission or close #10663.
- Do not add a liveness analysis or change generated function ABIs.
- Do not claim all AoT E2E programs are compiled as downstream Rust crates.
- #10663 owns converting the negative control to a positive compile regression
  and selecting the semantic-preserving codegen change.
