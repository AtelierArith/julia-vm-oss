# Issue #10105: the peephole pass fuses a slot-vs-constant compare-and-branch
# (`LoadSlotI64(i); PushI64(n); <cmp>I64; JumpIfZero`, or the compiler's
# directional-jump form) into a single `JumpIfCmpI64SlotConst`. This fixture
# exercises every comparison predicate through constant-bounded `while` loops
# plus a hot counted loop, asserting results identical to upstream Julia.

# Lt guard (exit when i >= n)
function count_lt(n::Int64)
    i = 0
    c = 0
    while i < n
        c += 1
        i += 1
    end
    c
end

# Gt guard (exit when i <= 0)
function count_gt(n::Int64)
    i = n
    c = 0
    while i > 0
        c += 1
        i -= 1
    end
    c
end

# Le / Ge / Ne guards
function count_le(n::Int64)
    i = 0
    c = 0
    while i <= n
        c += 1
        i += 1
    end
    c
end

function count_ge(n::Int64)
    i = n
    c = 0
    while i >= 1
        c += 1
        i -= 1
    end
    c
end

function count_ne(n::Int64)
    i = 0
    c = 0
    while i != n
        c += 1
        i += 1
    end
    c
end

# Negated equality guard (`!(i == n)`) and a nested constant guard.
function sum_until(n::Int64)
    i = 0
    s = 0
    while !(i == n)
        if i < 3
            s += 10
        end
        s += i
        i += 1
    end
    s
end

# Hot counted loop with a constant bound: must stay correct (and on the
# typed-loop fast path) after the guard fuses.
function triangular(n::Int64)
    s = 0
    i = 0
    while i < n
        s += i
        i += 1
    end
    s
end

println(count_lt(5) == 5)
println(count_gt(7) == 7)
println(count_le(4) == 5)
println(count_ge(8) == 8)
println(count_ne(6) == 6)
println(sum_until(10) == 75)
println(triangular(1000) == 499500)
println(triangular(100000) == 4999950000)
# A constant bound also works when supplied inline.
println(count_lt(0) == 0)

# Final value is the conjunction of every check, so the fixture harness (which
# gates on the returned value) fails if ANY predicate loop diverges.
all_ok = count_lt(5) == 5 &&
         count_gt(7) == 7 &&
         count_le(4) == 5 &&
         count_ge(8) == 8 &&
         count_ne(6) == 6 &&
         sum_until(10) == 75 &&
         triangular(1000) == 499500 &&
         triangular(100000) == 4999950000 &&
         count_lt(0) == 0
all_ok
