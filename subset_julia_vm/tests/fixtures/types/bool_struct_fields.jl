# Test Bool struct field access
# This test ensures that Bool fields in structs can be accessed correctly
# (regression test for Issue #1612)

using Test

# Struct definitions must be at top level (outside @testset)
mutable struct BoolContainer
    flag::Bool
    count::Int64
end

mutable struct MultiBool
    a::Bool
    b::Bool
    c::Int64
end

struct ImmutableBool
    flag::Bool
    value::Int64
end

@testset "Bool struct field access" begin
    bc = BoolContainer(true, 42)
    @test bc.flag == true
    @test bc.count == 42

    bc.flag = false
    @test bc.flag == false

    # Bool can be used in numeric context (Bool <: Integer in Julia)
    @test bc.flag + 1 == 1
    @test true + bc.count == 43
end

@testset "Multiple Bool fields" begin
    mb = MultiBool(true, false, 10)
    @test mb.a == true
    @test mb.b == false
    @test mb.c == 10

    # Modify Bool fields
    mb.a = false
    mb.b = true
    @test mb.a == false
    @test mb.b == true
end

@testset "Bool field in immutable struct" begin
    ib = ImmutableBool(true, 100)
    @test ib.flag == true
    @test ib.value == 100
end

true
