# Issue #9169: a scalar binary op on an integer-component Complex value built
# via the `a + b*im` literal path returned `ComplexF64` instead of preserving
# `Complex{Int64}` (and other integer-family element types). Root cause: (1) a
# bare `::Complex` method parameter (`Base.:+(x::Real, z::Complex) = ...`)
# resolved at compile time to an arbitrarily-guessed concrete instantiation
# instead of `Any`, and (2) the Complex-result-type recovery in
# `compile_user_defined_binary_op` promoted the method's *declared* (bare)
# parameter types instead of the actual call-site argument types, silently
# defaulting to `Complex{Float64}` whenever the declared type had no element
# parameter to inspect. Direct `Complex(a, b)` construction (bypassing the
# `+`/`*` operator methods) was already correct and is included here as a
# same-fixture regression guard.

# --- Complex{Int64} via the `a + b*im` literal path ---
z = 2 + 3im
@assert typeof(z) == Complex{Int64}
@assert typeof(z + z) == Complex{Int64}
@assert (z + z) == Complex(4, 6)
@assert typeof(z - z) == Complex{Int64}
@assert (z - z) == Complex(0, 0)
@assert typeof(z * z) == Complex{Int64}
@assert (z * z) == Complex(-5, 12)
# Division always promotes to Float64 in upstream Julia, even for integer
# components -- this is correct/expected, not part of the bug.
@assert typeof(z / z) == ComplexF64
@assert (z / z) == Complex(1.0, 0.0)

# --- Complex{Int64} via the `Complex(a, b)` constructor path (already correct
#     before this fix; kept as a same-fixture regression guard) ---
w = Complex(2, 3)
@assert typeof(w) == Complex{Int64}
@assert typeof(w + w) == Complex{Int64}
@assert (w + w) == Complex(4, 6)
@assert typeof(w * w) == Complex{Int64}
@assert (w * w) == Complex(-5, 12)

# --- Both literal and constructor paths agree ---
@assert (z + z) == (w + w)
@assert typeof(z + z) == typeof(w + w)

# --- Generality (Issue #9169 / AGENTS.md principle 10): not just Int64 ---
zi32 = Int32(2) + Int32(3) * im
@assert typeof(zi32) == Complex{Int32}
@assert typeof(zi32 + zi32) == Complex{Int32}
@assert (zi32 + zi32) == Complex(Int32(4), Int32(6))

zbool = true + false * im
@assert typeof(zbool) == Complex{Int64}
@assert typeof(zbool + zbool) == Complex{Int64}
@assert (zbool + zbool) == Complex(2, 0)

# --- Float64 Complex arithmetic must remain unaffected by the fix ---
zf = 2.0 + 3.0im
@assert typeof(zf) == ComplexF64
@assert typeof(zf + zf) == ComplexF64
@assert (zf + zf) == Complex(4.0, 6.0)

println("ok")
true
