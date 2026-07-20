# Issue #10566(c): the typed-loop array-store fast path resolves each STORED
# array local to a PRIVATE transactional buffer and commits it back at block
# exit. That model only stays correct if two live-in locals can never reach the
# SAME backing storage -- otherwise a stored local's writes are invisible to the
# other local's reads mid-block, and two stored buffers over one storage clobber
# each other at commit (a LOST STORE).
#
# The block-entry alias check must therefore key on BACKING STORAGE (the `Memory`
# Rc plus the element window), not on the `Array` wrapper's identity: `reshape`
# and `view` produce DISTINCT wrapper objects over one shared `Memory`. Keying
# the check on the wrapper (the first cut of this change, caught in adversarial
# review) missed exactly these shapes.
#
# Every case here must produce the SAME result as upstream `julia` (verified
# directly): the loop is expected to REJECT the typed fast path and fall back to
# the generic interpreter's per-element dispatch, which handles aliasing.

# --- reshape: distinct wrapper, same Memory, BOTH locals stored --------------
function bump_both!(x, y, n)
    for i in 1:n
        x[i] = x[i] + 1
        y[i] = y[i] + 1
    end
    return x
end

a = zeros(Int64, 3)
b = reshape(a, 3)            # different wrapper object, same backing Memory
bump_both!(a, b, 3)
# Each element is incremented TWICE per iteration (once through each alias).
@assert a == [2, 2, 2]
@assert b == [2, 2, 2]

# --- reshape: one local stored, the other only read --------------------------
function store_and_read!(x, y, n)
    s = 0
    for i in 1:n
        x[i] = i * 10
        s += y[i]            # must observe x's write in the SAME iteration
    end
    return s
end

p = zeros(Int64, 4)
q = reshape(p, 4)
s = store_and_read!(p, q, 4)
@assert p == [10, 20, 30, 40]
@assert q == [10, 20, 30, 40]
@assert s == 100             # 10 + 20 + 30 + 40, not 0 from a stale snapshot

# --- view: overlapping WINDOW into one Memory (offset, partial overlap) ------
function fill_five!(v, n)
    for i in 1:n
        v[i] = 5
    end
    return v
end

c = [0, 0, 0, 0, 0]
w = view(c, 2:4)             # shares c's Memory at offset 2, length 3
fill_five!(w, 3)
@assert c == [0, 5, 5, 5, 0] # the write went through to the parent's Memory

# --- the NON-aliasing twin still works (no over-rejection) ------------------
function map_plus!(y, x, n)
    for i in 1:n
        y[i] = x[i] + 1
    end
    return y
end

xs = [10, 20, 30]
ys = zeros(Int64, 3)         # distinct storage: must stay on the fast path
map_plus!(ys, xs, 3)
@assert ys == [11, 21, 31]
@assert xs == [10, 20, 30]

true
