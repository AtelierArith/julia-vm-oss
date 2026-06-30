# Issue #8183: mixed-type primitive scalar arithmetic (e.g. `Int64 / Float64`)
# is specialized to the typed `…ToF64; <op>F64` path instead of a dynamic method
# `Call` to the Base operator. Julia promotes an integer-and-float pair to the
# float, so the result and its type must be identical to upstream.
#
# NB: this fixture's final value IS the conjunction of every check, so the
# nextest fixture harness fails on any regression. (sjulia's `@testset` does not
# throw on a failed `@test`, so a fixture that merely ends in `true` would pass
# regardless — see the harness `expected`-value check.)
add_if(a::Int64, b::Float64) = a + b
sub_fi(a::Float64, b::Int64) = a - b
mul_if(a::Int64, b::Float64) = a * b
div_if(a::Int64, b::Float64) = a / b
div_fi(a::Float64, b::Int64) = a / b

# Each check `&&`s into `ok`; the final value of `ok` is the conjunction. (A
# multi-line typed array literal `Bool[…]` would be cleaner but currently fails
# to parse in sjulia — #8188 — so use sequential `&&` instead.)
ok = true
# Value parity
ok = ok && (add_if(7, 2.0) === 9.0)
ok = ok && (sub_fi(2.0, 7) === -5.0)
ok = ok && (mul_if(3, 1.5) === 4.5)
ok = ok && (div_if(7, 2.0) === 3.5)
ok = ok && (div_fi(7.0, 2) === 3.5)
# Result type stays Float64 (no widening to Any).
ok = ok && (typeof(add_if(1, 2.0)) === Float64)
ok = ok && (typeof(mul_if(2, 2.5)) === Float64)
ok = ok && (typeof(div_if(1, 4.0)) === Float64)
# Integer→Float64 promotion is exactly Julia's (lossy beyond 2^53, identically).
ok = ok && (div_if(9007199254740993, 2.0) === 4.503599627370496e15)
ok = ok && (mul_if(9007199254740993, 1.0) === 9.007199254740992e15)
# Smaller integer widths and Bool also promote to the float operand type.
ok = ok && (Int32(5) / 4.0 === 1.25)
ok = ok && (true + 0.5 === 1.5)
ok = ok && (0x1 * 2.0 === 2.0)
# Float32 mixing stays Float32 (computed in F64, narrowed back).
ok = ok && (2.0f0 * 3 === 6.0f0)
ok = ok && (typeof(1 + 2.0f0) === Float32)
# Arithmetic is the only thing specialized; the `<` ordering comparison stays
# exact beyond 2^53 (a naive promote-to-Float64 would wrongly say equal). NB:
# `==` / `<=` / `>=` on this same pair currently mis-promote in sjulia — a
# pre-existing bug tracked as #8187, independent of this specialization.
ok = ok && ((9007199254740993 < 9.007199254740992e15) === false)
ok
