# Function-form isdefined public surface (Issues #5002, #4958)
# isdefined(::Module, ::Symbol) checks module bindings.
# isdefined(x, ::Symbol) checks object field definitions.
using Test

struct ProbeStruct
    x::Int64
    y::String
end

module OrdinaryIsDefined11410 end
baremodule BareIsDefined11410 end
baremodule ExplicitBaseIsDefined11410
using Base
end
module FunctionOwnerIsDefined11410
unrelated_function_11410() = true
end
module ParentIsDefined11410
baremodule NestedBareIsDefined11410 end
module NestedOrdinaryIsDefined11410 end
end

@testset "isdefined(::Module, ::Symbol) checks Base bindings (#4958)" begin
    @test isdefined(Base, :sum)
    @test isdefined(Base, :reduce)
    @test isdefined(Base, :return_types)
    @test isdefined(Base, :code_lowered)
    @test isdefined(Base, :widen)
    @test isdefined(Base, :NonExistentMethod) == false
end

@testset "isdefined preserves implicit Base without leaking sibling functions (#11410)" begin
    @test isdefined(OrdinaryIsDefined11410, :map)
    @test isdefined(OrdinaryIsDefined11410, :println)
    @test isdefined(OrdinaryIsDefined11410, :zeros)
    @test !isdefined(BareIsDefined11410, :map)
    @test !isdefined(BareIsDefined11410, :println)
    @test !isdefined(BareIsDefined11410, :zeros)
    @test !isdefined(BareIsDefined11410, :length)
    @test isdefined(BareIsDefined11410, :getfield)
    @test isdefined(BareIsDefined11410, :typeof)
    @test isdefined(ExplicitBaseIsDefined11410, :map)
    @test isdefined(ExplicitBaseIsDefined11410, :println)
    @test isdefined(ParentIsDefined11410.NestedOrdinaryIsDefined11410, :map)
    @test !isdefined(ParentIsDefined11410.NestedBareIsDefined11410, :map)
    @test !isdefined(OrdinaryIsDefined11410, :unrelated_function_11410)
end

@testset "isdefined respects Core/Base binding authority (#11410)" begin
    @test !isdefined(BareIsDefined11410, :Vector)
    @test isdefined(BareIsDefined11410, :Int)
    @test !isdefined(BareIsDefined11410, :sizeof)
    @test isdefined(BareIsDefined11410, :applicable)
    @test isdefined(BareIsDefined11410, :fieldtype)
    @test isdefined(BareIsDefined11410, :throw)
    @test !isdefined(BareIsDefined11410, :ifelse)
    @test !isdefined(BareIsDefined11410, :memoryref)
    @test !isdefined(BareIsDefined11410, :print)
    @test !isdefined(BareIsDefined11410, :iterate)
    @test !isdefined(BareIsDefined11410, :eval)

    @test !isdefined(Core, :Vector)
    @test isdefined(Core, :Int)
    @test isdefined(Core, :sizeof)
    @test isdefined(Core, :applicable)
    @test isdefined(Core, :fieldtype)
    @test isdefined(Core, :throw)
    @test isdefined(Core, :ifelse)
    @test isdefined(Core, :memoryref)
    @test isdefined(Core, :print)
    @test isdefined(Core, :iterate)
    @test isdefined(Core, :eval)

    @test !isdefined(Base, :_ctpop_int)
    @test isdefined(Base, :_string)
end

@testset "isdefined uses the complete modeled Base export set (#11410)" begin
    for name in (:ComplexF32, :SubString, :StepRange, :RegexMatch)
        @test !isdefined(Core, name)
        @test !isdefined(BareIsDefined11410, name)
        @test isdefined(ExplicitBaseIsDefined11410, name)
        @test isdefined(OrdinaryIsDefined11410, name)
        @test isdefined(Main, name)
    end

    for name in (
        :Pipe,
        :redirect_stdout,
        :redirect_stderr,
        :seek,
        :position,
        :skip,
        :flush,
        :names,
        :memoryref,
        Symbol(">:"),
    )
        @test !isdefined(BareIsDefined11410, name)
        @test isdefined(ExplicitBaseIsDefined11410, name)
        @test isdefined(OrdinaryIsDefined11410, name)
        @test isdefined(Main, name)
    end
end

@testset "ordinary modules retain implicit eval/include (#11410)" begin
    @test isdefined(Core, :eval)
    @test isdefined(Core, :include)
    @test isdefined(Base, :eval)
    @test isdefined(Base, :include)
    @test isdefined(Main, :eval)
    @test isdefined(Main, :include)
    @test isdefined(OrdinaryIsDefined11410, :eval)
    @test isdefined(OrdinaryIsDefined11410, :include)
    @test isdefined(ParentIsDefined11410.NestedOrdinaryIsDefined11410, :eval)
    @test isdefined(ParentIsDefined11410.NestedOrdinaryIsDefined11410, :include)
    @test !isdefined(BareIsDefined11410, :eval)
    @test !isdefined(BareIsDefined11410, :include)
    @test !isdefined(ExplicitBaseIsDefined11410, :eval)
    @test !isdefined(ExplicitBaseIsDefined11410, :include)
    @test !isdefined(ParentIsDefined11410.NestedBareIsDefined11410, :eval)
    @test !isdefined(ParentIsDefined11410.NestedBareIsDefined11410, :include)
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
