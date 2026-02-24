# Dict indexing dispatch prevention tests (Issue #1820)
# Additional coverage for Dict indexing through multiple function layers,
# Symbol keys, and mixed operations to prevent dispatch regressions.

using Test

# Multi-layer function calls: Dict passed through inner/outer functions
function inner_get(d, k)
    return d[k]
end

function outer_get(d, k)
    return inner_get(d, k)
end

function inner_set(d, k, v)
    d[k] = v
    return d
end

function outer_set(d, k, v)
    return inner_set(d, k, v)
end

# Mixed Dict operations in a single function
function mixed_ops(d)
    d["new_key"] = 99
    val = d["new_key"]
    return val
end

# Multiple reads in one function
function multi_read(d)
    a = d["x"]
    b = d["y"]
    return a + b
end

@testset "Dict through multiple function layers" begin
    d = Dict("a" => 1, "b" => 2)
    @test outer_get(d, "a") == 1
    @test outer_get(d, "b") == 2

    d2 = Dict("x" => 10)
    result = outer_set(d2, "y", 20)
    @test get(result, "x", 0) == 10
    @test get(result, "y", 0) == 20
end

@testset "Dict with Symbol keys through functions" begin
    d = Dict(:a => 1, :b => 2, :c => 3)
    @test inner_get(d, :a) == 1
    @test inner_get(d, :b) == 2
    @test outer_get(d, :c) == 3
end

@testset "Mixed Dict operations in single function" begin
    d = Dict("a" => 1, "b" => 2)
    @test mixed_ops(d) == 99
end

@testset "Multiple Dict reads in one function" begin
    d = Dict("x" => 10, "y" => 20)
    @test multi_read(d) == 30
end

@testset "Integer-keyed Dict through multiple layers" begin
    d = Dict(1 => "one", 2 => "two", 3 => "three")
    @test outer_get(d, 1) == "one"
    @test outer_get(d, 2) == "two"
    @test outer_get(d, 3) == "three"
end

true
