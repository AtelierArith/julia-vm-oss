# Function-form isdefined public surface (Issues #5002, #4958)
# isdefined(::Module, ::Symbol) checks module bindings.
# isdefined(x, ::Symbol) checks object field definitions.
using Test

struct ProbeStruct
    x::Int64
    y::String
end

@testset "isdefined(::Module, ::Symbol) checks Base bindings (#4958)" begin
    @test isdefined(Base, :sum)
    @test isdefined(Base, :reduce)
    @test isdefined(Base, :return_types)
    @test isdefined(Base, :code_lowered)
    @test isdefined(Base, :widen)
    @test isdefined(Base, :NonExistentMethod) == false
end

@testset "isdefined(::Module, ::Symbol) checks Main bindings" begin
    @test isdefined(Main, :ProbeStruct)
    @test isdefined(Main, :sum)
    @test isdefined(Main, :definitely_not_bound_anywhere_xyz) == false
end

@testset "isdefined(x, ::Symbol) checks object fields" begin
    p = ProbeStruct(1, "a")
    @test isdefined(p, :x)
    @test isdefined(p, :y)
    @test isdefined(p, :z) == false
end

true
