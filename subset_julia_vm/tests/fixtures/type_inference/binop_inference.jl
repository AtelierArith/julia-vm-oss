# Issue #3524: Binary operators should not collapse to unknown_binop
# Returned types should be inferable for ===, !==, %, ÷, ^, &&, ||
function f(a, b)
    x = a % b
    y = a ÷ b
    z = a === b
    return (x, y, z)
end

@assert f(7, 3) == (1, 2, false)

g(a, b) = a && b
@assert g(true, false) == false
@assert g(true, true) == true

h(a, b) = a || b
@assert h(false, true) == true
@assert h(false, false) == false

# Identity/inequality
ne(a, b) = a !== b
@assert ne(1, 1) == false
@assert ne(1, 2) == true

# Power
p(a, b) = a ^ b
@assert p(2, 3) == 8

true
