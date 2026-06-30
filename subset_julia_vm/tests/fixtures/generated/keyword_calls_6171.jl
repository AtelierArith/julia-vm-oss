using Test

# Issue #6171 / #5936: @generated keyword calls should run the generated body
# with concrete type objects for both positional and keyword arguments. The
# returned Expr is then evaluated against the runtime argument frame.

@generated function generated_keyword_runtime_eval_6171(x; y=1)
    x == Int64 ? :(x + y) : :(0)
end

@generated function generated_keyword_type_cache_6171(x; y=1)
    if y == Int64
        return :(10)
    elseif y == Float64
        return :(20)
    else
        return :(30)
    end
end

@testset "generated keyword calls (Issue #6171)" begin
    @test generated_keyword_runtime_eval_6171(2) == 3
    @test generated_keyword_runtime_eval_6171(2; y=3) == 5

    kw = (; y=4)
    @test generated_keyword_runtime_eval_6171(2; kw...) == 6

    @test generated_keyword_type_cache_6171(1) == 10
    @test generated_keyword_type_cache_6171(1; y=2) == 10
    @test generated_keyword_type_cache_6171(1; y=2.0) == 20
end

kw_6171 = (; y=4)
generated_keyword_runtime_eval_6171(2) == 3 &&
    generated_keyword_runtime_eval_6171(2; y=3) == 5 &&
    generated_keyword_runtime_eval_6171(2; kw_6171...) == 6 &&
    generated_keyword_type_cache_6171(1) == 10 &&
    generated_keyword_type_cache_6171(1; y=2) == 10 &&
    generated_keyword_type_cache_6171(1; y=2.0) == 20
