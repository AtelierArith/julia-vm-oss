using Test

import Base: reshape

function reshape(v::Vector{Int64}, n::Int64)
    if n == 3
        return :user_reshape_4276
    end
    return :wrong_reshape_4276
end

reshape_any_4276(x::Any) = reshape(x, 3)

@testset "reshape user method wins before VM fallback (Issue #4276)" begin
    @test reshape([1, 2, 3], 3) == :user_reshape_4276
    @test reshape_any_4276([1, 2, 3]) == :user_reshape_4276

    xs = reshape([1.0, 2.0, 3.0], 3)
    ys = reshape_any_4276([1.0, 2.0, 3.0])
    @test length(xs) == 3
    @test length(ys) == 3
    @test xs[1] == 1.0
    @test ys[1] == 1.0
end

true
