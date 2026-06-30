using Test

# Issue #5603: an ambiguous method-table dispatch is an unreachable call in
# inference. Upstream Julia reports `Union{}` for the ambiguous signature, not
# `Any`, while non-ambiguous signatures still infer the selected method return.

ambiguous_ret_5603(x::Integer, y::Real) = 1
ambiguous_ret_5603(x::Real, y::Integer) = "ambiguous"

@testset "ambiguous method-table return inference (#5603)" begin
    @test Base.infer_return_type(ambiguous_ret_5603, Tuple{Int64,Int64}) === Union{}
    @test isempty(Base.return_types(ambiguous_ret_5603, Tuple{Int64,Int64}))

    @test Base.infer_return_type(ambiguous_ret_5603, Tuple{Int64,Float64}) === Int64
    @test Base.infer_return_type(ambiguous_ret_5603, Tuple{Float64,Int64}) === String
end

true
