# Issue #10787: broadcasting over an Any/abstract-eltype array must preserve
# each element's own result type (upstream copyto_nonleaf!/promote_typejoin
# semantics) instead of coercing every result to the first element's type
# (2.5 + 2.5 silently became 5).

double(x) = x + x

mixed = Any[1, 2.5, 3, 4.5]
result = double.(mixed)
@assert result == [2, 5.0, 6, 9.0]
@assert typeof(result) == Vector{Real}
@assert typeof(result[1]) == Int64
@assert typeof(result[2]) == Float64

# Operator broadcast over Any-eltype narrows the same way.
plus1 = Any[1, 2.5] .+ 1
@assert plus1 == [2, 3.5]
@assert typeof(plus1) == Vector{Real}

# Multi-dimensional Any storage keeps its shape through the widening path.
m = Array{Any}(undef, 2, 2)
m[1] = 1
m[2] = 2.5
m[3] = 3
m[4] = 4.5
r2 = double.(m)
@assert typeof(r2) == Matrix{Real}
@assert size(r2) == (2, 2)
@assert r2[1, 1] == 2
@assert r2[2, 1] == 5.0

# Homogeneous results narrow to their concrete eltype.
same = Any[1, 2, 3]
@assert typeof(double.(same)) == Vector{Int64}

# Mixed non-numeric results stay Any.
tostr(x) = x isa Int ? string(x) : x
mixedany = Any[1, 2.5]
@assert typeof(tostr.(mixedany)) == Vector{Any}

# Empty Any broadcast stays Vector{Any}.
@assert typeof(double.(Any[])) == Vector{Any}

# Concrete-eltype broadcasts keep their existing fast paths.
@assert typeof([1.0, 2.0] .+ 1) == Vector{Float64}
@assert typeof([1, 2] .* 2) == Vector{Int64}

println("All broadcast Any-eltype widening tests passed")
true
