using StaticArrays

v = SVector(1, 2, 3)
v2 = SVector{3, Int64}((1, 2, 3))
v3 = SVector{3}(1, 2, 3)
v4 = SVector{3, Int64}(1, 2, 3)
mv = @SVector [1, 2, 3]
m = SMatrix{2, 2, Int64}((1, 2, 3, 4))
m2 = SMatrix{2, 2}(1, 2, 3, 4)
m3 = SMatrix{2, 2, Int64}(1, 2, 3, 4)
a = SArray{Tuple{2, 2}, Int64, 2, 4}((1, 2, 3, 4))

ok = typeof(v) == SVector{3, Int64} &&
     typeof(v2) == SVector{3, Int64} &&
     typeof(v3) == SVector{3, Int64} &&
     typeof(v4) == SVector{3, Int64} &&
     typeof(mv) == SVector{3, Int64} &&
     v[2] == 2 &&
     v2[3] == 3 &&
     v3[1] == 1 &&
     v4[2] == 2 &&
     mv[1] == 1 &&
     m[2, 1] == 2 &&
     m2[2, 1] == 2 &&
     m3[1, 2] == 3 &&
     a[4] == 4 &&
     Tuple(mv) == (1, 2, 3) &&
     Tuple(m) == (1, 2, 3, 4) &&
     Tuple(m2) == (1, 2, 3, 4) &&
     Tuple(m3) == (1, 2, 3, 4)

println((typeof(v), typeof(m), Tuple(mv), m[2, 1], ok))
ok
