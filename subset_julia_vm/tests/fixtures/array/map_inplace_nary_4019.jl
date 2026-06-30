using Test

map_inplace_sum3_4019(x, y, z) = x + y + z
map_inplace_sum4_4019(w, x, y, z) = w + x + y + z
map_inplace_weighted3_4019(x, y, z) = x + 10y + 100z
map_inplace_weighted4_4019(w, x, y, z) = w + 10x + 100y + 1000z
map_inplace_div3_4019(x, y, z) = (x + y) / z

@testset "n-ary map! for arrays (Issue #4019)" begin
    dest = zeros(Int64, 4)
    a = [1, 2, 3, 4]
    b = [10, 20, 30, 40]
    c = [100, 200, 300, 400]
    result = map!(map_inplace_sum3_4019, dest, a, b, c)
    @test result === dest
    @test dest == [111, 222, 333, 444]

    short_dest = zeros(Int64, 2)
    short_result = map!(map_inplace_weighted3_4019, short_dest, [1, 2, 3], [4, 5, 6], [7, 8, 9])
    @test short_result === short_dest
    @test short_dest == [741, 852]

    long_dest = [0, 0, 0, 99]
    map!(map_inplace_sum3_4019, long_dest, [1, 2], [10, 20, 30], [100, 200, 300])
    @test long_dest == [111, 222, 0, 99]

    matrix_dest = zeros(Int64, 2, 2)
    map!(map_inplace_sum3_4019, matrix_dest, [1 2; 3 4], [10 20; 30 40], [100 200; 300 400])
    @test matrix_dest == [111 222; 333 444]

    float_dest = zeros(Float64, 3)
    map!(map_inplace_div3_4019, float_dest, [2, 3, 4], [2, 3, 4], [2, 2, 2])
    @test typeof(float_dest) === Vector{Float64}
    @test float_dest == [2.0, 3.0, 4.0]

    four_dest = zeros(Int64, 3)
    four_result = map!(map_inplace_sum4_4019, four_dest, [1, 2, 3], [10, 20, 30], [100, 200, 300], [1000, 2000, 3000])
    @test four_result === four_dest
    @test four_dest == [1111, 2222, 3333]

    four_short_dest = [0, 0, 0, 99]
    map!(map_inplace_weighted4_4019, four_short_dest, [1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11])
    @test four_short_dest == [9631, 10742, 0, 99]
end

true
