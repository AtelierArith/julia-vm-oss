using Test

import Base: similar

similar(v::Vector{Int64}) = [:user_similar_4276]

function similar(v::Vector{Int64}, n::Int64)
    if n == 3
        return :user_dim_similar_4276
    end
    return :wrong_dim_similar_4276
end

function similar(v::Vector{Int64}, ::Type{Float64}, n::Int64)
    if n == 3
        return :user_typed_dim_similar_4276
    end
    return :wrong_typed_dim_similar_4276
end

similar_any_4276(x::Any) = similar(x)
similar_any_dim_4276(x::Any) = similar(x, 3)
similar_any_typed_dim_4276(x::Any) = similar(x, Float64, 3)

@testset "similar Any path uses user method before fallback (Issue #4276)" begin
    @test similar([1, 2, 3]) == [:user_similar_4276]
    @test similar_any_4276([1, 2, 3]) == [:user_similar_4276]
    @test similar([1, 2, 3], 3) == :user_dim_similar_4276
    @test similar_any_dim_4276([1, 2, 3]) == :user_dim_similar_4276
    @test similar([1, 2, 3], Float64, 3) == :user_typed_dim_similar_4276
    @test similar_any_typed_dim_4276([1, 2, 3]) == :user_typed_dim_similar_4276

    xs = similar([1.0, 2.0])
    ys = similar_any_4276([1.0, 2.0])
    zs = similar_any_dim_4276([1.0, 2.0])
    ws = similar_any_typed_dim_4276([1.0, 2.0])
    @test length(xs) == 2
    @test length(ys) == 2
    @test length(zs) == 3
    @test length(ws) == 3
    @test eltype(ws) == Float64
end

true
