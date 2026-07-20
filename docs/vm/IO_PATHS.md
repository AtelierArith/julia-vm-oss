# IO Path Separation: `write` / `print`-`show` / `display` (Issue #10045)

**Status**: design document. Describes the upstream responsibility split,
sjulia's current entry points, the rule that now prevents write/text
crossover, and the regression tests guarding it. No code changes ship with
this document.

## Summary

Upstream Julia keeps three IO responsibilities structurally separate:
`write` moves raw bytes, `print`/`show` render text, and `display` renders
rich (MIME-dispatched) output built on top of `show`. sjulia's history shows
five closed incidents (#9578, #9585, #9777, #10008, #10002) where these
paths leaked into each other — a numeric `write` silently rendering decimal
text, `print` of an `Unsigned` silently rendering `show`'s hex form, or a
`sprint`-scoped buffer capturing writes meant for an unrelated nested
`IOBuffer`. The most recent pair (PR #10063/#10064, closing #10008) added a
CI-registered audit script that encodes the rule going forward; this
document records the responsibility split, sjulia's current file layout, and
that rule so the next new IO helper does not reopen the same class of bug.

## Upstream's Responsibility Split

Verified against `julia/` (this repo's upstream reference checkout):

- **`write` is binary, with no generic text fallback.** The generic method
  is `write(io::IO, x) = throw(MethodError(write, (io, x)))`
  (`julia/base/io.jl:794`) — there is no default that formats an unknown
  value as text. Concrete numeric overloads write raw bytes via
  `reinterpret`: `write(s::IO, x::Int8) = write(s, reinterpret(UInt8, x))`
  (`io.jl:814`); the multi-width numeric overload
  (`io.jl:815`, `Union{Int16,UInt16,Int32,UInt32,Int64,UInt64,Int128,UInt128,
  Float16,Float32,Float64}`) and `write(s::IO, x::Bool) = write(s, UInt8(x))`
  (`io.jl:819`) follow the same raw-byte contract. A type that wants
  `write` support must define it explicitly; it never falls back to `show`.
- **`print` is text, and defaults to `show`.** `julia/base/strings/io.jl:31-38`:

  ```julia
  function print(io::IO, x)
      lock(io)
      try
          show(io, x)
      finally
          unlock(io)
      end
      return nothing
  end
  ```

  The doc comment above it is explicit about the intended split: *"`print`
  falls back to calling the 2-argument `show(io, x)`... Define `print` if
  your type has a separate 'plain' representation. For example, `show`
  displays strings with quotes, and `print` displays strings without
  quotes."* (`strings/io.jl:13-16`). So `print`/`show` are two ends of one
  text pipeline — `show` is the primitive every type defines, `print` is the
  "undecorated" variant only some types override.
- **`display` is rich/MIME output, built on `show`.** `Base.Multimedia`
  (`julia/base/multimedia.jl`) dispatches by `MIME` type; the plain-text
  display path is literally implemented as `show`:
  `display(d::TextDisplay, M::MIME"text/plain", x) = (show(d.io, M, x);
  println(d.io))` (`multimedia.jl:254`). `display` is a layer *above*
  `print`/`show`, not a peer of `write`.

Layering, top to bottom: `display` → `print`/`show` (text) as one pipeline,
and `write` (bytes) as a structurally separate pipeline with no automatic
crossover between the two.

## sjulia's Current Entry Points

sjulia does not split `io.jl`/`show.jl`/`strings/io.jl` the way upstream
does; `print`/`show`/`write` Julia-level methods all live in one file,
`subset_julia_vm/src/julia/base/io.jl` (1823 lines) — e.g. the scalar `show`
ladder starting at line 1202 (`show(io::IO, x)`) through the per-type
overloads (`show(io::IO, x::Int8)` at 1279, `show(io::IO, x::UInt8) =
print(io, "0x", string(x, base=16, pad=2))` at 1314, mirroring upstream's
hex `show`-form for unsigned integers). The Rust runtime side is split
across a few files:

| Layer | File | Role |
|---|---|---|
| Byte payload construction | `subset_julia_vm_vm/src/vm/builtins_io.rs` | `iowrite_payload_bytes` (line 31) — the fixed raw-byte encoder for `BuiltinId::IOWrite` |
| Print/show text formatting | `subset_julia_vm_vm/src/vm/formatting/mod.rs` | `format_value_print`/`format_value_print_impl` (print-form) vs. `format_value`/`format_value_impl` (show-form) |
| `sprint`/redirect buffering | `subset_julia_vm_vm/src/vm/hof_exec/sprint.rs`, `subset_julia_vm_vm/src/vm/state.rs` | `emit_output`/`emit_stderr`, sprint-scoped sink routing |
| IOBuffer byte storage | `subset_julia_vm_bytecode/src/value/io.rs` | Raw byte cursor storage (post-#8996/#10004: `Vector{UInt8}`-shaped, not `String`) |

`iowrite_payload_bytes` (post-#9578/#10008 fix) encodes every primitive
numeric `Value` variant to little-endian bytes directly
(`Value::I8(n) => n.to_le_bytes().to_vec()`, and so on through `F64`/`Bool`/
`Char`/`Str`/`StrBytes`) — this is sjulia's analogue of upstream's
`reinterpret`-based numeric `write` overloads. Its final arm is a documented,
intentional exception, not a residual bug:

```rust
// subset_julia_vm_vm/src/vm/builtins_io.rs:53
_ => crate::vm::formatting::format_value_print(&Resolved::trivial(value)).into_bytes(),
```

Non-primitive values (anything without a dedicated raw-byte encoding) still
fall through to text formatting for `write`. Issue #10008's blast-radius
section names this explicitly as the boundary: *"`BuiltinId::IOWrite`
fallback for non-primitive values still intentionally formats text; primitive
payloads must stay on the raw-byte path."* This is a known, upstream-diverging
residual (upstream's generic `write(io::IO, x)` throws `MethodError` instead
of falling back to text) rather than an open bug — tracked here as a
candidate for a future `write`-path tightening, not something this document
resolves.

## The Five Incidents

| Issue | Symptom | Root cause |
|---|---|---|
| #9578 | `write(io, Int8(42))` printed `"42"` (decimal text) and returned `2`, instead of writing 1 raw byte | `BuiltinId::IOWrite` routed every payload through `format_value_print` before the #10008 fix added `iowrite_payload_bytes` |
| #9585 | `println(UInt8(2))` printed `0x02` (hex/show-form) instead of `2` (decimal/print-form) | `format_value_print` only overrode Symbol/VersionNumber; unsigned integers fell through to the show-form formatter (`format_value_impl`) |
| #9777 | `print(inner_io, ...)` for a temporary `IOBuffer` created *inside* an active `sprint(...)` leaked into the outer sprint buffer | `IOPrint`/`emit_print_text_to_sink` checked `sprint_state.is_some()` before honoring an explicit sink argument, so any active sprint captured all prints regardless of target |
| #10008 | `write(io, numeric)` behaved like text display (§ above); Pure Julia `sprint_context` used `write(io, x)` as its own text fallback, so restoring binary `write` broke `sprint(show, Float64; context=...)` | Same hidden write→text coupling as #9578, this time inside a Pure Julia display helper rather than the Rust builtin |
| #10002 | `print`/`println`/`string` of `UInt8`/`UInt16`/`UInt32`/`UInt64`/`UInt128` used the hex show-form | Same root cause as #9585, generalized across all unsigned widths and confirmed with a scalar-matrix regression test |

#9578/#9585/#9777 are the original bugs; #10008/#10002 are their
"prevention" issues — filed specifically because the same bug class
recurred (a display helper reaching for `write` as a generic text sink, or a
print-form override missing a numeric family) even after the first fix.

## The Rule That Prevents Recurrence

Issue #10008's proposed prevention item — *"Add a small static audit for
direct `write(io, x)` or `write(io, arg)` in
`subset_julia_vm/src/julia/base/*.jl` display helpers, with allowlisted
binary/string-only cases"* — landed as
`scripts/check_julia_display_write_text_paths.sh` (PR #10063). The script's
rule, verbatim from its header comment:

> "display-text helpers must not route arbitrary values through binary
> `write(io, x)`. Use `print`/`show` for text paths and reserve `write` for
> string/char/byte/raw payloads."

Mechanically: it scans every `subset_julia_vm/src/julia/base/*.jl` file
(one directory level, not recursive) for `write(io, <arbitrary-looking
identifier>)` call shapes (`x`, `arg`, `args`, `value`, `v`, `obj`, `item`)
outside comments, and fails the build if one is found. It is registered in
`docs/vm/CODE_AUDITS.md` alongside the other `check_*.sh` scripts and runs
as part of the local gate set (Actions is disabled for this repo; local
`check_*.sh` scripts are the only merge gate — see `sjulia-lead-review-merge`
skill).

**The layering rule this enforces, stated positively:**

1. A Julia-level display helper (`show`, `print`, `Base.print_matrix`,
   `sprint`-callable lambdas, etc.) may call `print`/`show`/string
   construction on an arbitrary value. It may call `write` only with a
   `String`/`Char`/byte-array-shaped literal or already-formatted text — never
   with an unexamined function parameter.
2. `write(io, x)` for a genuinely arbitrary `x` belongs only in the raw-byte
   encoder (`iowrite_payload_bytes` and its Julia-level numeric/`Bool`/`Char`
   overloads in `io.jl`), which is the one place responsible for the
   byte-encoding decision per concrete type — mirroring upstream's
   type-by-type `write` overload ladder (`io.jl:814-878`) rather than a
   generic fallback.
3. `sprint`/redirect buffering must honor an explicit IO sink argument
   ahead of any ambient `sprint_state` — a nested `IOBuffer()` inside a
   `sprint(...)` body is a distinct sink, not an alias for the outer buffer
   (the #9777 fix).

## Regression Tests Guarding This

| Fixture / test | Guards |
|---|---|
| `subset_julia_vm/tests/fixtures/io/iowrite_numeric_raw_bytes_9578.jl` | Byte count and `codeunit` output for `write(io, numeric)` across signed/unsigned/float/`Bool`/`Char`/`UInt8` |
| `subset_julia_vm/tests/fixtures/io/unsigned_print_decimal_9585.jl` | Every unsigned width across `print`, `println`, `IOBuffer`, `string`, `show`, `repr`, and `UInt8[...]` container element display in one fixture |
| `subset_julia_vm/tests/fixtures/io/test_sprint_lambda.jl` | Nested `IOBuffer()` inside `sprint` round-trips through `take!` without leaking into the outer buffer |
| `subset_julia_vm/tests/fixtures/error/error_typeassert_typeerror_5146.jl` | `sprint(showerror, err)` preserves upstream message ordering under the sink-precedence fix |
| `subset_julia_vm/tests/fixtures/iocontext/test_sprint_context.jl` | `context=:compact => true` (a `Pair`, not just a `Tuple`) is observed as a property by `sprint_context`, and stays textual after `write` became binary |
| `scripts/check_julia_display_write_text_paths.sh` | Static audit — no `subset_julia_vm/src/julia/base/*.jl` display helper may reintroduce an arbitrary-value `write(io, x)` |

## Non-Goals

- This document does not propose adding an explicit binary/text *mode* flag
  to `IOBuffer`/`GenericIOBuffer` (epic §C's phrasing); sjulia's IOBuffer
  already stores raw bytes (`subset_julia_vm_bytecode/src/value/io.rs`, since
  #8996/#10004) and dispatches binary vs. text behavior by **which function
  is called** (`write` vs. `print`/`show`), matching upstream's model of "one
  buffer, two disjoint method families" rather than a buffer-level mode.
- Tightening the `write`-for-non-primitive-values fallback (§ above,
  `builtins_io.rs:53`) to match upstream's `MethodError` default is a
  candidate follow-up, not something this document changes; it is called
  out so it is not mistaken for a still-open bug.
- This document does not cover binary file IO (`open(..., "wb")`,
  `read!`/`write` on `IOStream`) beyond the `IOBuffer` cases the cited
  fixtures exercise.

## Related Documentation

- `docs/vm/CODE_AUDITS.md` — where every `check_*.sh` script (including the
  one this document centers on) is registered.
- `memory/project/project_9478_display_path_parity.md` — prior display-path
  parity investigation; PR #10063 extended it with the #10008 IOContext
  sprint lesson.
- `docs/vm/TYPE_VALUE.md` — sibling #10045 design document (unrelated
  subsystem: the `Type{T}` value-representation gap).
