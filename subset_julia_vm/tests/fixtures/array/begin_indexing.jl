# Tests for begin keyword in indexing context (Issue #2310)
# a[begin] should resolve to a[firstindex(a)]

using Test

a = [10, 20, 30, 40, 50]

@testset "begin keyword indexing (Issue #2310)" begin
    # Simple begin indexing
    @test a[begin] == 10

    # begin with arithmetic
    @test a[begin + 1] == 20
    @test a[begin + 2] == 30

    # begin:end range
    @test a[begin:end] == [10, 20, 30, 40, 50]

    # begin+1:end-1 range
    @test a[begin+1:end-1] == [20, 30, 40]

    # Combining begin and end
    @test a[begin] == a[1]
    @test a[end] == a[5]
end

# Additional tests for begin/end symmetry (Issue #2325)
# Note: 2D array begin/end indexing with per-dimension resolution is not yet
# supported. The current implementation uses lastindex(array) without dimension
# awareness. See Issue #2349 for tracking this enhancement.

@testset "String begin indexing" begin
    s = "hello"
    @test s[begin] == 'h'
    @test s[end] == 'o'
    @test s[begin+1] == 'e'
    @test s[end-1] == 'l'
end

@testset "Nested array begin indexing" begin
    nested = [[1, 2], [3, 4]]
    @test nested[begin][begin] == 1
    @test nested[begin][end] == 2
    @test nested[end][begin] == 3
    @test nested[end][end] == 4
end

@testset "begin in comprehension indexing" begin
    arr = [10, 20, 30]
    result = [arr[begin+i] for i in 0:2]
    @test result == [10, 20, 30]
end

true
