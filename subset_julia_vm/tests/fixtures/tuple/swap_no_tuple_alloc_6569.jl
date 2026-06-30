# Issue #6569: tuple-literal destructuring swaps `a, b = b, a % b` lower to
# per-element temporaries (no tuple allocation). These results must match
# upstream Julia exactly, including the simultaneous-assignment semantics
# (all RHS elements evaluated before any target is written).

# Simple two-element swap.
function swap2_6569(a, b)
    a, b = b, a
    return (a, b)
end

# Three-cycle rotation.
function rotate3_6569(a, b, c, n)
    for _ in 1:n
        a, b, c = b, c, a
    end
    return a * 100 + b * 10 + c
end

# Four-element rotation.
function rotate4_6569(a, b, c, d)
    a, b, c, d = d, a, b, c
    return a * 1000 + b * 100 + c * 10 + d
end

# Dependent swap where the second element reads the *old* first target.
function gcd_swap_6569(a, b)
    while b != 0
        a, b = b, a % b
    end
    return a
end

# Mixed-type swap: each target keeps its own concrete type.
function mixed_swap_6569(a, s)
    a, s = a + 1, s
    return (a, s)
end

@assert swap2_6569(1, 2) == (2, 1)
@assert rotate3_6569(1, 2, 3, 1) == 231
@assert rotate3_6569(1, 2, 3, 3) == 123
@assert rotate4_6569(1, 2, 3, 4) == 4123
@assert gcd_swap_6569(1071, 462) == 21
@assert mixed_swap_6569(41, "x") == (42, "x")

true
