# AoT Generated-Rust Ownership Conventions

**Last updated:** 2026-07-15 (Issue #11202)

This document is the review contract for Rust ownership in code emitted by the
AoT backend. It covers whether a generated expression borrows, clones, or moves
a Julia value. GC/root lifetime across safepoints is a separate question owned
by [AOT_ROOTING_SAFETY.md](./AOT_ROOTING_SAFETY.md); a value can be safely
rooted and still be incorrectly consumed by a Rust move.

## Semantic invariant

Passing an argument to a Julia call does not consume the source binding. If the
Julia program reads that binding again, the generated Rust must keep a
semantically equivalent value available. Rust compilation is a necessary
check, but satisfying the borrow checker by deep-cloning mutable state is not
sufficient: Julia aliasing and mutation visibility must also be preserved.

## Representation classes

Classify an argument before choosing its generated Rust ownership form.

| Representation | Examples | Ownership consequence |
|---|---|---|
| Rust `Copy` scalar | integer, float, `bool`, `char` | Passing by value leaves the binding usable. |
| Runtime-owned `Value` | `Any`, dynamic call result | `Value` is not `Copy`. A reusable binding needs a borrow or a semantic-preserving clone when an owned argument is required. |
| Immutable aggregate | tuple or immutable generated struct | A structural clone may preserve Julia value semantics, but reviewers must check every field representation. |
| Mutable alias-bearing representation | runtime `Value::Array`/`Value::Dict`, typed `Vec<T>`, generated mutable struct | Preserve shared identity. `Value` clones share its `Rc<RefCell<_>>` array/dict handles, but a native `Vec<T>` or ordinary generated-struct clone may deep-copy and change Julia behavior. |
| Fresh rvalue | literal, conversion result, newly constructed temporary | May move when no source binding must survive. |

`Value::Tuple` and `Value::Struct` clone their contained values structurally.
That is compatible with immutable Julia values, but is not blanket permission
to clone a mutable generated representation.

## Decision order

Use the first applicable rule:

1. **Borrow** when the callee accepts a reference. This is the default for
   observation and comparison. The borrow must live long enough for the call,
   including for a freshly produced temporary.
2. **Copy** a native `Copy` scalar when the callee takes it by value.
3. **Clone** a reusable owned value only after proving that its `Clone`
   implementation preserves Julia identity and aliasing. Runtime `Value`
   arrays/dictionaries keep shared handles; a typed native container generally
   does not.
4. **Move** only a fresh rvalue or a binding whose last use is proven by a
   binding-identity-aware liveness analysis. Variable spelling, source order
   alone, and “the callee takes ownership” are not last-use proofs.
5. **Reject or redesign the ABI** when a mutable alias-bearing value must cross
   an owned boundary and no semantic-preserving clone exists. Prefer a borrow or
   shared handle; do not add `.clone()` merely to make rustc green.

Assignment into a new owner and function return may transfer a temporary.
Mutation receivers, call arguments, and any value read again after a call must
follow the decision order above.

## Generated call-site matrix

| Codegen site | Runtime/generated signature | Required form |
|---|---|---|
| `dynamic_binop(op, &lhs, &rhs)` | `lhs`/`rhs` are borrowed `&Value` | Borrow. The current `&({expr})` template keeps a binding usable and also borrows temporaries for the call expression. |
| `dynamic_call(name, &[...])` | the slice is borrowed, but each array element is an owned `Value` | Clone a reusable runtime `Value`; move only a fresh rvalue or proven last use. |
| generated dispatcher/static function call | generated parameters are passed by value today | Copy scalars; otherwise require a semantic-preserving clone, a borrowed/shared-handle ABI, or a proven last use. |
| builtin/runtime helper | helper-specific | Read the actual helper signature and classify every argument. Template text is not an ownership proof. |
| assignment/return | transfers to a new owner | Move a temporary; preserve a source binding when it remains live. |

## The #10663 failure

This valid Julia function uses `itr` in two calls:

```julia
function count_items(itr)
    next = iterate(itr)
    while next !== nothing
        val, st = next
        next = iterate(itr, st)
    end
end
```

The current template emits the equivalent of:

```rust
dynamic_call("iterate", &[itr]);
dynamic_call("iterate", &[itr, st]);
```

Although `dynamic_call` borrows the slice, constructing `[itr]` moves the
non-`Copy` `Value` into the array. rustc therefore reports E0382 at the second
call. The fix belongs to Issue #10663 and must use the decision order above;
this document does not prescribe `.clone()` for representation classes where a
clone would break aliasing.

## Review checklist for a template

Before adding or changing a codegen template:

1. Inspect the actual generated callee signature: owned value, shared borrow,
   mutable borrow, or shared handle.
2. Classify each argument using the representation table.
3. Check whether every source binding is read after the call. If there is no
   binding-aware liveness proof, treat an owned non-`Copy` argument as reusable.
4. Verify that any clone preserves mutation visibility and identity.
5. Compile a generated-Rust probe that uses the same non-`Copy` binding at
   least twice; a string assertion alone is insufficient.
6. Run `bash scripts/test_aot.sh`.

The concise operational version is also in
[CHECKLISTS.md](./CHECKLISTS.md#adding-an-aot-codegen-template-issue-11202).

## Enforcement and transition

`aot_e2e_tests::test_aot_generated_rust_ownership_gate_detects_reused_value_11202`
runs the real #10663 source through AoT codegen and a downstream temporary Cargo
crate. It is intentionally a **negative control** while #10663 is open: the
test passes only when rustc sees E0382 for the two-call `itr` output. This proves
that the compile harness catches the failure the earlier text-only #5658 test
missed.

The #10663 fix must convert that same test into a positive
`assert_generated_rust_compiles` (preferably warnings-denied) assertion. After
that transition, any reintroduced move makes the regression test red. The
mandatory AoT gate is `bash scripts/test_aot.sh`; do not replace it with a
generated-source substring check.
