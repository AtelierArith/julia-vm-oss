using Test

map_bang_add5_4019(a, b, c, d, e) = a + b + c + d + e

function map_bang_runtime_4019(dest::Any, a::Any, b::Any, c::Any, d::Any, e::Any)
    return map!(map_bang_add5_4019, dest, a, b, c, d, e)
end

@testset "map! five-source Array dispatch (Issue #4019)" begin
    dest = [0, 0]
    result = map!(
        map_bang_add5_4019,
        dest,
        [1, 2],
        [10, 20],
        [100, 200],
        [1000, 2000],
        [10000, 20000],
    )
    @test result === dest
    @test dest == [11111, 22222]
    @test typeof(dest) === Vector{Int64}

    short_dest = [0, 0, 0]
    runtime = map_bang_runtime_4019(
        short_dest,
        [1, 2, 3],
        [10, 20],
        [100, 200, 300],
        [1000, 2000, 3000],
        [10000, 20000, 30000],
    )
    @test runtime === short_dest
    @test short_dest == [11111, 22222, 0]
    @test typeof(short_dest) === Vector{Int64}
end

true
