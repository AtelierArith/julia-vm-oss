using Test

struct UserShowDisplay9364
    x::Int
end

Base.show(io::IO, f::UserShowDisplay9364) = print(io, "Foo<", f.x, ">")

struct ParamShow9456{T}
    x::T
end

Base.show(io::IO, w::ParamShow9456{Int64}) = print(io, "W-int(", w.x, ")")

@testset "user show reaches interpolation/show/repr (Issue #9364)" begin
    f = UserShowDisplay9364(7)
    @test string(f) == "Foo<7>"
    @test "$f" == "Foo<7>"
    @test sprint(show, f) == "Foo<7>"
    @test repr(f) == "Foo<7>"
end

@testset "concrete parametric show stays exact (Issue #9456)" begin
    @test sprint(show, ParamShow9456{Int64}(3)) == "W-int(3)"
    @test sprint(show, ParamShow9456{Float64}(1.5)) == "ParamShow9456{Float64}(1.5)"
    @test string(ParamShow9456{Float64}(1.5)) == "ParamShow9456{Float64}(1.5)"
end

true
