using Test

import Base: eltype

eltype(x::Vector{Int64}) = String

eltype_any_4276(x::Any) = eltype(x)

@testset "eltype uses user method before fallback (Issue #4276)" begin
    @test eltype([1, 2, 3]) === String
    @test eltype_any_4276([1, 2, 3]) === String

    @test eltype([1.0, 2.0]) === Float64
    @test eltype_any_4276([1.0, 2.0]) === Float64
end

true
