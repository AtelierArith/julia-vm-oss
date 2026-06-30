using Test

# Issue #5775: a typed array literal using the alias eltype `ComplexF64[...]` /
# `ComplexF32[...]` inferred `Vector{Any}` instead of `Vector{ComplexF64}` (the
# canonical `Complex{Float64}[...]` spelling already worked). Two gaps: the bare
# identifier eltype `ComplexF64` was not mapped to the `ComplexF64` array element
# type, and the verbatim-store push could not resolve a heap `StructRef` complex
# element into the interleaved Complex array.

@testset "ComplexFNN alias array literal eltype (Issue #5775)" begin
    a = ComplexF64[1.0 + 2im, 3.0 + 4im]
    @test typeof(a) == Vector{ComplexF64}
    @test a == Complex{Float64}[1.0 + 2im, 3.0 + 4im]
    @test a[1] == 1.0 + 2.0im
    @test sum(a) == 4.0 + 6.0im
    @test real(a[2]) == 3.0

    b = ComplexF32[1.0f0 + 2.0f0im, 3.0f0 + 4.0f0im]
    @test typeof(b) == Vector{ComplexF32}
    @test typeof(b[1]) == ComplexF32

    # Empty alias-typed literal
    @test typeof(ComplexF64[]) == Vector{ComplexF64}
    @test isempty(ComplexF64[])

    # Broadcast over the alias-typed array preserves the element type
    @test typeof(a .+ 1) == Vector{ComplexF64}
    @test (a .+ 1)[1] == 2.0 + 2.0im
    @test (a .+ 1)[2] == 4.0 + 4.0im
end

true
