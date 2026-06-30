using Test

arrow_kw_c_4294 = (x; y=1) -> x + y
arrow_kw_4294 = (y=2,)
arrow_kw_forward_4294 = (x; kwargs...) -> arrow_kw_c_4294(x; kwargs...)
arrow_kw_all_forward_4294 = (args...; kwargs...) -> arrow_kw_c_4294(args...; kwargs...)
arrow_kw_zero_4294 = (; y=4) -> y + 1

@testset "keyword-parameter arrow functions (Issue #4294)" begin
    @test arrow_kw_c_4294(1; arrow_kw_4294...) == 3
    @test arrow_kw_c_4294(1) == 2
    @test arrow_kw_c_4294(1; y=5) == 6

    @test arrow_kw_forward_4294(1; arrow_kw_4294...) == 3

    @test arrow_kw_all_forward_4294(1; y=7) == 8

    @test arrow_kw_zero_4294(; y=5) == 6

    @test ((x; y=10) -> x * y)(3; y=4) == 12
end

true
