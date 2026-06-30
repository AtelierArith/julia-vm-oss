using Test

struct RegistryLayout3909
    x::Int64
    y::Bool
end

mutable struct RegistryMutable3909
    x::Int64
end

struct RegistryReference3909
    name::String
    x::Int64
end

@testset "user struct layout metadata is registry-backed (Issue #3909)" begin
    @test fieldnames(RegistryLayout3909) == (:x, :y)
    @test fieldtypes(RegistryLayout3909) == (Int64, Bool)
    @test fieldcount(RegistryLayout3909) == 2
    @test isbitstype(RegistryLayout3909)
    @test sizeof(RegistryLayout3909) == 16

    @test fieldnames(RegistryMutable3909) == (:x,)
    @test fieldtypes(RegistryMutable3909) == (Int64,)
    @test fieldcount(RegistryMutable3909) == 1
    @test !isbitstype(RegistryMutable3909)
    @test sizeof(RegistryMutable3909) == 8

    @test fieldnames(RegistryReference3909) == (:name, :x)
    @test fieldtypes(RegistryReference3909) == (String, Int64)
    @test !isbitstype(RegistryReference3909)
    @test sizeof(RegistryReference3909) == 16
end

true
