using Test

# Issue #7333: a `Vector` produced by slicing a matrix (`m[:, 1]`, `m[1, :]`) —
# and `collect` of such a slice — must dispatch to `::Vector` methods exactly
# like a literal `Vector` does. Before the fix the slice inferred to the
# unparameterized `Array` (rank unknown), so the static dispatcher raised
# `MethodError: no method matching f(::Array)` even though `typeof`/`isa` report
# `Vector{Float64}`. A full slice (`m[:, :]`) keeps rank 2 and stays a `Matrix`.
@testset "Issue #7333: matrix slice carrier dispatches by rank" begin
    m = [1.0 2.0 3.0; 4.0 5.0 6.0]   # 2x3 Matrix{Float64}

    onlyvec(y::Vector) = length(y)
    onlymat(x::Matrix) = size(x)

    # typeof / isa already report a Vector; dispatch must agree.
    @test typeof(m[:, 1]) === Vector{Float64}
    @test m[:, 1] isa Vector

    # Column and row slices are rank-1 Vectors and reach `onlyvec(::Vector)`.
    @test onlyvec(m[:, 1]) == 2
    @test onlyvec(m[1, :]) == 3

    # `collect` of a slice is still a Vector and dispatches the same way.
    @test onlyvec(collect(m[:, 1])) == 2

    # A full slice keeps rank 2 and reaches `onlymat(::Matrix)`.
    @test onlymat(m[:, :]) == (2, 3)

    # A range slice over a vector is rank-1.
    v = collect(1.0:5.0)
    @test onlyvec(v[2:4]) == 3
end

true
