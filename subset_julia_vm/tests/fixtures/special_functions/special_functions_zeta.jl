using SpecialFunctions

# Reference values from Julia's SpecialFunctions.zeta (Riemann zeta)
tol = 1e-6
ok = true

# Positive integers (zeta(2) = pi^2/6, zeta(4) = pi^4/90)
ok = ok && (abs(zeta(2.0) - 1.6449340668482273) < tol)
ok = ok && (abs(zeta(3.0) - 1.2020569031595951) < tol)
ok = ok && (abs(zeta(4.0) - 1.0823232337111395) < tol)
ok = ok && (abs(zeta(5.0) - 1.0369277551433709) < tol)

# Integer argument routes through Float64
ok = ok && (abs(zeta(2) - 1.6449340668482273) < tol)

# Large s approaches 1
ok = ok && (abs(zeta(10.0) - 1.0009945751278184) < tol)
ok = ok && (abs(zeta(100.0) - 1.0) < tol)

# Fractional s >= 0.5 (main asymptotic path)
ok = ok && (abs(zeta(0.5) - (-1.4603545088095873)) < tol)
ok = ok && (abs(zeta(1.5) - 2.6123753486854886) < tol)

# s = 0 -> -1/2
ok = ok && (abs(zeta(0.0) - (-0.5)) < tol)

# Negative s via reflection formula
ok = ok && (abs(zeta(-1.0) - (-0.08333333333333338)) < tol)  # -1/12
ok = ok && (abs(zeta(-3.0) - 0.008333333333333345) < tol)    # 1/120
ok = ok && (abs(zeta(-0.5) - (-0.20788622497735462)) < tol)
ok = ok && (abs(zeta(-0.99) - (-0.0850001190598196)) < tol)

# Trivial zeros at negative even integers
ok = ok && (abs(zeta(-2.0)) < tol)

# Taylor branch for small |s|
ok = ok && (abs(zeta(1.0e-4) - (-0.5000919038861036)) < tol)

# Pole at s = 1
ok = ok && isnan(zeta(1.0))

ok
