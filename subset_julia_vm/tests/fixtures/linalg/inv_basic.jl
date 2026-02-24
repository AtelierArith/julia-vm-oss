# Test matrix inverse using faer library
# Issue #626: Type-dispatched inv (Array -> faer builtin, Rational -> Pure Julia)

# Create a 2x2 matrix
A = [4.0 7.0; 2.0 6.0]

# Compute inverse
A_inv = inv(A)

# Verify: A * A_inv should be close to identity matrix
# det(A) = 4*6 - 7*2 = 24 - 14 = 10
# A_inv = (1/10) * [6 -7; -2 4] = [0.6 -0.7; -0.2 0.4]

# Check A * A_inv ≈ I
product = zeros(2, 2)
for i in 1:2
    for j in 1:2
        s = 0.0
        for k in 1:2
            s = s + A[i, k] * A_inv[k, j]
        end
        product[i, j] = s
    end
end

# Verify diagonal is 1, off-diagonal is 0
# Use round to handle floating point precision
diag_sum = round(product[1, 1]) + round(product[2, 2])
off_diag_sum = round(product[1, 2] * 1000) / 1000 + round(product[2, 1] * 1000) / 1000

# diag_sum should be 2 (1+1), off_diag_sum should be 0
result = diag_sum - off_diag_sum  # Expected: 2.0
