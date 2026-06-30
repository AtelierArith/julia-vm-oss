using Test

@testset "similar with Nothing element type preserves Vector{Nothing}" begin
    empty = similar(Float64[], Nothing)
    @test typeof(empty) == Vector{Nothing}
    @test empty isa Vector{Nothing}
    @test eltype(empty) == Nothing
    @test length(empty) == 0

    sized = similar(Float64[], Nothing, 2)
    @test typeof(sized) == Vector{Nothing}
    @test eltype(sized) == Nothing
    @test length(sized) == 2
    sized[1] = nothing
    @test sized[1] === nothing
end

true
