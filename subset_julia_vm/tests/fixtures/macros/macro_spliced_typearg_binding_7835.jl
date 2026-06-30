using Test

macro vector_of(T)
    esc(:(Vector{$T}))
end

GlobalElementType = Int64

function vector_from_local_type()
    LocalElementType = Float64
    @vector_of(LocalElementType)
end

@testset "macro-spliced parametric type arguments evaluate caller bindings" begin
    @test (@vector_of(GlobalElementType)) == Vector{Int64}
    @test vector_from_local_type() == Vector{Float64}
end

true
