# Test abs2 return type for Complex{Float64} (Issue #3466)
# Note: Complex{Float32} abs2 runtime return type is tracked separately

using Test

@testset "type_inference_complex_abs2_type: abs2(Complex{Float64}) returns Float64" begin
    z = Complex{Float64}(1.0, 2.0)
    @test typeof(z) == Complex{Float64}
    @test typeof(abs2(z)) == Float64
end

true
