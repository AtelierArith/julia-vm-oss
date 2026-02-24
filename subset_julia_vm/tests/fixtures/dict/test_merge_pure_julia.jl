using Test

@testset "Dict merge Pure Julia (Issue #2573)" begin
    # === Basic merge ===
    d1 = Dict("a" => 1, "b" => 2)
    d2 = Dict("b" => 3, "c" => 4)
    m = merge(d1, d2)
    @test get(m, "a", 0) == 1
    @test get(m, "b", 0) == 3   # d2's value wins
    @test get(m, "c", 0) == 4
    @test length(m) == 3

    # === Original dicts unchanged ===
    @test get(d1, "b", 0) == 2   # d1 not modified
    @test length(d1) == 2

    # === Merge with empty dict ===
    d3 = Dict("x" => 10)
    m2 = merge(d3, Dict{String,Int64}())
    @test get(m2, "x", 0) == 10
    @test length(m2) == 1

    # === Merge empty into non-empty ===
    m3 = merge(Dict{String,Int64}(), d3)
    @test get(m3, "x", 0) == 10

    # === Merge with int keys ===
    d4 = Dict(1 => "a", 2 => "b")
    d5 = Dict(2 => "B", 3 => "c")
    m4 = merge(d4, d5)
    @test get(m4, 1, "") == "a"
    @test get(m4, 2, "") == "B"
    @test get(m4, 3, "") == "c"

    # === copy uses merge ===
    d6 = Dict("p" => 100, "q" => 200)
    c = copy(d6)
    @test get(c, "p", 0) == 100
    @test get(c, "q", 0) == 200
    @test length(c) == 2

    # === mergewith uses merge (via copy) ===
    d7 = Dict("a" => 1, "b" => 2)
    d8 = Dict("b" => 3, "c" => 4)
    mw = mergewith(+, d7, d8)
    @test get(mw, "a", 0) == 1
    @test get(mw, "b", 0) == 5   # 2 + 3
    @test get(mw, "c", 0) == 4
end

true
