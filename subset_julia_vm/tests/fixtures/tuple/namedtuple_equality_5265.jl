# Issue #5265: `==` / `isequal` on named tuples must return `true` for
# structurally-equal named tuples. The `==` operator routes through the
# `BuiltinId::Isequal` Rust builtin (the same early route bare-tuple `==`
# uses); without a `Value::NamedTuple` arm equal named tuples fell through to
# `false`. All assertions match upstream Julia 1.12.

checks = Bool[]

# --- `==` on equal named tuples (the bare repro cases) -------------------
push!(checks, (a = 1, b = 2) == (a = 1, b = 2))
push!(checks, (a = 1,) == (a = 1,))
push!(checks, (x = 1, y = 2, z = 3) == (x = 1, y = 2, z = 3))

# --- `==` negatives ------------------------------------------------------
# Same names, different values.
push!(checks, !((a = 1, b = 2) == (a = 1, b = 3)))
# Same values, different names.
push!(checks, !((a = 1, b = 2) == (a = 1, c = 2)))
# Same names/values, different order — order is significant.
push!(checks, !((a = 1, b = 2) == (b = 2, a = 1)))
# Different arity.
push!(checks, !((a = 1, b = 2) == (a = 1,)))

# --- `!=` mirrors `==` ---------------------------------------------------
push!(checks, !((a = 1, b = 2) != (a = 1, b = 2)))
push!(checks, (a = 1, b = 2) != (a = 1, b = 3))

# --- `isequal` (Pure-Julia path) agrees ----------------------------------
push!(checks, isequal((a = 1, b = 2), (a = 1, b = 2)))
push!(checks, !isequal((a = 1, b = 2), (a = 1, b = 3)))
push!(checks, !isequal((a = 1, b = 2), (b = 2, a = 1)))

# `isequal` distinguishes -0.0 / 0.0 (this matches upstream; note that the
# `==`/-0.0/NaN aggregate distinction is the separate Issue #5267).
push!(checks, !isequal((x = 0.0,), (x = -0.0,)))

# --- `==` through variable bindings (not only inline literals) -----------
nt1 = (a = 1, b = 2)
nt2 = (a = 1, b = 2)
nt3 = (a = 1, b = 9)
push!(checks, nt1 == nt2)
push!(checks, !(nt1 == nt3))

# --- mixed element types compare by value --------------------------------
push!(checks, (a = 1, b = 2.0) == (a = 1, b = 2.0))
push!(checks, (a = 1, b = 2) == (a = 1, b = 2.0))

all(checks)
