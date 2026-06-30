using StaticArrays

v = SVector(1, 2, 3)
v2 = SVector{3, Int64}((1, 2, 3))
m = SMatrix{2, 2, Int64}((1, 2, 3, 4))
a = SArray{Tuple{2, 2}, Int64, 2, 4}((1, 2, 3, 4))

ok = typeof(v) == SVector{3, Int64} &&
     typeof(v2) == SVector{3, Int64} &&
     m isa SMatrix{2, 2, Int64} &&
     a isa SArray{Tuple{2, 2}, Int64, 2, 4} &&
     Size(v) == Size(3) &&
     Size(typeof(v)) == Size(3) &&
     Length(v) == Length(3) &&
     Length(typeof(m)) == Length(4) &&
     size(v) == (3,) &&
     size(m) == (2, 2) &&
     size(a) == (2, 2) &&
     length(v) == 3 &&
     length(m) == 4 &&
     length(a) == 4 &&
     eltype(v) == Int64 &&
     eltype(typeof(m)) == Int64 &&
     ndims(v) == 1 &&
     ndims(m) == 2 &&
     Tuple(v) == (1, 2, 3) &&
     Tuple(m) == (1, 2, 3, 4) &&
     tuple_length((2, 3, 4)) == 3 &&
     tuple_prod((2, 3, 4)) == 24 &&
     tuple_minimum((4, 2, 3)) == 2 &&
     size_to_tuple(Size(m)) == (2, 2) &&
     check_array_parameters(SArray{Tuple{2, 2}, Int64, 2, 4})

println((typeof(v), typeof(m), size(v), size(m), Length(m), ok))
ok
