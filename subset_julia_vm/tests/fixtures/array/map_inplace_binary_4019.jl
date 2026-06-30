using Test

@testset "binary map! for arrays (Issue #4019)" begin
    dest = zeros(Int64, 4)
    a = [1, 2, 3, 4]
    b = [10, 20, 30, 40]
    result = map!((x, y) -> x + y, dest, a, b)
    @test result === dest
    @test dest == [11, 22, 33, 44]

    short_dest = zeros(Int64, 2)
    short_result = map!((x, y) -> x * y, short_dest, [2, 3, 4], [5, 6, 7])
    @test short_result === short_dest
    @test short_dest == [10, 18]

    long_dest = [0, 0, 0, 99]
    map!((x, y) -> x - y, long_dest, [8, 9], [3, 4, 5])
    @test long_dest == [5, 5, 0, 99]

    matrix_dest = zeros(Int64, 2, 2)
    map!((x, y) -> x + y, matrix_dest, [1 2; 3 4], [10 20; 30 40])
    @test matrix_dest == [11 22; 33 44]

    float_dest = zeros(Float64, 3)
    map!((x, y) -> x / y, float_dest, [2, 3, 4], [2, 2, 2])
    @test typeof(float_dest) === Vector{Float64}
    @test float_dest == [1.0, 1.5, 2.0]
end

true
