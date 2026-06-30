using SpecialFunctions

# Reference values from Julia's SpecialFunctions
tol = 1e-6
ok = true

# gamma at small positive integers
ok = ok && (abs(gamma(1.0) - 1.0) < tol)
ok = ok && (abs(gamma(2.0) - 1.0) < tol)
ok = ok && (abs(gamma(3.0) - 2.0) < tol)
ok = ok && (abs(gamma(4.0) - 6.0) < tol)

# loggamma
ok = ok && (abs(loggamma(2.5) - 0.28468287047291908) < tol)
ok = ok && (abs(loggamma(5.0) - log(24.0)) < tol)

ok
