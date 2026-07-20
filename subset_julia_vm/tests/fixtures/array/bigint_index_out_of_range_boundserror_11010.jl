# Issue #11010: an array index that is an out-of-i64-range `BigInt` integer
# index container must raise a catchable `BoundsError`, matching upstream
# Julia's `checkbounds` -- not a `TypeError`. `checkbounds` performs container
# membership without ever converting the endpoints to `Int`, so an oversized
# `BigInt` endpoint is still recognized as an *integer* index, just one that
# is out of bounds for the (small) target array.

a = [10, 20, 30, 40]
f(a, k) = a[k]

# --- scalar-shaped range: a[h:h] with h just above typemax(Int64) ------------
h = big(typemax(Int64)) + big(1)
err1 = nothing
try
    f(a, h:h)
catch e
    global err1 = e
end
@assert err1 isa BoundsError

# --- wider range: a[h:(h+2)] ------------------------------------------------
err2 = nothing
try
    f(a, h:(h + big(2)))
catch e
    global err2 = e
end
@assert err2 isa BoundsError

# --- negative BigInt endpoint, below typemin(Int64) -------------------------
n = -(big(typemax(Int64)) + big(2))
err3 = nothing
try
    f(a, n:n)
catch e
    global err3 = e
end
@assert err3 isa BoundsError

# --- in-range BigInt ranges keep working (no regression) -------------------
@assert f(a, big(2):big(3)) == [20, 30]
b = big(1)
@assert f(a, b:b) == [10]

true
