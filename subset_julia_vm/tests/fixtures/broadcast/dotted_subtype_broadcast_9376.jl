# Dotted subtype operator broadcasts through broadcast(<:, ...) (Issue #9376)

using Test

@testset "dotted subtype broadcast (Issue #9376)" begin
    types = [Int64, Float64, String]
    @test (types .<: Number) == Bool[true, true, false]
    @test ([Vector{Int64}, Matrix{Int64}, String] .<: AbstractArray) == Bool[true, true, false]
    @test ([Number, String] .>: Int64) == Bool[true, false]
end

true
