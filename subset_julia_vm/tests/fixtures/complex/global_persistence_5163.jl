# Issue #5163: Complex globals are persisted through the same generic
# struct-persistence path Rational already uses (StructRef -> struct_instances
# -> struct_instance_to_literal), instead of a Complex-specific (f64, f64)
# REPL slot that lost the real struct_name and element type.
#
# This fixture verifies that bare Complex globals of every element type keep
# their exact type and value, mirroring upstream Julia. Re-injection of these
# globals (the REPL round-trip) must preserve Complex{Int64}, Complex{Float64},
# and Complex{Float32} faithfully rather than collapsing them to
# Complex{Float64} with (f64, f64) fields.

using Test

# Globals of each element type
z_int = Complex(3, 4)
z_f64 = Complex(1.5, -2.5)
z_f32 = Complex{Float32}(1.5f0, 2.5f0)

@testset "Complex{Int64} global keeps type and parts (Issue #5163)" begin
    @test typeof(z_int) === Complex{Int64}
    @test real(z_int) === 3
    @test imag(z_int) === 4
    @test z_int == 3 + 4im
end

@testset "Complex{Float64} global keeps type and parts (Issue #5163)" begin
    @test typeof(z_f64) === Complex{Float64}
    @test real(z_f64) === 1.5
    @test imag(z_f64) === -2.5
end

@testset "Complex{Float32} global keeps type and parts (Issue #5163)" begin
    @test typeof(z_f32) === Complex{Float32}
    @test real(z_f32) === 1.5f0
    @test imag(z_f32) === 2.5f0
end

# Reassigning a Complex global to a different element type must replace it
# cleanly (the old REPL slot used to special-case this path).
z_int = Complex{Float32}(7.0f0, 8.0f0)
@testset "Complex global reassignment replaces element type (Issue #5163)" begin
    @test typeof(z_int) === Complex{Float32}
    @test real(z_int) === 7.0f0
    @test imag(z_int) === 8.0f0
end

true
