using StaticArrays

m = @SMatrix [1 2; 3 4]
a = @SArray [1 2; 3 4]
row = @SMatrix [1 2 3]
col = @SMatrix [1; 2; 3]

ok = m isa SMatrix{2, 2, Int64} &&
     a isa SMatrix{2, 2, Int64} &&
     row isa SMatrix{1, 3, Int64} &&
     col isa SMatrix{3, 1, Int64} &&
     size(m) == (2, 2) &&
     size(row) == (1, 3) &&
     size(col) == (3, 1) &&
     Tuple(m) == (1, 3, 2, 4) &&
     Tuple(a) == (1, 3, 2, 4) &&
     Tuple(row) == (1, 2, 3) &&
     Tuple(col) == (1, 2, 3) &&
     m[2, 1] == 3 &&
     a[1, 2] == 2 &&
     row[1, 3] == 3 &&
     col[3, 1] == 3

println((typeof(m), typeof(a), Tuple(m), m[2, 1], ok))
ok
