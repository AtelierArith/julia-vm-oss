# Duplicate-Branch Clippy Ratchet Design

**Issue:** #10725
**Date:** 2026-07-15

## Context

The workspace-wide advisory probe

```bash
timeout 1800 cargo clippy --workspace --all-targets -- \
  -W clippy::match_same_arms \
  -W clippy::if_same_then_else
```

currently reports 526 unique diagnostics after de-duplicating repeated
lib/test-target messages by lint, file, line, and message. All 526 diagnostics
are `match_same_arms`; `if_same_then_else` contributes none.

The inventory is produced from Cargo's JSON diagnostics with this exact
command. Keeping the primary span in the key collapses duplicate lib/test
target emissions without merging distinct warnings on different lines:

```bash
mkdir -p target/duplicate-branch-inventory
timeout 1800 cargo clippy --workspace --all-targets --message-format=json -- \
  -W clippy::match_same_arms \
  -W clippy::if_same_then_else \
  > target/duplicate-branch-inventory/clippy.json
jq -r '
  select(.reason == "compiler-message")
  | .message as $message
  | select(
      $message.code.code == "clippy::match_same_arms"
      or $message.code.code == "clippy::if_same_then_else"
    )
  | ($message.spans | map(select(.is_primary)) | first) as $span
  | [$message.code.code, $span.file_name, $span.line_start, $message.message]
  | @tsv
' target/duplicate-branch-inventory/clippy.json \
  | sort -u \
  > target/duplicate-branch-inventory/unique.tsv
wc -l target/duplicate-branch-inventory/unique.tsv
cut -f1 target/duplicate-branch-inventory/unique.tsv | sort | uniq -c
```

| Crate | Unique diagnostics |
|---|---:|
| `subset_julia_vm_compile` | 218 |
| `subset_julia_vm_vm` | 122 |
| `subset_julia_vm_types` | 74 |
| `subset_julia_vm_bytecode` | 51 |
| `subset_julia_vm_lowering` | 38 |
| `subset_julia_vm` | 16 |
| `subset_julia_vm_runtime` | 3 |
| `subset_julia_vm_parser` | 2 |
| `subset_julia_vm_ffi` | 2 |

The three largest owners are now the physically split compiler (218), VM
(122), and type-system (74) crates. Turning the lint into a workspace-wide deny
would therefore mix hundreds of unrelated semantic tables and exhaustive
matches into one high-risk change.

## Selected Approach

Ratchet one high-signal VM module instead of rewriting the whole baseline.
`subset_julia_vm_vm/src/vm/exec/return_ops.rs` has three byte-for-byte identical implementations for
`ReturnRng`, `ReturnRange`, and `ReturnRef`. Return continuation code is
high-signal because a future fix applied to only one arm would silently split
the behavior of three typed return instructions.

The change will:

1. Extract the common implementation into a `Vm` helper.
2. Route the three instructions through one combined match arm and that helper.
3. Put `#[deny(clippy::match_same_arms)]` on the `return_ops` module declaration,
   so ordinary Clippy lanes prevent this module from accumulating another
   identical-arm warning.
4. Record the workspace baseline and module-by-module ratchet policy in
   `docs/vm/CODE_AUDITS.md`.

The helper must preserve the current order exactly:

1. pop a value, falling back to `Value::Nothing`;
2. offer it to the generator-iterate continuation;
3. offer it to composed-call continuation;
4. return to the caller frame and push the value, if a caller exists;
5. otherwise exit with the final value.

It deliberately does not use `route_value_return`, whose broader HOF, sprint,
and dynamic-return behavior is not part of these three instructions' current
semantics.

## Alternatives Rejected

### Fix all 526 diagnostics

This would combine mechanically mergeable patterns, intentional semantic
tables, wildcard fallbacks, and genuinely duplicated implementations. The
review surface is too broad for one appropriately sized PR and would make
behavior changes hard to distinguish from formatting.

### Add `allow` annotations to the existing duplicates

Suppressions would make the advisory output quieter without reducing the risk
that the three return implementations drift apart. The high-signal case should
be structurally unified and then protected.

### Add a source-scanning audit script

A source regex would only recognize the current spelling of the match arms.
The Clippy lint already models the property we want; a module-level lint level
is both narrower and more semantic, and requires no new audit registration or
negative self-test.

## Verification

TDD uses the lint ratchet itself:

1. Add the module-level `deny` before changing `return_ops.rs`.
2. Run Clippy and confirm it fails on the existing three identical arms.
3. Extract the helper and combine the arms.
4. Re-run the same Clippy command and the repository default Clippy lane.

Behavioral verification will run the focused VM/lib tests that exercise return
dispatch, followed by the full release suite required for VM changes. The
workspace advisory probe will be re-counted to prove that `return_ops.rs` is at
zero and the unique total falls by one diagnostic.
