# Core.apply_type with a prefix of parameters returns a partial UnionAll.
# Issue #10192.

using Test

struct PairApplyType10192{A,B}
    a::A
    b::B
end

struct TriApplyType10192{A,B,C}
    a::A
    b::B
    c::C
end

partial_prefix_type_param_11232(::Type{PairApplyType10192{T}}) where {T} = T

@testset "Core.apply_type partial UnionAll (Issue #10192)" begin
    w = Core.apply_type(PairApplyType10192, Int64)
    @test typeof(w) == UnionAll
    @test string(w) == "PairApplyType10192{Int64}"
    @test Core.apply_type(w, Float64) === PairApplyType10192{Int64, Float64}

    wlit = PairApplyType10192{Int64}
    @test typeof(wlit) == UnionAll
    @test string(wlit) == "PairApplyType10192{Int64}"
    @test wlit{Float64} === PairApplyType10192{Int64, Float64}

    tri = Core.apply_type(TriApplyType10192, Int64)
    @test typeof(tri) == UnionAll
    @test string(tri) == "TriApplyType10192{Int64}"

    tri2 = Core.apply_type(tri, Float64)
    @test typeof(tri2) == UnionAll
    @test string(tri2) == "TriApplyType10192{Int64, Float64}"
    @test Core.apply_type(tri2, String) === TriApplyType10192{Int64, Float64, String}

    # A partial type object keeps its already-applied prefix available to a
    # Type{W{T}} method even though W has additional trailing parameters.
    @test partial_prefix_type_param_11232(PairApplyType10192{Int64}) === Int64

end

true
