using Test

import Base: length, ndims

length(x::Vector{Int64}) = 99
ndims(x::Vector{Int64}) = 7

length_any_4276(x::Any) = length(x)
ndims_any_4276(x::Any) = ndims(x)

@testset "length and ndims use user methods before fallback (Issue #4276)" begin
    @test length([1, 2, 3]) == 99
    @test length_any_4276([1, 2, 3]) == 99
    @test length([1.0, 2.0]) == 2
    @test length_any_4276([1.0, 2.0]) == 2

    @test ndims([1, 2, 3]) == 7
    @test ndims_any_4276([1, 2, 3]) == 7
    @test ndims([1.0, 2.0]) == 1
    @test ndims_any_4276([1.0, 2.0]) == 1
end

true
