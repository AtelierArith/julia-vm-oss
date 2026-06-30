using AbstractAlgebra

G = SymmetricGroup(5)
g = Perm([2, 3, 1, 5, 4])
h = Perm([1, 3, 4, 2, 5])

product = g * h
inverse = inv(g)
cube = g^3

show_G = AbstractAlgebra._symmetric_group_to_string(G)
show_g = AbstractAlgebra._perm_to_string(g)
show_product = AbstractAlgebra._perm_to_string(product)
show_inverse = AbstractAlgebra._perm_to_string(inverse)
show_cube = AbstractAlgebra._perm_to_string(cube)

show(stdout, G); println()
show(stdout, g); println()
show(stdout, product); println()
show(stdout, inverse); println()
show(stdout, cube); println()
println(sign(g))
println(permtype(g))

ok = show_G == "Full symmetric group over 5 elements" &&
     show_g == "(1,2,3)(4,5)" &&
     show_product == "(1,3)(2,4,5)" &&
     show_inverse == "(1,3,2)(4,5)" &&
     show_cube == "(4,5)" &&
     parent(g) == G &&
     elem_type(G) === typeof(g) &&
     parent_type(g) === typeof(G) &&
     one(G) == Perm([1, 2, 3, 4, 5]) &&
     isone(one(G)) &&
     !isone(g) &&
     product == Perm([3, 4, 1, 5, 2]) &&
     inverse == Perm([3, 1, 2, 5, 4]) &&
     cube == Perm([1, 2, 3, 5, 4]) &&
     g * inverse == one(G) &&
     inverse * g == one(G) &&
     sign(g) == -1 &&
     parity(g) == 1 &&
     permtype(g) == [3, 2] &&
     length(G) == 120 &&
     number_of_generators(G) == 2 &&
     AbstractAlgebra.ngens(G) == 2

println(ok)
ok
