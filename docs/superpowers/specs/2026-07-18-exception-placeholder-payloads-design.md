# Restore the remaining exception-funnel payloads (#11572)

## Problem

The central `VmError` to Julia-exception funnel now reconstructs typed payloads
for the common VM-raised exception classes, but two arms still manufacture
observable placeholder fields:

- `ParseError.detail` is always `nothing`.
- `StringIndexError.string` is always `""`.

Upstream Julia carries the parser diagnostic object in the former and the exact
offending `AbstractString` in the latter. Code which catches either exception
therefore observes a different object in sjulia even though the exception class
and displayed message are already routed through the correct funnel arm.

## Upstream contract

For a JuliaSyntax parser failure, upstream `Base.Meta.ParseError.detail` is a
`Base.JuliaSyntax.ParseError` with these observable fields:

```text
ParseError(source, diagnostics, incomplete_tag)
  SourceFile(code, byte_offset, filename, first_line, line_starts)
  Diagnostic(first_byte, last_byte, level, message)
```

The parser detail uses one-based inclusive diagnostic byte positions (including
`SourceFile.byte_offset`) and a symbolic severity (`:error`). Its incomplete
tag is one of Julia's structural categories (`:none`, `:string`, `:char`,
`:comment`, `:block`, `:cmd`, or `:other`). `StringIndexError` has exactly
`(string::AbstractString, index::Int)` and retains the original string object.

The sjulia parser already retains the source, a structured `ParseError` enum,
and spans. The string-index raise sites already hold the offending `Value`,
including the byte-backed invalid-UTF-8 `StrBytes` representation.

sjulia currently does not preserve the outer `Base.` owner when qualified code
accesses a Base-preloaded submodule (`Base.Meta.parse` fails while bare
`Meta.parse` works). That independently discovered namespace gap is tracked by
#11614. This design creates the real `JuliaSyntax.ParseError` type family and
tests its fields; restoration of the `Base.JuliaSyntax` qualified binding is
not silently approximated here.

The producer audit also found that an in-bounds invalid code-unit supplied by
an index vector (`"é"[[2]]`) currently becomes `BoundsError` before reaching
the funnel. That wrong-class bug is tracked by #11615 and is repaired together
with the string payload because the same producer must call the new helper.
The distinct out-of-bounds vector case correctly selects `BoundsError` but
still lacks its `.a` receiver; #11616 tracks that broader BoundsError payload
work and is not hidden or reclassified by this design.

## Constraints

- Preserve `VmError` as an equality-comparable, lightweight error taxonomy.
  Although `VmError` and `Value` share the bytecode crate, `VmError` derives
  `PartialEq + Eq` while the full runtime `Value` deliberately does not (it
  contains mutable/reference-bearing carriers). Embedding `Value` would remove
  that contract or require a second, incomplete value representation.
- Preserve the exact offending string representation. Converting every string
  to Rust `String` would lose `StrBytes` and would preclude future
  `AbstractString` implementations.
- Parser detail must be structured and field-addressable, not a rendered string,
  tuple, or named-tuple approximation.
- A parked payload must be attached atomically to the error it belongs to,
  consumed exactly once by the funnel, and discarded on a key mismatch. A
  successful operation must never leave a payload behind.
- Keep exception class selection centralized in `VmError::exception_class`.
- Preserve existing message and `showerror` behavior; this issue restores
  payload fields rather than replacing sjulia's parser diagnostics formatter.

## Chosen design

Use the existing one-shot pending-payload boundary already proven for
`MethodError`, `DomainError`, `TypeError`, and getfield `BoundsError`.

### StringIndexError

Add a VM-local pending carrier containing the exact `Value`, requested index,
and the variant's nearby-index metadata. A
`string_index_error_with_string(value, index, valid_indices)` helper parks that
carrier and returns the corresponding `VmError::StringIndexError` in one
operation. Every runtime string-index raise site uses the helper. The
index-vector helper returns `StringIndexError` for an in-bounds non-character
boundary (#11615), while true numeric out-of-bounds indices remain
`IndexOutOfBounds` for #11616 to restore separately.

Range indexing validates both caller-visible Julia code-unit endpoints before
advancing the final character to Rust's exclusive byte slice end. This keeps
valid multibyte endpoints valid and rejects continuation-byte endpoints as
`StringIndexError` (#11618); it never treats the Julia inclusive final index as
an already-exclusive Rust offset.

Base string consumers exposed by that correction (`split`, `rsplit`,
`chopprefix`, `chopsuffix`, and the registered Irrational display workaround)
must likewise construct inclusive endpoints with `prevind`, never arbitrary
byte offsets. The separate pre-existing `lastindex(::String)` mismatch is
tracked by #11624 rather than reused as an endpoint shortcut.

At funnel entry, take the carrier unconditionally. The `StringIndexError` arm
uses the parked string only when both the index and nearby-index key match the
current error; otherwise it retains the existing empty-string fallback for
synthetic or externally constructed `VmError` values. This preserves safety
for callers which do not own a VM `Value` while removing placeholders from all
real runtime raise paths.

### ParseError

Define the upstream-shaped JuliaSyntax detail structs in Base's Julia source:

- `JuliaSyntax.SourceFile`
- `JuliaSyntax.Diagnostic`
- `JuliaSyntax.ParseError`

The Rust `Meta.parse` bridge converts each parser error into a `Diagnostic`,
using its byte span, `:error` level, and structured diagnostic message. It
constructs one `SourceFile` for the parsed source and a `JuliaSyntax.ParseError`
whose `incomplete_tag` is derived from the parser error variant and expected
delimiter: string, character, and block-comment lexer errors map directly;
block constructs waiting for `end` map to `:block`; other appendable syntax
maps to `:other`; ordinary errors map to `:none`. The parsed substring used by
`Meta.parse(str, start)` is the detail's code, while its `byte_offset` records
the zero-based offset into the caller's original string and is added to each
diagnostic's one-based byte bounds.

A `parse_error_with_detail(message, detail)` helper parks the resulting detail
and returns `VmError::ParseError(message)` atomically. The funnel consumes the
carrier unconditionally and attaches it only when its message key matches.
Synthetic `VmError::ParseError` construction continues to fall back to
`nothing` rather than risking a stale detail object.

Changing the parser bridge methods from `&self` to `&mut self` is intentional:
constructing VM struct values and parking the one-shot payload are runtime state
mutations.

## Error and lifetime behavior

All pending exception payloads are taken before the funnel matches the current
error. Therefore:

1. a matching error receives its payload;
2. a mismatching error discards stale state;
3. an uncatchable/internal error also clears stale payloads; and
4. a second error cannot inherit the first error's string or parser detail.

The helper combines payload parking with `VmError` construction, so no success
path can intervene between those operations. This preserves the transactional
invariant established after the #9787 stale-side-channel regression.

## Rejected alternatives

### Put VM values directly in `VmError`

Rejected even though both types live in `subset_julia_vm_bytecode`: `VmError`
derives `PartialEq + Eq`, whereas `Value` intentionally cannot because it
contains mutable and reference-bearing carriers. Removing error equality or
inventing partial equality for runtime values would expand the semantic and
testing scope far beyond these two payload fields.

### Store only a Rust `String` in `VmError::StringIndexError`

Rejected because the offending Julia value may be byte-backed `StrBytes` or a
future `AbstractString` subtype. A lossy textual copy does not implement the
upstream object contract.

### Use a tuple or named tuple for parser detail

Rejected because callers can observe `typeof(e.detail)`, field names, nested
source and diagnostics types, and incomplete status. A shape-compatible
container with the wrong type would preserve the compatibility gap.

### Fix StringIndexError and defer ParseError

Rejected because #11572 explicitly tracks both remaining placeholders, and the
parser already exposes enough structured information to construct the detail
node without a parser rewrite.

## Tests

1. Add an upstream-parity exception fixture which catches `Meta.parse` failures
   and checks that the detail type name ends in `JuliaSyntax.ParseError`, plus
   its source fields, diagnostic byte bounds, severity, message, and incomplete
   tag. Exact `Base.JuliaSyntax` qualification remains gated by #11614.
2. In the same fixture, catch invalid indexing for a Unicode string and verify
   `.string === original`, `.string` contents, and `.index`.
3. Exercise both scalar and index-vector invalid byte positions so #11615 is a
   class-and-payload regression, without changing true out-of-bounds behavior.
4. Cover byte-backed invalid-UTF-8 strings at the Rust VM layer so the payload is
   retained as `StrBytes`, not normalized to UTF-8.
5. Add a two-error regression: a payload-bearing error followed by a synthetic
   mismatching error must receive the fallback, proving one-shot consumption and
   no cross-contamination.
6. Keep existing exception-taxonomy and `showerror` fixtures green to prove the
   restored fields do not alter class selection or display behavior.

The fixture ends with `true`, is registered in the existing exceptions
manifest, and is first run with upstream Julia before sjulia.

## Documentation

- Remove the simplified-`nothing` note from Base's `ParseError` documentation.
- Record #11572 in `docs/vm/STATUS.md` and `docs/vm/DONE.md` with the one-shot
  payload invariant and upstream parity evidence.
- Record the #10445 same-leaf constructor collision exposed by the nested
  detail type. Concrete inner allocation resolves by declaration owner; the
  remaining cached synthetic-default gap is registered as W-75 until #10445
  is complete.
- Track cross-route string-index validator consolidation as prevention #11621.
- Track upstream-compatible `lastindex(::String)` semantics separately as
  #11624; this change deliberately uses `prevind(s, ncodeunits(s) + 1)` while
  that broader Base bug remains open.
