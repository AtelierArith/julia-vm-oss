# Test Pure Julia keys/values/pairs for Dict (Issue #2669, #2573)
# These functions now dispatch to _dict_keys/_dict_values/_dict_pairs intrinsics
# via Pure Julia wrappers in dict.jl.

using Test

@testset "Dict keys Pure Julia" begin
    # Basic keys
    d = Dict("a" => 1, "b" => 2, "c" => 3)
    ks = keys(d)
    @test length(ks) == 3
    # Keys should contain all expected keys (order may vary)
    @test "a" in ks
    @test "b" in ks
    @test "c" in ks

    # Empty dict keys
    d2 = Dict()
    ks2 = keys(d2)
    @test length(ks2) == 0

    # Integer keys
    d3 = Dict(1 => "x", 2 => "y")
    ks3 = keys(d3)
    @test length(ks3) == 2
    @test 1 in ks3
    @test 2 in ks3
end

@testset "Dict values Pure Julia" begin
    # Basic values
    d = Dict("a" => 10, "b" => 20, "c" => 30)
    vs = values(d)
    @test length(vs) == 3
    # Values should contain all expected values
    @test 10 in vs
    @test 20 in vs
    @test 30 in vs

    # Empty dict values
    d2 = Dict()
    vs2 = values(d2)
    @test length(vs2) == 0

    # Integer-keyed dict values
    d3 = Dict(1 => "hello", 2 => "world")
    vs3 = values(d3)
    @test length(vs3) == 2
    @test "hello" in vs3
    @test "world" in vs3
end

@testset "Dict pairs Pure Julia" begin
    # Basic pairs
    d = Dict("x" => 100, "y" => 200)
    ps = pairs(d)
    @test length(ps) == 2

    # Each pair should be a 2-element tuple (key, value)
    found_x = false
    found_y = false
    for p in ps
        if p[1] == "x" && p[2] == 100
            found_x = true
        end
        if p[1] == "y" && p[2] == 200
            found_y = true
        end
    end
    @test found_x
    @test found_y

    # Empty dict pairs
    d2 = Dict()
    ps2 = pairs(d2)
    @test length(ps2) == 0
end

@testset "Dict keys/values used in iteration" begin
    # Use keys to iterate and build a new dict
    d = Dict("a" => 1, "b" => 2)
    result = Dict()
    for k in keys(d)
        result[k] = d[k] * 10
    end
    @test get(result, "a", 0) == 10
    @test get(result, "b", 0) == 20

    # Use values to compute sum
    d2 = Dict("x" => 3, "y" => 7)
    total = 0
    for v in values(d2)
        total = total + v
    end
    @test total == 10
end

true
