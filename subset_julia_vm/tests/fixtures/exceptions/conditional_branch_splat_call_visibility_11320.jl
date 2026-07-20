# A method defined in an untaken top-level branch (`if cond; f(x)=x; end` with
# `cond == false`) must leave its name undefined at runtime, and a splat/
# kwargs call to that name must consult the same runtime source-order
# visibility decision the direct-call path uses for a hoisted-but-not-yet-
# active definition, raising `UndefVarError` rather than silently succeeding
# via a call path that bypasses it (Issue #11320; siblings #11286/#10461).
#
# Root cause was two-fold:
#   1. `compile_main`'s eager top-level-definition activation drain treated
#      any function found by scanning statements (including one nested
#      inside an untaken `if`/loop branch) as unconditionally reached by
#      source position, activating it regardless of whether its own
#      enclosing branch ever executed.
#   2. The positional-splat dynamic call path manufactured a callable token
#      via `PushFunction` with no visibility check at all, and evaluated the
#      splat argument expression before ever consulting the callee's
#      existence -- violating Julia's callee-before-arguments evaluation
#      order for a keyword-free call and never raising `UndefVarError` for
#      an invisible callee.
#
# Julia's own evaluation order differs by call shape (verified with
# `Meta.lower`): a keyword-free call (splat or not) resolves the callee
# `GlobalRef` BEFORE any argument expression, but a call carrying keyword
# arguments evaluates every positional/keyword value BEFORE the callee is
# read (`Core.kwcall` receives already-evaluated arguments). This fixture
# pins both shapes at top-level (`global` mutation needs the true top-level
# scope of the original bug report, not a `@testset`/function scope): the
# splat form must never evaluate `arg()` (`side` stays `0`), while the
# kwargs form legitimately does (`side` becomes `1`) -- both must still
# raise `UndefVarError`, not a wrong error type or no error at all.

# Positional splat call: upstream never evaluates the splat argument
# expression because the callee is resolved (and found undefined) first.
side = 0
arg() = (global side += 1; (1,))
if false
    f_splat(x) = x
end
err = nothing
try
    f_splat(arg()...)
catch e
    global err = e
end
@assert err isa UndefVarError "expected UndefVarError, got $(typeof(err))"
@assert side == 0 "callee-before-arguments: arg() must not run, got side=$side"

# Keyword-argument call: upstream evaluates the positional argument before
# reading the callee `GlobalRef`, so `side_kw` becomes `1`, but the eventual
# failure is still `UndefVarError` (a hoisted-but-inactive definition), not a
# `MethodError`/"not found" that would wrongly imply the generic function
# itself already exists.
side_kw = 0
arg_kw() = (global side_kw += 1; 1)
if false
    f_kw(x; y=1) = x + y
end
err_kw = nothing
try
    f_kw(arg_kw(); y=2)
catch e
    global err_kw = e
end
@assert err_kw isa UndefVarError "expected UndefVarError, got $(typeof(err_kw))"
@assert side_kw == 1 "kwargs calls evaluate args before the callee upstream, got side_kw=$side_kw"

# Non-regression: an ordinary (unconditionally reached) splat call still
# dispatches normally once the callee is genuinely visible.
f_ok(x, y) = x + y
@assert f_ok((1, 2)...) == 3

true
