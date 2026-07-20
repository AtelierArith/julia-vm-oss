# No-suffix reached-method recovery design

Issue: #11745

## Problem

A fresh `REPLSession` reaches a conditional method and then raises an uncaught
error as the final statement. Upstream keeps the method, but sjulia loses it by
the next probe or full-rebuild barrier. The nearby #11742 case passes when the
failed input also contains an unreached source-later method, so recovery is
incorrectly dependent on a dormant function suffix.

## Investigation boundary

First pin two observations in one regression: the immediate next eval and the
same probe after a module barrier. Use the first failing assertion to determine
whether runtime recovery itself failed or the recovered compiler/session
snapshot cannot reconstruct the no-suffix method.

## Design constraints

- Reached publication is determined only by the typed activation trace.
- No dummy dormant method or source-shape workaround may be introduced.
- The no-suffix and dormant-suffix cases must use the same recovery authority.
- Unreached definitions must remain absent.
- Successful full compiles and runtime-nominal recovery must remain unchanged.

## Rejected hypothesis

The all-reached fast path in
`ReplPersistentCompile::retain_reached_function_prefix` initially appeared to
skip a necessary fresh-compile inference-cache clear. Forcing the no-suffix
case through the existing projection branch did not change the failure: the
method remained callable immediately and still disappeared only after the
barrier. Therefore cache invalidation at that checkpoint is not the root cause;
the session-definition mirror and its later merge must be inspected instead.

## Root cause and chosen correction

The #11742 eligibility change passed `current_input_function_count`, which is
`repl_support::source_function_count(&program)`: it counts only Julia-visible
methods hoisted into `Program.functions`. A conditional method remains a named
`Stmt::FunctionDef` inside main. Session persistence already uses the complete
authority, `current_input_stored_function_count`, defined as the hoisted count
plus `collect_main_inline_named_functions(&program).len()`.

In #11742, the source-later root method happened to contribute one hoisted row
and opened the recovery plan for the earlier inline method. With no suffix the
hoisted count was zero, so no plan was built. Pass the existing complete stored
function count to `full_compile_definition_recovery_plan`; its marker scan and
typed VM prefix validation remain unchanged, and generated anonymous helpers
remain excluded by `collect_main_inline_named_functions`.

## Verification

Run the new RED/GREEN regression, the #11742 regression, the #11654 recovery
module, source audits, fmt, clippy, and the guarded full release suite.
