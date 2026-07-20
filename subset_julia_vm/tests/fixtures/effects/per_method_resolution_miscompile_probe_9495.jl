# Miscompile probe for statically-resolved per-method effect summaries
# (Issue #9495). A pure method call that is statically resolved and then
# CSE'd / DCE'd across an impure same-name sibling must still produce
# upstream-correct output — the more-aggressive effect-gated transforms must
# never suppress, duplicate, or corrupt observable behavior.
using Test

# Count the impure sibling's side effect so we can assert it is neither
# dropped by DCE nor duplicated by CSE.
const io_calls = Ref(0)

f(x::Int) = x + 1
f(x::Float64) = (io_calls[] += 1; x * 2.0)

# Both f(y) resolve to the pure f(::Int); the name-level merge (poisoned by the
# impure Float64 sibling) would block CSE, but resolution recovers it. Output
# must be correct regardless of whether the two calls are merged.
g(y::Int) = f(y) + f(y)
@test g(10) == 22
@test g(100) == 202

# The impure Float64 sibling still runs its side effect exactly once per call:
# resolution must NOT apply the pure summary to it, and CSE/DCE must not touch
# an effectful call.
@test f(2.5) == 5.0
@test io_calls[] == 1

# A dead, statically-resolved *pure* call must not change observable behavior
# whether or not DCE removes it.
h(y::Int) = (f(y); 99)
@test h(7) == 99
@test io_calls[] == 1

# A dead, statically-resolved *impure* call keeps its side effect (nothrow but
# not effect-free => never removable).
k() = (f(1.5); 7)
@test k() == 7
@test io_calls[] == 2

true
