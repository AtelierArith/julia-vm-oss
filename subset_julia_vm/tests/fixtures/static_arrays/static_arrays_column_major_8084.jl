using StaticArrays

# Column-major storage must match upstream StaticArrays / Julia (Issue #8084).
# A flat tuple / vararg fills the matrix column-by-column, getindex(i,j) reads
# data[(j-1)*M + i], Tuple(m) exposes the column-major backing, and matrix
# products honour that layout.

# Flat-tuple constructor: SMatrix{2,2}((1,2,3,4)) == [1 3; 2 4]
m = SMatrix{2, 2}((1, 2, 3, 4))
# Literal macro: @SMatrix [1 2; 3 4] == [1 2; 3 4]
ml = @SMatrix [1 2; 3 4]
# 3x3 literal and products
m3 = @SMatrix [1 2 3; 4 5 6; 7 8 9]
mv = SMatrix{2, 2}((1.0, 2.0, 3.0, 4.0)) * SVector(1.0, 0.0)
mm = (@SMatrix [1 2; 3 4]) * (@SMatrix [5 6; 7 8])

ok = m[1, 1] == 1 && m[2, 1] == 2 && m[1, 2] == 3 && m[2, 2] == 4 &&
     Tuple(m) == (1, 2, 3, 4) &&
     ml[1, 1] == 1 && ml[2, 1] == 3 && ml[1, 2] == 2 && ml[2, 2] == 4 &&
     Tuple(ml) == (1, 3, 2, 4) &&
     Tuple(mv) == (1.0, 2.0) &&
     Tuple(m3) == (1, 4, 7, 2, 5, 8, 3, 6, 9) &&
     m3[1, 3] == 3 && m3[3, 1] == 7 &&
     Tuple(mm) == (19, 43, 22, 50)

println((Tuple(m), Tuple(ml), Tuple(mv), Tuple(mm), ok))
ok
