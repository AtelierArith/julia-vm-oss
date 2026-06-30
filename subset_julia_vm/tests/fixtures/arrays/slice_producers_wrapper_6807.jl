# Issue #6807: the array-slice producers in `exec/array_index_slice.rs`
# (`a[range]`, `a[indexvec]`, `m[rows, cols]`, n-dim slices) now emit the
# MemoryRef-backed `Array{T,N}` wrapper instead of the legacy native carrier.
#
# A slice is a fresh array (sjulia materializes a copy, not a view), so it must
# be independently mutable and must not alias the parent. Verified against
# upstream Julia 1.12.6.

using Test

@testset "slice_producers_wrapper_6807: 1-D slices" begin
    a = [10, 20, 30, 40, 50]
    @test a[2:4] == [20, 30, 40]
    @test a[[1, 3, 5]] == [10, 30, 50]
    @test a[1:2:5] == [10, 30, 50]
    @test typeof(a[2:4]) == Vector{Int64}
    @test length(a[2:4]) == 3
end

@testset "slice_producers_wrapper_6807: 2-D slices" begin
    m = [1 2 3; 4 5 6; 7 8 9]
    @test m[1:2, 2:3] == [2 3; 5 6]
    @test m[:, 2] == [2, 5, 8]
    @test m[2, :] == [4, 5, 6]
    @test m[[1, 3], [1, 3]] == [1 3; 7 9]
    @test size(m[1:2, 2:3]) == (2, 2)
end

@testset "slice_producers_wrapper_6807: slice is a fresh mutable array" begin
    a = [10, 20, 30, 40, 50]
    s = a[2:4]
    push!(s, 99)
    @test s == [20, 30, 40, 99]
    @test a == [10, 20, 30, 40, 50]      # parent unchanged

    s[1] = 0
    @test s == [0, 30, 40, 99]
    @test a == [10, 20, 30, 40, 50]      # parent still unchanged

    col = [1 2; 3 4][:, 1]
    push!(col, 5)
    @test col == [1, 3, 5]
end

@testset "slice_producers_wrapper_6807: float + slice-of-slice" begin
    a = [1.0, 2.0, 3.0, 4.0]
    @test a[2:3] == [2.0, 3.0]
    @test a[2:4][1:2] == [2.0, 3.0]
    @test eltype(a[2:3]) == Float64
end

true
