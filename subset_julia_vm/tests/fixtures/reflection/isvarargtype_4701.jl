using Test

@testset "Base.isvarargtype recognises Vararg-bearing DataTypes (Issue #4701)" begin
    @test Base.isvarargtype(Vararg{Int})
    @test Base.isvarargtype(Vararg{Float64})
    @test !Base.isvarargtype(Int)
    @test !Base.isvarargtype(Float64)
    @test !Base.isvarargtype(Tuple{Int, Int})
    @test !Base.isvarargtype(Vector{Int})
end

@testset "Base.isvatuple detects trailing Vararg in Tuple types (Issue #4701)" begin
    @test Base.isvatuple(Tuple{Int, Vararg{Int}})
    @test Base.isvatuple(Tuple{Vararg{Int}})
    @test Base.isvatuple(Tuple{Int, String, Vararg{Any}})
    @test !Base.isvatuple(Tuple{Int, Int})
    @test !Base.isvatuple(Tuple{})
    @test !Base.isvatuple(Tuple{Int, String})
end

true
