# Issue #5137: `map(f, v)` over a SubArray now materializes the view first
# (the runtime `collect` iterator path does not recognize the SubArray struct,
# but `collect(::SubArray)` does), so mapping over a 1-D/2-D/dimension-dropping
# view matches upstream.

using Test

@testset "map over SubArray (Issue #5137)" begin
    # 1-D range view
    @test map(x -> x * 2, view([1, 2, 3, 4], 2:3)) == [4, 6]

    # 2-D range view preserves the matrix shape
    A = [1 2; 3 4]
    @test map(x -> x + 10, view(A, 1:2, 1:2)) == [11 12; 13 14]

    # dimension-dropping view (a row / column) stays 1-D
    B = [1 2 3; 4 5 6]
    @test map(x -> x * x, view(B, 1, :)) == [1, 4, 9]
    @test map(x -> x - 1, view(B, :, 2)) == [1, 4]

    # the parent is untouched by the (copying) map
    @test A == [1 2; 3 4]
    @test B == [1 2 3; 4 5 6]
end

true
