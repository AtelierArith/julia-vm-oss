# Regression test for all dot operators
# Validates that existing VM-level broadcast operations continue working
# correctly during and after Pure Julia migration.
# Related: Issue #2550 (broadcast regression test suite)

using Test

@testset "Dot arithmetic operators" begin
    a = [1.0, 2.0, 3.0]
    b = [4.0, 5.0, 6.0]

    # .+ (addition)
    @test a .+ b == [5.0, 7.0, 9.0]

    # .- (subtraction)
    @test b .- a == [3.0, 3.0, 3.0]

    # .* (multiplication)
    @test a .* b == [4.0, 10.0, 18.0]

    # ./ (division)
    @test b ./ a == [4.0, 2.5, 2.0]

    # .^ (power)
    @test [2.0, 3.0] .^ [2.0, 3.0] == [4.0, 27.0]
end

@testset "Dot comparison operators" begin
    a = [1.0, 5.0, 3.0]
    b = [2.0, 4.0, 3.0]

    # .> (greater than)
    @test (a .> b) == [false, true, false]

    # .< (less than)
    @test (a .< b) == [true, false, false]

    # .>= (greater or equal)
    @test (a .>= b) == [false, true, true]

    # .<= (less or equal)
    @test (a .<= b) == [true, false, true]

    # .== (equality)
    @test (a .== b) == [false, false, true]

    # .!= (not equal)
    @test (a .!= b) == [true, true, false]
end

@testset "Scalar-Array dot operations" begin
    a = [1.0, 2.0, 3.0]

    # Scalar on left
    @test 10.0 .+ a == [11.0, 12.0, 13.0]
    @test 10.0 .- a == [9.0, 8.0, 7.0]
    @test 2.0 .* a == [2.0, 4.0, 6.0]
    @test 6.0 ./ a == [6.0, 3.0, 2.0]

    # Scalar on right
    @test a .+ 10.0 == [11.0, 12.0, 13.0]
    @test a .- 1.0 == [0.0, 1.0, 2.0]
    @test a .* 3.0 == [3.0, 6.0, 9.0]
    @test a ./ 2.0 == [0.5, 1.0, 1.5]
end

@testset "Int64 dot operations preserve type" begin
    a = [1, 2, 3]
    b = [4, 5, 6]

    # Int64 .+ Int64 -> Int64
    c = a .+ b
    @test c == [5, 7, 9]
    @test typeof(c[1]) == Int64

    # Int64 .- Int64 -> Int64
    d = b .- a
    @test d == [3, 3, 3]
    @test typeof(d[1]) == Int64

    # Int64 .* Int64 -> Int64
    e = a .* b
    @test e == [4, 10, 18]
    @test typeof(e[1]) == Int64

    # Int64 ./ Int64 -> Float64 (Julia semantics)
    f = [4, 10, 18] ./ [4, 5, 6]
    @test f == [1.0, 2.0, 3.0]
    @test typeof(f[1]) == Float64
end

true
