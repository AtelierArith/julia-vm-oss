# Issue #6775: floor/ceil/round/trunc(::Rational) must return Rational (matching
# upstream Julia 1.12), not Float64. Typed forms floor(Int, ::Rational) etc.
# return the integer type. Verified against julia 1.12.

# --- bare value forms return Rational (positive) ---
@assert floor(7//2) === 3//1
@assert ceil(7//2) === 4//1
@assert round(7//2) === 4//1
@assert trunc(7//2) === 3//1
@assert typeof(floor(7//2)) === Rational{Int64}
@assert typeof(ceil(7//2)) === Rational{Int64}
@assert typeof(round(7//2)) === Rational{Int64}
@assert typeof(trunc(7//2)) === Rational{Int64}

# --- negative values ---
@assert floor(-7//2) === -4//1
@assert ceil(-7//2) === -3//1
@assert round(-7//2) === -4//1   # round-half-to-even: -3.5 -> -4
@assert trunc(-7//2) === -3//1
@assert typeof(floor(-7//2)) === Rational{Int64}

# --- exact rationals stay Rational ---
@assert floor(6//2) === 3//1
@assert ceil(6//2) === 3//1
@assert trunc(6//2) === 3//1
@assert round(6//2) === 3//1

# --- round half-to-even ties ---
@assert round(5//2) === 2//1   # 2.5 -> 2
@assert round(3//2) === 2//1   # 1.5 -> 2
@assert round(1//2) === 0//1   # 0.5 -> 0

# --- typed integer forms return the integer type ---
@assert floor(Int, 7//2) === 3
@assert ceil(Int, 7//2) === 4
@assert round(Int, 7//2) === 4
@assert trunc(Int, 7//2) === 3
@assert typeof(floor(Int, 7//2)) === Int64
@assert typeof(round(Int, 7//2)) === Int64
@assert floor(Int, -7//2) === -4
@assert ceil(Int, -7//2) === -3
@assert trunc(Int, -7//2) === -3
@assert round(Int, -7//2) === -4

# --- round with explicit RoundingMode returns Rational ---
@assert round(7//2, RoundDown) === 3//1
@assert round(7//2, RoundUp) === 4//1
@assert round(7//2, RoundToZero) === 3//1

# --- element type preserved (Int32) ---
r32 = Int32(7)//Int32(2)
@assert floor(r32) === Rational{Int32}(3, 1)
@assert typeof(floor(r32)) === Rational{Int32}

println("ok")
true
