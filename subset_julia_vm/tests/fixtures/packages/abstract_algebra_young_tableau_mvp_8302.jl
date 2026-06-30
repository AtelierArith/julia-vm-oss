using AbstractAlgebra

p = AbstractAlgebra.Generic.Partition([4, 2, 1, 1, 1])
y = AbstractAlgebra.Generic.YoungTableau([4, 3, 1])

partition_diagram = AbstractAlgebra._young_diagram_to_string(p)
tableau_diagram = AbstractAlgebra._young_tableau_to_string(y)

println(p.n)
println(p.part)
println(partition_diagram)
println(size(y))
println(y[1])
println(y[2])
println(y[4])
println(y[6])
println(tableau_diagram)

ok = p.n == 9 &&
     p.part == [4, 2, 1, 1, 1] &&
     AbstractAlgebra.Generic.Partition === Partition &&
     AbstractAlgebra.Generic.YoungTableau === YoungTableau &&
     size(p) == (5,) &&
     p[1] == 4 &&
     sum(p) == 9 &&
     partition_diagram == "####\n##\n#\n#\n#" &&
     y.part == Partition([4, 3, 1]) &&
     y.fill == [1, 2, 3, 4, 5, 6, 7, 8] &&
     size(y) == (3, 4) &&
     y[1] == 1 &&
     y[2] == 5 &&
     y[4] == 2 &&
     y[6] == 0 &&
     tableau_diagram == "1 2 3 4\n5 6 7\n8"

println(ok)
ok
