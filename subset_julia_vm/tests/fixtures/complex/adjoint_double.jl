# Test that double conjugate returns original complex number
# conj(conj(z)) == z
#
# Note: Uses Complex{Float64} constructor directly.
# Note: adjoint() for scalars is equivalent to conj()

# Test with Float64 complex numbers
z = Complex{Float64}(1.0, 2.0)

# Get conjugate
w = conj(z)  # w = 1.0 - 2.0im

# Get conjugate again (back to original)
x = conj(w)  # x = 1.0 + 2.0im

# Verify x == z (component-wise)
check1 = x.re == z.re
check2 = x.im == z.im

# Also verify w is correctly conjugated
check3 = w.re == z.re
check4 = w.im == -z.im

check1 && check2 && check3 && check4
