# Issue #7891: a filtered array comprehension `[r for r in 1:n if r != i]` whose
# `if` guard references a captured function argument (`i`) must keep the guard on
# EVERY specialization path. The reported bug dropped the guard on some
# specialization orders (cache/dispatch-order/HashMap-seed dependent class, per
# CLAUDE.md #5966), silently returning ALL iterator elements (e.g. rows=[1, 2]
# instead of [2]) and corrupting matrix-minor (cofactor) extraction
# `B[rows, cols]`. It is a SILENT wrong-output bug (no exception).
#
# The issue surfaced inside Symbolics `det`/`inv` cofactor expansion on
# `Matrix{Num}`, but the failing path is the comprehension+filter lowering itself,
# so this is a non-Symbolics reproduction (the issue notes the minimal
# `keep(n,i) = [r for r in 1:n if r != i]` form). The cases below cover the
# specialization orders the issue flags as toggling the bug: a
# println-materialized result, a tuple-returned result, a plain return called
# after another comprehension function, and the originating matrix-minor use over
# Int and Float64 matrices (forcing the same function to be re-specialized).
#
# This is a GENUINE regression guard: the final expression is `false` (=> the
# fixture FAILS) if the guard is dropped on any path. Verified against upstream
# julia 1.12.

# (1) println-materialized form (the exact order that triggered the bug).
function keep_println(n, i)
    rows = [r for r in 1:n if r != i]
    println("  keep_println n=$n i=$i rows=$rows")
    return rows
end

# (2) tuple-returned form.
function keep_tuple(n, m, i, j)
    rows = [r for r in 1:n if r != i]
    cols = [c for c in 1:m if c != j]
    return (rows, cols)
end

# (3) plain return form, called AFTER another comprehension function.
function keep_plain(n, i)
    return [r for r in 1:n if r != i]
end

# (4) matrix-minor extraction (the originating use-case).
function minor(B, i, j)
    n = size(B, 1); m = size(B, 2)
    rows = [r for r in 1:n if r != i]
    cols = [c for c in 1:m if c != j]
    return B[rows, cols]
end

ok1 = keep_println(2, 1) == [2]
ok2 = keep_println(3, 1) == [2, 3]
ok3 = keep_tuple(2, 2, 1, 1) == ([2], [2])
ok4 = keep_tuple(3, 3, 2, 3) == ([1, 3], [1, 2])
ok5 = keep_plain(4, 3) == [1, 2, 4]

A = [1 2; 3 4]
ok6 = minor(A, 1, 1) == reshape([4], 1, 1)
B3 = [1 2 3; 4 5 6; 7 8 9]
ok7 = minor(B3, 1, 1) == [5 6; 8 9]

# Guard must hold even when the same functions are re-specialized for Float64.
ok8 = keep_plain(3, 2) == [1, 3]
Af = [1.0 2.0; 3.0 4.0]
ok9 = minor(Af, 2, 1) == reshape([2.0], 1, 1)

@assert ok1 && ok2 && ok3 && ok4 && ok5 && ok6 && ok7 && ok8 && ok9

# Genuine regression guard: `false` (=> fixture fails) if any guard is dropped.
ok1 && ok2 && ok3 && ok4 && ok5 && ok6 && ok7 && ok8 && ok9
