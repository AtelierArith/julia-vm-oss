using Test

@testset "Dict Pure Julia read-only operations (Issue #2572)" begin
    # === haskey ===
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    @test haskey(d, "a") == true
    @test haskey(d, "z") == false

    # === get(d, key, default) - 3-arg form ===
    @test get(d, "a", 0) == 1
    @test get(d, "b", 0) == 2
    @test get(d, "z", 99) == 99

    # get with int keys
    d2 = Dict(1 => "x", 2 => "y")
    @test get(d2, 1, "none") == "x"
    @test get(d2, 3, "none") == "none"

    # get with empty dict
    d3 = Dict{String, Int64}()
    @test get(d3, "any", 42) == 42

    # === getkey ===
    @test getkey(d, "a", "missing") == "a"
    @test getkey(d, "z", "missing") == "missing"
    @test getkey(d2, 1, -1) == 1
    @test getkey(d2, 99, -1) == -1
    @test getkey(d3, "any", "default") == "default"

    # === Verify haskey/get work after multiple calls (no corruption) ===
    for i in 1:5
        @test haskey(d, "a") == true
        @test get(d, "a", 0) == 1
    end
end

true
