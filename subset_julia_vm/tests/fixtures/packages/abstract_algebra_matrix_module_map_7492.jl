using AbstractAlgebra

M = matrix_space(ZZ, 2, 2)
A = matrix(ZZ, 2, 2, [1 2; 3 4])
B = matrix(ZZ, 2, 2, [0 1; 1 0])
I2 = identity_matrix(ZZ, 2)

Q = matrix_space(QQ, 2, 2)
C = matrix(QQ, 2, 2, [1//2 1//3; 2//3 3//4])

R, x = polynomial_ring(ZZ, "x")
P = matrix_space(R, 2, 2)
PX = matrix(R, 2, 2, [x 1; 0 x + 1])

F = free_module(ZZ, 2)
v = gen(F, 1) + 2 * gen(F, 2)

function twice(w)
   return w + w
end

id = identity_map(F)
phi = hom(F, F, twice)

show_M = AbstractAlgebra._matrix_space_to_string(M)
show_A = AbstractAlgebra._matrix_to_string(A)
show_F = AbstractAlgebra._free_module_to_string(F)
show_v = AbstractAlgebra._free_module_elem_to_string(v)

show(stdout, M); println()
show(stdout, A); println()
show(stdout, F); println()
show(stdout, v); println()

ok = show_M == "Matrix space of 2 rows and 2 columns over Integers" &&
     show_A == "[1   2]\n[3   4]" &&
     show_F == "Free module of rank 2 over Integers" &&
     show_v == "(1, 2)" &&
     parent(A) isa AbstractAlgebra.MatSpace &&
     base_ring(A) === ZZ &&
     elem_type(M) === typeof(A) &&
     parent_type(A) === typeof(M) &&
     AbstractAlgebra.nrows(A) == 2 &&
     AbstractAlgebra.ncols(A) == 2 &&
     size(A, 1) == 2 &&
     size(A, 2) == 2 &&
     A[1, 1] == big(1) &&
     typeof(A[1, 1]) === BigInt &&
     A + B == matrix(ZZ, 2, 2, [1 3; 4 4]) &&
     A - B == matrix(ZZ, 2, 2, [1 1; 2 4]) &&
     A * B == matrix(ZZ, 2, 2, [2 1; 4 3]) &&
     transpose(A) == matrix(ZZ, 2, 2, [1 3; 2 4]) &&
     I2 * A == A &&
     isone(I2) &&
     iszero(zero_matrix(ZZ, 2, 2)) &&
     det(A) == big(-2) &&
     tr(A) == big(5) &&
     rank(A) == 2 &&
     det(C) == (1//2) * (3//4) - (1//3) * (2//3) &&
     tr(C) == (1//2) + (3//4) &&
     det(PX) == x^2 + x &&
     tr(PX) == 2*x + 1 &&
     number_of_generators(F) == 2 &&
     AbstractAlgebra.ngens(F) == 2 &&
     parent(v) === F &&
     v[1] == big(1) &&
     v[2] == big(2) &&
     id(v) == v &&
     domain(id) === F &&
     codomain(id) === F &&
     phi(v) == v + v &&
     domain(phi) === F &&
     codomain(phi) === F

println(ok)
ok
