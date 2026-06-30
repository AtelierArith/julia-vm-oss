using Test

map_bang_add6_4019(a, b, c, d, e, f) = a + b + c + d + e + f
map_bang_add9_4019(a, b, c, d, e, f, g, h, i) = a + b + c + d + e + f + g + h + i

function map_bang_vararg_runtime_4019(dest::Any, sources::Any)
    return map!(map_bang_add6_4019, dest, sources...)
end

@testset "map! vararg Array sources (Issue #4019)" begin
    dest = [0, 0]
    result = map!(
        map_bang_add6_4019,
        dest,
        [1, 2],
        [10, 20],
        [100, 200],
        [1000, 2000],
        [10000, 20000],
        [100000, 200000],
    )
    @test result === dest
    @test dest == [111111, 222222]
    @test typeof(dest) === Vector{Int64}

    splat_dest = [0, 0, 0]
    sources = ([1, 2, 3], [10, 20], [100, 200, 300], [1000, 2000, 3000], [10000, 20000, 30000], [100000, 200000, 300000])
    splat_result = map_bang_vararg_runtime_4019(splat_dest, sources)
    @test splat_result === splat_dest
    @test splat_dest == [111111, 222222, 0]

    fallback_dest = [0, 0]
    map!(
        map_bang_add9_4019,
        fallback_dest,
        [1, 2],
        [10, 20],
        [100, 200],
        [1000, 2000],
        [10000, 20000],
        [100000, 200000],
        [1000000, 2000000],
        [10000000, 20000000],
        [100000000, 200000000],
    )
    @test fallback_dest == [111111111, 222222222]
end

true
