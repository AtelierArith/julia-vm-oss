using Test

@testset "Core type parity regressions (Issue #8415)" begin
    @test typejoin(Tuple{Int,Int}, Tuple{Float64,Float64}) == Tuple{Real,Real}
    @test typejoin(Tuple{Int}, Tuple{Int,Int}) == Tuple{Int,Vararg{Int}}
    @test typejoin(Val{1}, Val{2}) == Val

    @test typeintersect(Tuple{T,T} where T, Tuple{Int,Float64}) == Union{}
    @test typeintersect(Tuple{T,T} where T, Tuple{Int,Real}) == Tuple{Int,Int}

    f_cross_bound_8415(x::T, y::S) where {T<:Real,S<:T} = (T, S)
    @test f_cross_bound_8415(1, 1) == (Int, Int)
    @test_throws Exception f_cross_bound_8415(1, 2.0)

    f_lower_bound_8415(x::T) where {T>:Int} = T
    @test f_lower_bound_8415(1) == Int
    @test_throws Exception f_lower_bound_8415(1.0)

    @test Vector{Int} <: (Vector{T} where T<:S where S<:Real)
    @test !(Vector{String} <: (Vector{T} where T<:S where S<:Real))

    @test typejoin(Vector{Int}, Matrix{Float64}) == Array
    @test typejoin(Vector{Int}, Matrix{Int}) == Array{Int}

    @test Vector{Int} === (Vector{T} where T){Int}
end

true
