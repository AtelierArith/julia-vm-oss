# Issue #6561: self-referential destructuring swaps (`a, b = b, a % b`) lower to
# a temporary tuple plus indexed reads. The lazy specializer must keep the
# swapped bindings type-stable; these results must match upstream Julia exactly.

# Integer GCD via the Euclidean swap.
function gcd_swap_6561(a, b)
    while b != 0
        a, b = b, a % b
    end
    return a
end

# Fibonacci accumulated through a swap whose RHS references both targets.
function fib_swap_6561(n)
    a, b = 0, 1
    for _ in 1:n
        a, b = b, a + b
    end
    return a
end

# Float64 swap: the second element mixes both targets through float arithmetic.
function float_swap_6561(x, y, n)
    for _ in 1:n
        x, y = y, x + y * 0.5
    end
    return x
end

# Swap whose target is consumed downstream: `s += a` after the swap must stay
# typed (the case where type stability actually changes runtime instructions).
function swap_sum_6561(a, b, n)
    s = 0
    for _ in 1:n
        a, b = b, (a + b) % 1000003
        s += a
    end
    return s
end

@assert gcd_swap_6561(48, 36) == 12
@assert gcd_swap_6561(1071, 462) == 21
@assert fib_swap_6561(20) == 6765
@assert float_swap_6561(1.0, 2.0, 10) == 15.9921875
@assert swap_sum_6561(1, 1, 2000) == 999369993

true
