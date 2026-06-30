using Test

# Regression test for Issue #5812: the public `collect(::SubArray)` dispatch path
# for a 2D view must yield a Matrix of the view's shape (not route through the
# Array collect path and return a flat Vector of the parent's linear storage).
# Resolved on main; this guards against regression.

@testset "collect of a 2D view dispatches to SubArray collect (Issue #5812)" begin
    A = reshape(collect(1:9), 3, 3)
    v = view(A, 1:2, 2:3)
    c = collect(v)
    @test c == [4 7; 5 8]
    @test typeof(c) == Matrix{Int64}
    @test size(c) == (2, 2)

    # Float and column-selected views
    Af = reshape(collect(1.0:9.0), 3, 3)
    @test collect(view(Af, 2:3, 1:2)) == [2.0 5.0; 3.0 6.0]

    # 1D view collect stays a Vector
    a = [10, 20, 30, 40, 50]
    @test collect(view(a, 2:4)) == [20, 30, 40]
    @test typeof(collect(view(a, 2:4))) == Vector{Int64}
end

true
