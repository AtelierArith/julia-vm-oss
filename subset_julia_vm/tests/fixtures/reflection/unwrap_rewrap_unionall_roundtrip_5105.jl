using Test

@testset "unwrap_unionall on concrete types is a no-op (Issue #5105)" begin
    @test Base.unwrap_unionall(Int64) === Int64
    @test Base.unwrap_unionall(Float64) === Float64
    @test Base.unwrap_unionall(Vector{Int64}) === Vector{Int64}
    @test Base.unwrap_unionall(Dict{Symbol,Int64}) === Dict{Symbol,Int64}
    @test !isa(Base.unwrap_unionall(Vector{Int64}), UnionAll)
end

@testset "unwrap_unionall strips outer UnionAll wrappers (Issue #5105)" begin
    @test isa(Vector, UnionAll)
    @test !isa(Base.unwrap_unionall(Vector), UnionAll)
    @test !isa(Base.unwrap_unionall(Dict), UnionAll)
    @test !isa(Base.unwrap_unionall(Set), UnionAll)
end

@testset "rewrap_unionall round-trips unwrap_unionall (Issue #5105)" begin
    @test Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector) === Vector
    @test Base.rewrap_unionall(Base.unwrap_unionall(Set), Set) === Set
    @test Base.rewrap_unionall(Base.unwrap_unionall(Dict), Dict) === Dict
    # rewrap onto a non-UnionAll returns the body unchanged
    @test Base.rewrap_unionall(Int64, Int64) === Int64
    @test Base.rewrap_unionall(Int64, Vector) === Int64
    # round-trip result is again a UnionAll
    @test isa(Base.rewrap_unionall(Base.unwrap_unionall(Vector), Vector), UnionAll)
end

true
