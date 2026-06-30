using StaticArrays

v = SVector(1, 2, 3)
println((typeof(v), length(v), v[2]))

length(v) == 3 && v[2] == 2
