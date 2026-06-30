using AbstractAlgebra

R, x = polynomial_ring(ZZ, "x")
F = fraction_field(R)

# Workaround: callable fraction-field parent dispatch fails for
# `F(num, den)` in sjulia, so this fixture uses the package helper. (Issue #8264)
a = AbstractAlgebra._frac_make(F, x + 1, x - 1)
b = AbstractAlgebra._frac_make(F, x - 1, x + 1)

Z5_tuple = residue_ring(ZZ, 5)
Z5 = Z5_tuple[1]
u = Z5(7)
v = Z5(4)

show_F = AbstractAlgebra._frac_field_to_string(F)
show_a = AbstractAlgebra._frac_to_string(a)
show_Z5 = AbstractAlgebra._residue_ring_to_string(Z5)
show_u = AbstractAlgebra._residue_to_string(u)

show(stdout, F); println()
show(stdout, a); println()
show(stdout, Z5); println()
show(stdout, u); println()

ok = show_F == "Fraction field of Univariate polynomial ring in x over integers" &&
     show_a == "(x + 1)/(x - 1)" &&
     parent(a) === F &&
     base_ring(F) === R &&
     elem_type(F) === typeof(a) &&
     parent_type(a) === typeof(F) &&
     AbstractAlgebra.numerator(a) == x + 1 &&
     AbstractAlgebra.denominator(a) == x - 1 &&
     a * b == one(F) &&
     a + a == AbstractAlgebra._frac_make(F, 2 * (x + 1), x - 1) &&
     show_Z5 == "Residue ring of integers modulo 5" &&
     show_u == "2" &&
     parent(u) === Z5 &&
     base_ring(Z5) === ZZ &&
     modulus(Z5) == big(5) &&
     modulus(u) == big(5) &&
     data(u) == big(2) &&
     lift(u) == big(2) &&
     elem_type(Z5) === typeof(u) &&
     parent_type(u) === typeof(Z5) &&
     u + v == Z5(1) &&
     u * v == Z5(3) &&
     -u == Z5(3) &&
     zero(Z5) + u == u &&
     one(Z5) * v == v &&
     is_unit(u) &&
     is_zero_divisor(Z5(0)) &&
     characteristic(Z5) == big(5) &&
     is_known(characteristic, Z5)

println(ok)
ok
