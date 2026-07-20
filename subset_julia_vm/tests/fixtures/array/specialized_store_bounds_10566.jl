# Issue #10566: an out-of-bounds index on a MemoryRef-backed Vector must raise a
# catchable `BoundsError` -- with the SAME error type and message -- whether or
# not the enclosing function got runtime-specialized.
#
# Regression guard: blocker (a) made untyped array-argument functions
# specializable, and the specialized body's `IndexLoadTyped`/`IndexStoreTyped`
# delegate to the generic `IndexLoad`/`IndexStore` arms. Those arms' MemoryRef
# fast path reports an out-of-range index as a shape-based internal error
# ("Index [10] out of bounds for array with shape [3]") rather than a
# `BoundsError`, so the delegation is now taken only for IN-BOUNDS indices; an
# out-of-bounds index falls back to `getindex`/`setindex!` dispatch, which
# raises the upstream-compatible `BoundsError`.

# --- store out of bounds, inside an untyped (specialized) function -----------
function fill_oob!(a, n)
    for i in 1:n
        a[i] = 1.0 * i
    end
    return a
end

a = zeros(Float64, 3)
store_err = nothing
try
    fill_oob!(a, 10)
catch e
    global store_err = e
end
@assert store_err isa BoundsError
# Stores that precede the error are still applied (upstream semantics).
@assert a == [1.0, 2.0, 3.0]

# --- load out of bounds, inside an untyped (specialized) function ------------
function sum_oob(x, n)
    s = 0
    for i in 1:n
        s += x[i]
    end
    return s
end

load_err = nothing
try
    sum_oob([1, 2, 3], 10)
catch e
    global load_err = e
end
@assert load_err isa BoundsError

# --- top-level load out of bounds (the unspecialized path) ------------------
top_err = nothing
try
    [1, 2, 3][10]
catch e
    global top_err = e
end
@assert top_err isa BoundsError

# Specialized and unspecialized bodies are indistinguishable to try/catch.
@assert typeof(load_err) === typeof(top_err)

true
