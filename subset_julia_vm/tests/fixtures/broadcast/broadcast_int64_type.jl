# Broadcast type preservation test
# Verifies that Int64 .* Int64 returns Int64 (not Float64)

using Test

@testset "Broadcast Int64 type preservation" begin
    # Test 1: Vector .* Vector
    a = [1, 2, 3]
    b = [4, 5, 6]
    c = a .* b
    @test c == [4, 10, 18]
    @test c[1] == 4
    @test typeof(c[1]) == Int64

    # Test 2: Vector .+ Vector
    d = [1, 2, 3] .+ [10, 20, 30]
    @test d == [11, 22, 33]
    @test typeof(d[1]) == Int64

    # Test 3: Vector .- Vector
    e = [10, 20, 30] .- [1, 2, 3]
    @test e == [9, 18, 27]
    @test typeof(e[1]) == Int64

    # Test 4: Vector .* Scalar (Int64)
    f = [1, 2, 3] .* 2
    @test f == [2, 4, 6]
    @test typeof(f[1]) == Int64

    # Test 5: Scalar .* Vector
    g = 3 .* [1, 2, 3]
    @test g == [3, 6, 9]
    @test typeof(g[1]) == Int64

    # Test 6: Range .* Range (1D)
    h = collect(1:3) .* collect(4:6)
    @test h == [4, 10, 18]
    @test typeof(h[1]) == Int64

    # Test 7: Division should return Float64 (Julia semantics)
    i = [4, 10, 18] ./ [4, 5, 6]
    @test i == [1.0, 2.0, 3.0]
    @test typeof(i[1]) == Float64
end

true
