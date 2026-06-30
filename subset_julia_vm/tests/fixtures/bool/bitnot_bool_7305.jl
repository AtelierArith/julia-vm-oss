# Test ~(::Bool) bitwise NOT on Bool
# Issue #7305: `~(::Bool)` threw MethodError; upstream Julia returns the negation
#   ~true === false, ~false === true  (upstream base/bool.jl: (~)(x::Bool) = !x)

using Test

@testset "~(::Bool) (Issue #7305)" begin
    @testset "scalar negation" begin
        @test ~true === false
        @test ~false === true
        @test typeof(~true) === Bool
        @test typeof(~false) === Bool
    end

    @testset "broadcast .~ over a Bool vector" begin
        v = .~[true, false]
        @test v == [false, true]
        @test eltype(v) === Bool
    end
end

true
