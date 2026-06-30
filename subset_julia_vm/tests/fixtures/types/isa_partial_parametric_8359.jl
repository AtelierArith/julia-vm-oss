using Test

struct IsaPartialParamIssue8359{T}
    x::T
end

@testset "isa with partial parametric type value (Issue #8359)" begin
    value = IsaPartialParamIssue8359{Float64}(1.0)

    @test value isa IsaPartialParamIssue8359{<:Real}
    @test !(value isa IsaPartialParamIssue8359{<:Integer})

    @test IsaPartialParamIssue8359{Float64} <: IsaPartialParamIssue8359{<:Real}
    @test !(IsaPartialParamIssue8359{Float64} <: IsaPartialParamIssue8359{<:Integer})

    @test string(IsaPartialParamIssue8359{<:Real}) ==
          "IsaPartialParamIssue8359{<:Real}"
end

true
