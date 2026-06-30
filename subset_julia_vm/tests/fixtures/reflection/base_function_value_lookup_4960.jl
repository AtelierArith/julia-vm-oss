# Base.<fn> resolves as a callable function value (Issues #4960-#4966)
# Upstream Julia exposes these Base helpers as function values that can be
# bound to a variable and applied.
using Test

@testset "Base reflection function values" begin
    rt = Base.return_types
    @test rt isa Function
    @test rt(+, Tuple{Int64,Int64}) == [Int64]

    ct = Base.code_typed
    @test ct isa Function

    cl = Base.code_lowered
    @test cl isa Function

    irt = Base.infer_return_type
    @test irt isa Function
end

@testset "Base conversion/promotion function values" begin
    w = Base.widen
    @test w isa Function
    @test w(Int32) == Int64

    pt = Base.promote_type
    @test pt isa Function
    @test pt(Int64, Float64) == Float64

    pr = Base.promote_rule
    @test pr isa Function

    cv = Base.convert
    @test cv isa Function
    @test cv(Float64, 3) == 3.0

    ot = Base.oftype
    @test ot isa Function
    @test ot(1.0, 3) == 3.0
end

true
