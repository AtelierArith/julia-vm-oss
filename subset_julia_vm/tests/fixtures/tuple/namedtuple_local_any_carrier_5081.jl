using Test

namedtuple_global_roundtrip_5081 = (a = 1, b = "x")
namedtuple_global_reassign_5081 = (left = 2, right = 3)

@testset "NamedTuple fallback locals use locals_any carrier" begin
    global namedtuple_global_roundtrip_5081
    global namedtuple_global_reassign_5081

    @test namedtuple_global_roundtrip_5081.a == 1
    @test namedtuple_global_roundtrip_5081.b == "x"
    @test namedtuple_global_reassign_5081.right == 3

    namedtuple_global_roundtrip_5081 = (a = 4, b = 5, c = 6)
    @test namedtuple_global_roundtrip_5081.c == 6
    @test length(namedtuple_global_roundtrip_5081) == 3

    namedtuple_global_reassign_5081 = 42
    @test namedtuple_global_reassign_5081 == 42
end

namedtuple_global_roundtrip_5081.c == 6 && namedtuple_global_reassign_5081 == 42
