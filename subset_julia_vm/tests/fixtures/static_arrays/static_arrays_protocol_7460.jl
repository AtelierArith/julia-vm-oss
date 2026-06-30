using StaticArrays

# Phase 4 static array protocol (Issue #7460): indexing, iteration, reductions,
# map, unary minus, broadcasting (scalar-mixed, elementwise, unary fused), and
# conversion all behave like upstream StaticArrays and return static results
# where upstream does. Type checks use `isa` (not `typeof ==`) so the same
# assertions hold under both sjulia and upstream Julia, whose `SMatrix` carries
# an extra `L` length parameter.

function forsum(v)
    s = zero(eltype(v))
    for x in v
        s += x
    end
    return s
end

v = SVector(1, 2, 3)
M = @SMatrix [1 2; 3 4]

# Scalar / shape-aware / linear indexing.
idx_ok = v[2] == 2 && M[2, 1] == 3 && M[1, 2] == 2 && M[3] == 2

# Iteration drives for-loops, reduce, and the reductions.
iter_ok = forsum(v) == 6 && reduce(+, v) == 6 && sum(v) == 6 && prod(v) == 6 &&
          minimum(SVector(3, 1, 2)) == 1 && maximum(SVector(3, 1, 2)) == 3 &&
          any(SVector(false, true)) && !all(SVector(true, false)) &&
          forsum(M) == 10

# map / unary minus preserve the static shape.
mv = map(x -> x * 2, v)
mm = map(x -> x + 1, M)
map_ok = Tuple(mv) == (2, 4, 6) && mv isa SVector{3,Int64} &&
         Tuple(mm) == (2, 4, 3, 5) && mm isa SMatrix{2,2,Int64} &&
         Tuple(-v) == (-1, -2, -3) && Tuple(-(M)) == (-1, -3, -2, -4)

# Broadcasting returns static arrays with upstream-compatible values.
b1 = v .+ 10
b2 = sin.(SVector(0.0, 1.5707963267948966))
bcast_ok = Tuple(b1) == (11, 12, 13) && b1 isa SVector{3,Int64} &&
           Tuple(10 .+ v) == (11, 12, 13) &&
           Tuple(v .- 1) == (0, 1, 2) &&
           Tuple(SVector(1, 2, 3) .+ SVector(4, 5, 6)) == (5, 7, 9) &&
           Tuple(2 .* v) == (2, 4, 6) &&
           Tuple(b2) == (0.0, 1.0) && b2 isa SVector{2,Float64} &&
           Tuple(abs.(SVector(-1, 2, -3))) == (1, 2, 3) &&
           Tuple(SVector(1, 2, 3) .^ 2) == (1, 4, 9) &&
           Tuple(SVector(2.0, 4.0, 6.0) ./ 2) == (1.0, 2.0, 3.0) &&
           Tuple(M .+ 10) == (11, 13, 12, 14) && (M .+ 10) isa SMatrix{2,2,Int64}

# Conversion coerces element types and preserves layout / Tuple round-trips.
cv = convert(SVector{3,Float64}, v)
cm = convert(SMatrix{2,2,Float64}, M)
conv_ok = Tuple(cv) == (1.0, 2.0, 3.0) && cv isa SVector{3,Float64} && cv[1] isa Float64 &&
          Tuple(cm) == (1.0, 3.0, 2.0, 4.0) && cm isa SMatrix{2,2,Float64} &&
          convert(SVector{3,Int64}, v) isa SVector{3,Int64} &&
          Tuple(v) == (1, 2, 3)

ok = idx_ok && iter_ok && map_ok && bcast_ok && conv_ok

println((idx_ok, iter_ok, map_ok, bcast_ok, conv_ok, ok))
ok
