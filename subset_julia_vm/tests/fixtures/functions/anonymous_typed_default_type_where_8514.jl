using Test

short_anon_typed_default_8514(v::Val{N}, ::Type{T}=Float64) where {N,T<:Real} = T

function block_anon_typed_default_8514(v::Val{N}, ::Type{T}=Float64) where {N,T<:Real}
    T
end

@testset "anonymous typed default Type argument with where clause (Issue #8514)" begin
    @test short_anon_typed_default_8514(Val{2}()) == Float64
    @test short_anon_typed_default_8514(Val{2}(), Float32) == Float32

    @test block_anon_typed_default_8514(Val{3}()) == Float64
    @test block_anon_typed_default_8514(Val{3}(), Float32) == Float32
end

true
