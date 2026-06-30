using Test

@testset "bare typevar where body collapses to bound (Issue #5570)" begin
    @test (T where T) === Any
    @test string(T where T) == "Any"
    @test (T where T<:Real) === Real
    @test string(T where T<:Real) == "Real"
    @test Int <: (T where T)
    @test !(String <: (T where T<:Real))
    @test typeof(T where T) === DataType
    @test typeof(T where T<:Real) === DataType
end

true
