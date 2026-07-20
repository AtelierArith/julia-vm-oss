# AoT Variable Identity and Lexical Scoping (Issue #10251)

How the AoT IR converter distinguishes same-named variable *bindings* that live
in different lexical scopes, so it does not unify them under a single
first-seen static type.

## The problem (Issue #10111)

The AoT pipeline assigns each local variable a single Rust storage slot and a
single static Rust type. Historically that slot/type was keyed on the bare
**variable-name string** in `IrConverter::declared_locals` /
`TypeInferenceEngine::env`. Two sibling top-level `let` blocks that each bind a
local with the *same name* but a *different* concrete type therefore collapsed
onto one slot:

```julia
let
    r = Int8(3) + Int8(3)     # r :: Int8
    println(typeof(r), " ", r)
end
let
    r = UInt8(200) + UInt8(200)  # a DISTINCT binding, r :: UInt8
    println(typeof(r), " ", r)
end
```

Upstream `julia` and the sjulia VM print `Int8 6` / `UInt8 144`. The AoT
compiler instead:

- typed `r` from the **first** occurrence and treated the second `let`'s binding
  as a *reassignment* of that slot, forcing a truncating `i8::try_from(144)`
  (spurious `InexactError` panic), or
- when whole-program inference widened `r` to the union `Any`, emitted
  `let mut r: Value = Value::from(i8_value)` — which does not even compile,
  because the runtime `Value` enum has no `Int8`/`UInt8` variant (it only carries
  `I64`/`I32`/`F64`/`F32`), and `Value: From<i8>` is unimplemented. Boxing is not
  a viable fallback here: even if it compiled, `typeof` would report `Int64`.

The root cause is that a `let` block introduces a **new lexical scope** in Julia,
so its locals are independent of same-named locals in sibling scopes, but the
converter tracked bindings purely by name across the whole enclosing body.

## The design

Each `let` block is a nested lexical scope. `IrConverter` now maintains a
**scope stack** (`scope_stack: Vec<LexicalScopeFrame>`) and brackets every
`let`-block conversion with `enter_lexical_scope` / `exit_lexical_scope`
(`subset_julia_vm/src/aot/analyze/ir_converter/mod.rs`,
`.../ir_converter/stmt.rs`). This gives a binding its scope as its identity —
without renaming variables or threading numeric `BindingId`s through the whole
IR and codegen (issue solution option 1's heavier form); it is the push/pop
type-environment shape (option 2).

1. **`enter_lexical_scope`** snapshots the enclosing `declared_locals` set and
   the `engine.env` type map.

2. **`exit_lexical_scope(keep)`** removes from `declared_locals` every name that
   was *newly* bound inside the scope (so a subsequent sibling scope sees a
   fresh binding, not a reassignment) and restores those names' `engine.env`
   type to the enclosing-scope value. `keep` is the single name that must leak
   its value to the enclosing scope — the target of `x = let ... end` /
   `elapsed = @elapsed ...`.

   Because inference pre-seeds `engine.env` with the type of any binding that
   genuinely leaks (a top-level global, a function local, or an `@time`-assigned
   variable), restoring the env from the entry snapshot means later *reads* of a
   leaked variable still resolve its type. `exit_lexical_scope` only changes the
   fresh-vs-reassignment decision for future *assignments*; it never loses a
   read's type. (Verified by
   `aot::analyze::tests::statement_letblock_drops_plain_result_alias_issue_8499`,
   the `@time xs = ...; xs[1]` leak case.)

3. **Precise per-scope typing.** When a variable is freshly bound inside a
   lexical scope (`in_lexical_scope()` is true) and its initializer type is
   known (not `Any`), the new binding's slot takes that precise initializer type
   instead of the inference-**widened** union. The widened type is a union over
   *every* assignment to the name across *all* sibling scopes; using it here is
   what boxed the value to `Value` (wrong) or coerced it to the first scope's
   type. A fresh binding in the function / top-level scope keeps the legacy
   widened slot type, so nothing outside a `let` body changes.

Two sibling `let` blocks are flattened into the same enclosing Rust scope, so
the two fresh bindings become two `let mut r: i8` / `let mut r: u8` — Rust
**shadowing** makes each subsequent statement see the nearest binding, which is
exactly the Julia scoping semantics.

## Scope and known limitations

- **Fixed:** sibling `let`-block same-name rebinds with distinct concrete types
  (the reported bug). Covered by
  `aot_e2e_tests::test_aot_sibling_let_same_name_distinct_types_10251`,
  `..._int_float_10251`, and the VM-parity fixture
  `tests/fixtures/aot/scope_sibling_rebind_10251.jl`.

- **Deferred — sibling `for`-loop rebinds (Issue #10523).** The same bug in
  sibling `for` loops is *not* fixed here. `emit_main` does not hoist, so the
  main-body case would be fixed by bracketing loop bodies too — but inside a
  **function** the `compute_hoisted_locals` analysis
  (`aot/codegen/aot_codegen/program.rs`) also keys on the bare name and records
  only the first `Let`'s scope, so two sibling-loop fresh `Let`s for the same
  name would be conflated into one hoisted slot and truncate again. Fixing that
  cleanly requires making the hoisting analysis binding-identity-aware, a larger
  change tracked separately.

- **Same-scope type-unstable bindings keep a boxed `Value` slot.** When the
  *scope-local* join of assignments within one `let` is `StaticType::Any` or
  `StaticType::Union { .. }`, `enter_lexical_scope` keeps that dynamic entry so
  the declaration site emits `let mut x: Value` (Issues #7075 / #6978 / #10537).
  `unify_types` collapses `Any+T` to `Any` (not `Union`), so both cases must be
  preserved — keeping only `Union` would drop an `Any` entry, let the first
  concrete assignment declare `i64`, and fail codegen on a later Any store
  (codex review of #10528). Monomorphic-within-scope bindings still drop the
  cross-scope whole-program entry for precise typing (#10251).

- **Pre-existing, out of scope — same-scope type-changing reassignment of
  monomorphic-but-incompatible concretes that AoT cannot box.** e.g. some
  numeric pairs where `Value` has no variant. A fully polymorphic single-scope
  local still needs true per-assignment SSA typing beyond the Any/Union boundary.

- **Pre-existing, out of scope — outer-name shadowing inside a `let`.** A `let`
  body assignment to a name that already exists as an enclosing *local* still
  reassigns that outer slot (matching prior behavior); the fix targets sibling
  independence, not fresh shadowing of an enclosing local.
