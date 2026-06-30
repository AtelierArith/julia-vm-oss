using AbstractAlgebra

@req true "Phase2 macro import gate"

# Bare references must resolve after `using AbstractAlgebra`.
PolynomialElem
MatrixElem
MatSpace
UniversalRing
is_diagonal
is_hermitian
is_symmetric
is_lower_triangular
is_upper_triangular

A = [1 0; 0 2]
L = [1 0; 3 2]
U = [1 3; 0 2]

exports = names(AbstractAlgebra)
ok = (:PolynomialElem in exports) &&
     (:MatrixElem in exports) &&
     (:UniversalRing in exports) &&
     (:is_empty in exports) &&
     (:is_diagonal in exports) &&
     (:is_hermitian in exports) &&
     (:is_symmetric in exports) &&
     (:is_lower_triangular in exports) &&
     (:is_upper_triangular in exports) &&
     (:characteristic_polynomial in exports) &&
     isdefined(AbstractAlgebra, :PolynomialElem) &&
     isdefined(AbstractAlgebra, :MatrixElem) &&
     isdefined(AbstractAlgebra, :UniversalRing) &&
     is_empty(Int[]) &&
     is_zero(0) &&
     isdefined(AbstractAlgebra, :characteristic_polynomial) &&
     AbstractAlgebra.is_diagonal(A) &&
     AbstractAlgebra.is_hermitian(A) &&
     AbstractAlgebra.is_symmetric(A) &&
     AbstractAlgebra.is_lower_triangular(L) &&
     AbstractAlgebra.is_upper_triangular(U)

println(ok)
ok
