using StaticArrays

v = SVector(1, 2, 3)
m = @SMatrix [1 2; 3 4]

ok = length(v) == 3 &&
     size(v) == (3,) &&
     v[2] == 2 &&
     size(m) == (2, 2) &&
     m[2, 1] == 3 &&
     Size(v) == Size(3)

println((typeof(v), size(v), typeof(m), size(m), ok))
ok
