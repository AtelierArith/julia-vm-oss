# Multi-dimensional begin/end indexing (Issue #2349)
# Tests dimension-aware begin/end keyword resolution

using Test

@testset "Multi-dimensional begin/end indexing (Issue #2349)" begin
    # 2x2 matrix
    m = [1 2; 3 4]

    # Basic corner access
    @test m[begin, begin] == 1  # top-left
    @test m[begin, end] == 2    # top-right
    @test m[end, begin] == 3    # bottom-left
    @test m[end, end] == 4      # bottom-right

    # begin/end with arithmetic
    @test m[begin, begin+1] == 2
    @test m[begin+1, begin] == 3
    @test m[end-1+1, end-1+1] == 4

    # 3x3 matrix
    m3 = [1 2 3; 4 5 6; 7 8 9]
    @test m3[begin, begin] == 1
    @test m3[begin, end] == 3
    @test m3[end, begin] == 7
    @test m3[end, end] == 9
    @test m3[begin+1, begin+1] == 5  # center element

    # 2x3 matrix (non-square)
    m23 = [1 2 3; 4 5 6]  # 2 rows, 3 cols
    @test m23[begin, begin] == 1
    @test m23[begin, end] == 3    # end in dim 2 is 3
    @test m23[end, begin] == 4
    @test m23[end, end] == 6

    # 3x2 matrix
    m32 = [1 2; 3 4; 5 6]  # 3 rows, 2 cols
    @test m32[begin, begin] == 1
    @test m32[begin, end] == 2    # end in dim 2 is 2
    @test m32[end, begin] == 5    # end in dim 1 is 3
    @test m32[end, end] == 6
end

@testset "1D array begin/end (regression)" begin
    # Ensure 1D arrays still work correctly
    v = [10, 20, 30, 40, 50]
    @test v[begin] == 10
    @test v[end] == 50
    @test v[begin+1] == 20
    @test v[end-1] == 40
    @test v[begin:end] == [10, 20, 30, 40, 50]
end

true
