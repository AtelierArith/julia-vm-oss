using AbstractAlgebra

R, x = polynomial_ring(ZZ, "x")
S, y = polynomial_ring(QQ, "y")

p = (x + 1)^2
q = x^3 - x
r = (y + (1//2))^2

show_R = AbstractAlgebra._poly_ring_to_string(R)
show_p = AbstractAlgebra._poly_to_string(p)
show_S = AbstractAlgebra._poly_ring_to_string(S)
show_r = AbstractAlgebra._poly_to_string(r)

show(stdout, R); println()
show(stdout, p); println()
show(stdout, S); println()
show(stdout, r); println()

ok = show_R == "Univariate polynomial ring in x over integers" &&
     show_S == "Univariate polynomial ring in y over rationals" &&
     show_p == "x^2 + 2*x + 1" &&
     AbstractAlgebra._poly_to_string(q) == "x^3 - x" &&
     show_r == "y^2 + y + 1//4" &&
     parent(x) === R &&
     base_ring(R) === ZZ &&
     elem_type(R) === typeof(x) &&
     parent_type(x) === typeof(R) &&
     gen(R) == x &&
     gens(R)[1] == x &&
     degree(p) == 2 &&
     coeff(p, 0) == big(1) &&
     coeff(p, 1) == big(2) &&
     coeff(p, 2) == big(1) &&
     coeff(p, 4) == big(0) &&
     evaluate(p, big(3)) == big(16) &&
     derivative(p) == 2*x + 2 &&
     divexact(p, x + 1) == x + 1 &&
     zero(R) + p == p &&
     one(R) * p == p &&
     (x + 2) * (x - 2) == x^2 - 4 &&
     parent(y) === S &&
     base_ring(S) === QQ &&
     coeff(r, 1) == big(1)//big(1) &&
     evaluate(r, big(1)//big(2)) == big(1)//big(1) &&
     derivative(r) == 2*y + 1 &&
     divexact(r, y + (1//2)) == y + (1//2) &&
     AbstractAlgebra.polynomial_ring(AbstractAlgebra.ZZ, "z")[1] isa AbstractAlgebra.PolyRing

println(ok)
ok
