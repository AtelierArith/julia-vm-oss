using Test

@testset "map over tuple preserves tuple shape and element types (Issue #4283)" begin
    mapped = map(identity, (1, "a"))
    @test mapped == (1, "a")
    @test typeof(mapped) == Tuple{Int64, String}

    types = map(typeof, (1, "a"))
    @test types == (Int64, String)
    @test typeof(types) == Tuple{DataType, DataType}

    @test map(identity, ()) == ()
    @test typeof(map(identity, (42,))) == Tuple{Int64}
    @test typeof(map(identity, (1, 2.0, "three"))) == Tuple{Int64, Float64, String}

    pairwise = map(+, (1, 2), (3, 4))
    @test pairwise == (4, 6)
    @test typeof(pairwise) == Tuple{Int64, Int64}

    mixed_pairwise = map((x, y) -> (x, y), (1, "a"), (2.0, 'b'))
    @test mixed_pairwise == ((1, 2.0), ("a", 'b'))
    @test typeof(mixed_pairwise) == Tuple{Tuple{Int64, Float64}, Tuple{String, Char}}

    @test map(+, (1, 2, 3), (10, 20)) == (11, 22)

    triple = map((x, y, z) -> (x, y, z), (1, "a"), (2.0, false), (true, 'b', 9))
    @test triple == ((1, 2.0, true), ("a", false, 'b'))
    @test typeof(triple) == Tuple{Tuple{Int64, Float64, Bool}, Tuple{String, Bool, Char}}

    triple_sum = map(+, (1, 2, 3), (10, 20), (100, 200, 300))
    @test triple_sum == (111, 222)
    @test typeof(triple_sum) == Tuple{Int64, Int64}

    quad_sum = map(+, (1, 2), (10, 20), (100, 200), (1000, 2000))
    @test quad_sum == (1111, 2222)
    @test typeof(quad_sum) == Tuple{Int64, Int64}

    quad_mixed = map((a, b, c, d) -> (a + d, string(b), c),
                     (1, 2), ("x", "y"), (true, false), (10, 20, 30))
    @test quad_mixed == ((11, "x", true), (22, "y", false))
    @test typeof(quad_mixed) == Tuple{Tuple{Int64, String, Bool}, Tuple{Int64, String, Bool}}
end

true
