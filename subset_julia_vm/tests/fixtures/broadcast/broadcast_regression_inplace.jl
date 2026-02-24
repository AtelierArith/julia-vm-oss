# Comprehensive regression test for in-place broadcasting
# Tests broadcast!() and .= assignment patterns.
# Related: Issue #2550 (broadcast regression test suite)

using Test

add(x, y) = x + y
mul(x, y) = x * y

@testset "broadcast! basic operations" begin
    # broadcast!(f, dest, A, B) with addition
    dest = zeros(3)
    broadcast!(add, dest, [1.0, 2.0, 3.0], [10.0, 20.0, 30.0])
    @test dest == [11.0, 22.0, 33.0]

    # broadcast!(f, dest, A, B) with multiplication
    dest2 = zeros(3)
    broadcast!(mul, dest2, [2.0, 3.0, 4.0], [5.0, 6.0, 7.0])
    @test dest2 == [10.0, 18.0, 28.0]
end

@testset "broadcast! with scalars" begin
    dest = zeros(3)
    broadcast!(add, dest, [1.0, 2.0, 3.0], 10.0)
    @test dest == [11.0, 12.0, 13.0]

    dest2 = zeros(3)
    broadcast!(mul, dest2, 2.0, [1.0, 2.0, 3.0])
    @test dest2 == [2.0, 4.0, 6.0]
end

@testset "broadcast! returns destination" begin
    dest = zeros(3)
    result = broadcast!(add, dest, [1.0, 2.0, 3.0], [4.0, 5.0, 6.0])
    @test result[1] == 5.0
    @test result[2] == 7.0
    @test result[3] == 9.0
end

@testset "broadcast! preserves source arrays" begin
    a = [1.0, 2.0, 3.0]
    b = [10.0, 20.0, 30.0]
    dest = zeros(3)
    broadcast!(add, dest, a, b)

    # Source arrays should be unchanged
    @test a == [1.0, 2.0, 3.0]
    @test b == [10.0, 20.0, 30.0]
end

@testset ".= broadcast assignment" begin
    # .= syntax for in-place broadcast
    dest = [0.0, 0.0, 0.0]
    dest .= [1.0, 2.0, 3.0] .+ [4.0, 5.0, 6.0]
    @test dest == [5.0, 7.0, 9.0]

    # Alias should observe in-place update (no rebinding)
    alias = dest
    dest .= [10.0, 20.0, 30.0]
    @test alias == [10.0, 20.0, 30.0]
end

true
