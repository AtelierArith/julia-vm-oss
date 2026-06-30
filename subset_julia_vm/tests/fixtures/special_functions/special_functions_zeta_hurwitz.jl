using SpecialFunctions

# Reference values from Julia's SpecialFunctions for the generalized (Hurwitz)
# zeta zeta(s, z) and the Dirichlet eta eta(s). (Issue #8310)
tol = 1e-6
ok = true

# Hurwitz zeta for z > 0
ok = ok && (abs(zeta(2.0, 3.0) - 0.39493406684822646) < tol)
ok = ok && (abs(zeta(3.0, 2.0) - 0.2020569031595943) < tol)
ok = ok && (abs(zeta(2.0, 1.5) - 0.9348022005446792) < tol)
ok = ok && (abs(zeta(4.0, 0.5) - 16.234848505667077) < tol)
ok = ok && (abs(zeta(2.5, 10.0) - 0.022728699194534543) < tol)
ok = ok && (abs(zeta(2.0, 0.25) - 17.197329154507113) < tol)
ok = ok && (abs(zeta(5.0, 2.5) - 0.013073166646113807) < tol)
ok = ok && (abs(zeta(2.0, 15.0) - 0.0689382278476838) < tol)

# s < 0 (analytic continuation) and fractional s
ok = ok && (abs(zeta(-1.0, 2.0) - (-1.0833333333333333)) < tol)
ok = ok && (abs(zeta(0.5, 3.0) - (-3.1674612899961345)) < tol)

# z == 1 reduces to the Riemann zeta
ok = ok && (abs(zeta(3.0, 1.0) - zeta(3.0)) < tol)
ok = ok && (abs(zeta(2.0, 1.0) - 1.6449340668482273) < tol)

# Hurwitz zeta for z < 0
ok = ok && (abs(zeta(2.0, -0.5) - 8.934802200544679) < tol)
ok = ok && (abs(zeta(2.0, -1.5) - 9.379246644989124) < tol)
ok = ok && (abs(zeta(3.0, -0.5) - 16.414398322117165) < tol)
ok = ok && (abs(zeta(2.0, -2.5) - 9.539246644989124) < tol)

# Integer arguments route through Float64
ok = ok && (abs(zeta(2, 3) - 0.39493406684822646) < tol)

# Dirichlet eta
ok = ok && (abs(eta(1.0) - 0.6931471805599453) < tol)   # ln 2 (Taylor branch)
ok = ok && (abs(eta(2.0) - 0.8224670334241136) < tol)   # pi^2/12
ok = ok && (abs(eta(0.0) - 0.5) < tol)
ok = ok && (abs(eta(-1.0) - 0.25) < tol)
ok = ok && (abs(eta(3.0) - 0.9015426773696964) < tol)
ok = ok && (abs(eta(0.5) - 0.6048986434216306) < tol)
ok = ok && (abs(eta(4.0) - 0.947032829497247) < tol)

ok
