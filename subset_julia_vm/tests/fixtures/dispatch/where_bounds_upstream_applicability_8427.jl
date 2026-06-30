using Test

@testset "where bounds match upstream applicability (Issue #8427)" begin
    lower_value(x::T) where {T>:Int} = 1
    @test lower_value(1.0) == 1
    @test lower_value(1) == 1

    lower_type(x::T) where {T>:Int} = T
    @test_throws UndefVarError lower_type(1.0)

    cross_value(x::T, y::S) where {T<:Real,S<:T} = 1
    @test cross_value(1, 2.0) == 1
    @test cross_value(1, 1) == 1

    cross_type(x::T, y::S) where {T<:Real,S<:T} = (T, S)
    @test cross_type(1, 1) == (Int64, Int64)
    @test_throws UndefVarError cross_type(1, 2.0)
end

true
