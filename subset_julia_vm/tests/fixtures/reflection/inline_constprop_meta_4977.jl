# CodeInfo.inlining / Method.constprop / CodeInfo.constprop metadata reflection
# (Issues #4977, #4978, #4980, #4981)
using Test

plain_meta_4977(x) = x + 1
@inline inline_meta_4977(x) = x + 2
@noinline noinline_meta_4977(x) = x + 3
Base.@propagate_inbounds prop_meta_4977(x) = x + 4

Base.@constprop :aggressive function cp_aggr_4977(x)
    x + 5
end
Base.@constprop :none cp_none_4977(x) = x + 6

@testset "inline/constprop metadata reflection" begin
    # CodeInfo.inlining (UInt8): 0 default, 1 inline, 2 noinline (Issue #4977)
    @test Base.code_lowered(plain_meta_4977, Tuple{Int64})[1].inlining == 0
    @test Base.code_lowered(inline_meta_4977, Tuple{Int64})[1].inlining == 1
    @test Base.code_lowered(noinline_meta_4977, Tuple{Int64})[1].inlining == 2
    @test Base.code_lowered(plain_meta_4977, Tuple{Int64})[1].inlining isa UInt8

    # @propagate_inbounds feeds CodeInfo.inlining == 1 (Issue #4980)
    @test Base.code_lowered(prop_meta_4977, Tuple{Int64})[1].inlining == 1
    @test Base.code_typed(prop_meta_4977, Tuple{Int64})[1][1].inlining == 1

    # Method.constprop (UInt8): 0 default, 1 aggressive, 2 none (Issue #4978)
    @test first(methods(plain_meta_4977)).constprop == 0
    @test first(methods(cp_aggr_4977)).constprop == 1
    @test first(methods(cp_none_4977)).constprop == 2
    @test first(methods(plain_meta_4977)).constprop isa UInt8

    # CodeInfo.constprop (UInt8) on lowered and typed (Issue #4981)
    @test Base.code_lowered(plain_meta_4977, Tuple{Int64})[1].constprop == 0
    @test Base.code_lowered(cp_aggr_4977, Tuple{Int64})[1].constprop == 1
    @test Base.code_lowered(cp_none_4977, Tuple{Int64})[1].constprop == 2
    @test Base.code_typed(cp_aggr_4977, Tuple{Int64})[1][1].constprop == 1
end

true
