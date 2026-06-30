# Issue #4804: Float32 print didn't use scientific notation for
# very small / very large magnitudes — same bug family as the
# just-fixed #4802 but for `format_float32_julia` instead of
# `format_float_julia`. The Float64 fix in PR #4803 didn't
# propagate to the Float32 helper.
#
# Fix: format_float32_julia now lowers the whole-number fixed-form
# upper bound from 1e7 to 1e6 and routes magnitudes outside
# [1e-4, 1e6) through format_float32_scientific_julia (which uses
# Rust's f32 {:e} so the printed mantissa reflects f32's shortest
# round-trip rather than the wider f64 cast).
#
# `print` of a Float32 strips the `f0` repr suffix, so the
# scientific-notation output is `1.5e-10` (not `1.5e-10f0`).

using Test

@testset "Float32 scientific notation — very small (Issue #4804)" begin
    @test string(Float32(1.5e-10)) == "1.5e-10"
    @test string(Float32(1.0e-5)) == "1.0e-5"
end

@testset "Float32 scientific notation — very large (Issue #4804)" begin
    @test string(Float32(1.5e20)) == "1.5e20"
    @test string(Float32(2.5e15)) == "2.5e15"
    @test string(Float32(1.5e6)) == "1.5e6"
end

@testset "Float32 normal range stays fixed-point (Issue #4804)" begin
    @test string(Float32(0.0)) == "0.0"
    @test string(Float32(0.5)) == "0.5"
    @test string(Float32(3.14)) == "3.14"
    @test string(Float32(2.0)) == "2.0"
    @test string(Float32(150000.0)) == "150000.0"
    @test string(Float32(0.0001)) == "0.0001"
end

@testset "Float32 threshold boundary (Issue #4804)" begin
    @test string(Float32(1.0e5)) == "100000.0"  # fixed
    @test string(Float32(1.0e6)) == "1.0e6"      # scientific
end

@testset "Float32 special values unchanged (Issue #4804)" begin
    @test string(Float32(NaN)) == "NaN"
    @test string(Float32(Inf)) == "Inf"
    @test string(Float32(-Inf)) == "-Inf"
    @test string(Float32(-0.0)) == "-0.0"
end

true
