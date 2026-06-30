using Test

# Issue #5704: Complex{FloatNN} type values must DISPLAY via their upstream
# aliases ComplexFNN (ComplexF64/F32/F16) in show/print/string/repr, matching
# Julia. The type identity is already correct (Complex{Float64} === ComplexF64);
# this is a display-only alias gap. Complex{Int64} (no alias) is unchanged.
# The aliasing is applied at the display site only (not in JuliaType::name(),
# which dispatch relies on) and is boundary-aware + recursive.

@testset "ComplexFNN alias display (Issue #5704)" begin
    # Identity is unaffected
    @test Complex{Float64} === ComplexF64
    @test Complex{Float32} === ComplexF32

    # Top-level type-value display
    @test string(Complex{Float64}) == "ComplexF64"
    @test string(Complex{Float32}) == "ComplexF32"
    @test string(Complex{Float16}) == "ComplexF16"
    @test repr(Complex{Float64}) == "ComplexF64"
    @test "$(Complex{Float32})" == "ComplexF32"

    # typeof of a complex value
    @test string(typeof(1.0 + 2.0im)) == "ComplexF64"
    @test string(typeof(1.0f0 + 2.0f0im)) == "ComplexF32"

    # Recursive: alias applies inside container/tuple type params
    @test string(Vector{Complex{Float64}}) == "Vector{ComplexF64}"
    @test string(Tuple{Complex{Float64},Int}) == "Tuple{ComplexF64, Int64}"

    # Complex{Int64} is NOT a registered alias — unchanged
    @test string(Complex{Int64}) == "Complex{Int64}"

    # Array literal prefix (native-carrier path) aliases too
    @test string(Complex{Float64}[1.0 + 2.0im]) == "ComplexF64[1.0 + 2.0im]"
end

true
