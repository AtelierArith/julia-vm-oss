using Test

kwargs_pairs_forward_inner_4292(; x=1) = x + 1
kwargs_pairs_forward_outer_4292(; kwargs...) = kwargs_pairs_forward_inner_4292(; kwargs...)

kwargs_pairs_forward_inner_pos_4292(a; y=1) = a + y
kwargs_pairs_forward_outer_pos_4292(a; kwargs...) =
    kwargs_pairs_forward_inner_pos_4292(a; kwargs...)

kwargs_positional_splat_4292(x, y) = x + y
kwargs_closure_splat_4292 = (x, y) -> x * y
kwargs_known_pos_kw_splat_4292(x, y; scale=1) = (x + y) * scale
kwargs_function_value_4292 = kwargs_known_pos_kw_splat_4292

function kwargs_nested_closure_kw_splat_4292(offset)
    function inner(x, y; scale=1)
        (x + y + offset) * scale
    end
    args = (1, 2)
    kw = (scale=2,)
    inner(args...; kw...)
end

struct KwargsCallable4292 end
(::KwargsCallable4292)(x, y; scale=1) = (x + y) * scale

module KwargsSplatModule4292
export kw_module_target_4292, positional_module_target_4292

kw_module_target_4292(; x=1, y=2) = x + y
positional_module_target_4292(x, y; scale=1) = (x + y) * scale

end

using .KwargsSplatModule4292

@testset "kwargs... Pairs forwarding (Issue #4292)" begin
    @test kwargs_pairs_forward_outer_4292(; x=4) == 5
    @test kwargs_pairs_forward_outer_pos_4292(3; y=5) == 8

    args = (2, 3)
    @test kwargs_positional_splat_4292(args...) == 5
    @test kwargs_closure_splat_4292(args...) == 6

    nt = (x=4, y=5)
    scale_kw = (scale=4,)
    callable = KwargsCallable4292()
    @test kwargs_known_pos_kw_splat_4292(args...; scale_kw...) == 20
    @test kwargs_function_value_4292(args...; scale_kw...) == 20
    @test kwargs_nested_closure_kw_splat_4292(1) == 8
    @test callable(args...; scale_kw...) == 20
    @test KwargsSplatModule4292.kw_module_target_4292(; nt...) == 9
    @test KwargsSplatModule4292.positional_module_target_4292(args...; scale_kw...) == 20
end

true
