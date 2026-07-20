# REPL Semantics Policy

SubsetJuliaVM's host REPL is a persistent session over `REPLSession::eval`.
Each input step is parsed, lowered, compiled, and executed against saved
session state, then the host prints captured stdout and the step result.

## Scope Policy

- Top-level `for` and `while` bodies use Julia REPL soft-scope behavior for
  existing globals: reads see the session global, and assignments such as
  `x += i` update that global. This **lenient** rule is specific to the
  interactive `REPLSession::eval` line-input surface (this document).
- **Whole-buffer host surfaces are strict.** Running a whole program/buffer
  — `sjulia file.jl` / `-e` / piped stdin, the C ABI editor entries
  (`compile_and_run` / `compile_and_run_detailed` / `compile_and_run_streaming`),
  and the WASM `run_from_source` — uses **strict file-mode soft scope**
  (`pipeline::SoftScopeMode::Strict`, Issues #9210 / #9283): a top-level loop
  assignment to a name that already exists as a global binds a *new local*, so a
  read-before-write (`+=`) raises `UndefVarError` and an explicit `global` is
  required to mutate the global — matching `julia file.jl`. Only the interactive
  REPL keeps the lenient rule above.
- `let` bodies introduce local bindings. A same-named assignment inside `let`
  shadows the session global and must not overwrite it after the step returns
  (Issue #8972).
- Timing macros such as `@time` evaluate the user's expression in caller scope:
  `@time x = 42` persists `x`, even though the macro expansion uses a local
  result-capture `let` internally (Issue #9044).
- Function bodies are local scopes. Assignments without `global` remain local;
  explicit `global x` writes the session global.
- `ans` is updated after successful non-`nothing` steps, including steps whose
  display is suppressed by a trailing semicolon.

## Display Policy

- The host prints `REPLResult.output` first.
- Function definitions display as `name (generic function with 1 method)`.
- A trailing semicolon suppresses host display for the step result but does not
  suppress stdout and does not prevent `ans` from updating.
- User-defined `show` output in `REPLResult.value_display` wins over the default
  VM value formatter.

## Current Divergences

- Vector and matrix display is intentionally compact in sjulia's current host
  formatter; full upstream `MIME("text/plain")` array layout remains outside the
  #8715 matrix.
- A function whose final expression is a global `+=` assignment is tracked as
  Issue #8976.
- Assignment from a multiline `begin ... end` block not displaying the assigned
  value is tracked as Issue #8977.

## Verification

- `subset_julia_vm/tests/fixtures/repl_session/*.toml` is the source of truth for
  ordered REPL-session parity cases.
- `scripts/repl_session_julia_oracle.py --check` evaluates the same steps under
  upstream Julia with `REPL.softscope(Meta.parseall(step))`.
- `repl_session_fixture_tests` evaluates the same steps through
  `REPLSession::eval`.
- `scripts/repl_session_parity.sh` runs both sides.
