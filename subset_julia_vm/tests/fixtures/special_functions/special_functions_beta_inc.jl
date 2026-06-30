using SpecialFunctions

tol = 1e-5
ok = true

# beta_inc(a, b, x) reference values from Julia's SpecialFunctions.beta_inc
ok = ok && abs(beta_inc(1.0, 1.0, 0.5) - 0.5) < tol
ok = ok && abs(beta_inc(2.0, 2.0, 0.5) - 0.5) < tol
ok = ok && abs(beta_inc(2.0, 3.0, 0.4) - 0.5248) < tol
# Asymmetric complementary branch: x > (a+1)/(a+b+2), so the else branch is exercised.
ok = ok && abs(beta_inc(2.0, 3.0, 0.8) - 0.9728) < tol

ok
