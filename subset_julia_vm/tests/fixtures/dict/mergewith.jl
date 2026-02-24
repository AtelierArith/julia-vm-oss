# Test mergewith! and mergewith for Dict (Issue #1813)
# Merge dictionaries with a custom combine function for duplicate keys
# Note: Uses named functions instead of bare operators (+, *) as first-class
# values are not yet supported in the lowering stage.
# Note: Uses get(d, key, default) instead of d[key] to work around
# type inference limitation when indexing Any-typed return values.

using Test

add(a, b) = a + b
mul(a, b) = a * b

@testset "mergewith! and mergewith" begin
    # mergewith! with add combines values for shared keys
    d1 = Dict("a" => 1, "b" => 2)
    d2 = Dict("a" => 3, "c" => 4)
    result = mergewith!(add, d1, d2)
    @test get(result, "a", 0) == 4   # 1 + 3
    @test get(result, "b", 0) == 2   # unchanged
    @test get(result, "c", 0) == 4   # from d2
    @test length(result) == 3

    # mergewith! with mul (multiply)
    d3 = Dict("x" => 2, "y" => 3)
    d4 = Dict("x" => 5, "z" => 7)
    result2 = mergewith!(mul, d3, d4)
    @test get(result2, "x", 0) == 10  # 2 * 5
    @test get(result2, "y", 0) == 3   # unchanged
    @test get(result2, "z", 0) == 7   # from d4

    # mergewith (non-mutating) creates a new dict
    d5 = Dict("a" => 10, "b" => 20)
    d6 = Dict("a" => 5, "c" => 30)
    d7 = mergewith(add, d5, d6)
    @test get(d7, "a", 0) == 15  # 10 + 5
    @test get(d7, "b", 0) == 20  # from d5
    @test get(d7, "c", 0) == 30  # from d6
    # d5 should be unchanged (non-mutating)
    @test get(d5, "a", 0) == 10

    # mergewith! with empty second dict (no-op)
    d8 = Dict("a" => 1)
    result3 = mergewith!(add, d8, Dict{String,Int64}())
    @test get(result3, "a", 0) == 1
    @test length(result3) == 1

    # mergewith! with empty first dict (copies from second)
    d9 = Dict{String,Int64}()
    d10 = Dict("x" => 42)
    result4 = mergewith!(add, d9, d10)
    @test get(result4, "x", 0) == 42
    @test length(result4) == 1

    # mergewith with min function
    d11 = Dict("a" => 10, "b" => 5)
    d12 = Dict("a" => 3, "b" => 20)
    d13 = mergewith(min, d11, d12)
    @test get(d13, "a", 0) == 3   # min(10, 3)
    @test get(d13, "b", 0) == 5   # min(5, 20)
end

true
