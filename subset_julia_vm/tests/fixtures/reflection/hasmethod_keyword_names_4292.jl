using Test

hasmethod_kw_named_4292(; oranges=0) = oranges
hasmethod_kw_rest_4292(; xs...) = length(xs)

@testset "hasmethod keyword-name filtering (Issue #4292)" begin
    @test hasmethod(hasmethod_kw_named_4292, Tuple{}, (:oranges,))
    @test !hasmethod(hasmethod_kw_named_4292, Tuple{}, (:apples,))
    @test hasmethod(hasmethod_kw_named_4292, Tuple{}, ())
    @test hasmethod(hasmethod_kw_rest_4292, Tuple{}, (:a, :b))
end

true
