# Issue #5267: `==` on tuples and named tuples must fold `==` (not `isequal`)
# over the elements, so floating-point edge cases match upstream Julia:
#   - `0.0 == -0.0` is `true`  (isequal would be `false`)
#   - `NaN == NaN` is `false`  (isequal would be `true`)
# The previous early tuple route emitted `BuiltinId::Isequal`, which mishandled
# both. `isequal` on the same aggregates is unaffected and stays correct.
# All assertions match upstream Julia 1.12.

checks = Bool[]

# --- bare-tuple `==`: -0.0 / 0.0 compare equal ---------------------------
push!(checks, (0.0,) == (-0.0,))
push!(checks, (1.0, 0.0) == (1.0, -0.0))
push!(checks, (-0.0, -0.0) == (0.0, 0.0))

# --- bare-tuple `==`: NaN compares not-equal -----------------------------
push!(checks, !((NaN,) == (NaN,)))
push!(checks, !((1.0, NaN) == (1.0, NaN)))

# --- named-tuple `==`: -0.0 / 0.0 compare equal --------------------------
push!(checks, (x = 0.0,) == (x = -0.0,))
push!(checks, (x = 1.0, y = 0.0) == (x = 1.0, y = -0.0))

# --- named-tuple `==`: NaN compares not-equal ----------------------------
push!(checks, !((x = NaN,) == (x = NaN,)))
push!(checks, !((x = 1.0, y = NaN) == (x = 1.0, y = NaN)))

# --- `!=` mirrors `==` on the float edge cases ---------------------------
push!(checks, !((0.0,) != (-0.0,)))
push!(checks, (NaN,) != (NaN,))
push!(checks, !((x = 0.0,) != (x = -0.0,)))
push!(checks, (x = NaN,) != (x = NaN,))

# --- `isequal` is the OTHER semantics and stays correct ------------------
# `isequal` distinguishes -0.0 / 0.0 and treats NaN as equal to NaN.
push!(checks, !isequal((0.0,), (-0.0,)))
push!(checks, isequal((NaN,), (NaN,)))
push!(checks, !isequal((x = 0.0,), (x = -0.0,)))
push!(checks, isequal((x = NaN,), (x = NaN,)))

# --- plain (non-float) `==` still works ----------------------------------
push!(checks, (1, 2) == (1, 2))
push!(checks, !((1, 2) == (1, 3)))
push!(checks, (x = 1, y = 2) == (x = 1, y = 2))
push!(checks, !((x = 1, y = 2) == (x = 1, y = 3)))

# --- nested tuples fold `==` recursively ---------------------------------
push!(checks, ((0.0,), 1) == ((-0.0,), 1))
push!(checks, !(((NaN,), 1) == ((NaN,), 1)))

all(checks)
