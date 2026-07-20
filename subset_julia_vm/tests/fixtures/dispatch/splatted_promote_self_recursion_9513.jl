# Issue #9513: a splatted self-recursive call in upstream's canonical numeric
# form `f(x::Integer, y::Integer) = f(promote(x, y)...)` must dispatch the
# splatted promoted tuple on its concrete element types, selecting the more
# specific diagonal `f(x::T, y::T) where {T<:Integer}` — not re-select the same
# `(Integer, Integer)` method and recurse to a StackOverflow.

myop(x::Integer, y::Integer) = myop(promote(x, y)...)
myop(x::T, y::T) where {T<:Integer} = x + y

# Mixed integer widths that promote to a common type: the diagonal method must
# be chosen from the promoted (equal-typed) tuple. Convert to Int for display so
# the check is independent of unsigned print formatting.
r1 = Int(myop(4, UInt128(5)))
r2 = Int(myop(Int8(3), Int64(4)))
r3 = Int(myop(0x01, 0x0002))      # UInt8, UInt16 -> UInt16
r4 = Int(myop(7, 8))              # already Int, Int

println(r1)
println(r2)
println(r3)
println(r4)

# The two-variable form (already correct) and the splat form must agree.
myop2(x::Integer, y::Integer) = (p = promote(x, y); myop2(p[1], p[2]))
myop2(x::T, y::T) where {T<:Integer} = x + y

println(Int(myop2(4, UInt128(5))) == r1)

@assert r1 == 9 && r2 == 7 && r3 == 3 && r4 == 15
@assert Int(myop2(4, UInt128(5))) == r1

true
