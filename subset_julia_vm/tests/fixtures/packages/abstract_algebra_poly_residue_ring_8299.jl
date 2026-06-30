using AbstractAlgebra

R, x = polynomial_ring(ZZ, :x)
Q = residue_ring(R, x^2 + x + 1)[1]
q = Q(x)

a = q + 1
b = q - 1
zero_relation = q^2 + q + 1
product = a * b

show_Q = AbstractAlgebra._poly_residue_ring_to_string(Q)
show_q = AbstractAlgebra._poly_residue_to_string(q)
show_zero = AbstractAlgebra._poly_residue_to_string(zero_relation)
show_product = AbstractAlgebra._poly_residue_to_string(product)

show(stdout, Q); println()
show(stdout, q); println()
println(zero_relation)
println(product)

ok = show_Q == "Residue ring of Univariate polynomial ring in x over integers modulo x^2 + x + 1" &&
     show_q == "x" &&
     show_zero == "0" &&
     show_product == "-x - 2" &&
     parent(q) === Q &&
     base_ring(Q) === R &&
     modulus(Q) == x^2 + x + 1 &&
     modulus(q) == x^2 + x + 1 &&
     data(q) == x &&
     lift(q) == x &&
     elem_type(Q) === typeof(q) &&
     parent_type(q) === typeof(Q) &&
     zero(Q) + q == q &&
     one(Q) * q == q &&
     iszero(zero_relation) &&
     q^2 == -q - 1 &&
     q^3 == 1

println(ok)
ok
