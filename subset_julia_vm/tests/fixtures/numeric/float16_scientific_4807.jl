# Issue #4807: Float16 print didn't use scientific notation for
# large magnitudes — third member of the #4802 / #4804 fix family.
#
# Fix: format_float16_julia now lowers the whole-number fixed-form
# upper bound from 1e5 to 1e3 (matching F16's narrower magnitude
# range) and adds a scientific arm for |x| outside [1e-4, 1e3),
# using a round-trip mantissa search via {:.*e} for shortest faithful
# form.

using Test

@testset "Float16 scientific — large magnitudes (Issue #4807)" begin
    @test string(Float16(1.5e3)) == "1.5e3"
    @test string(Float16(1.5e4)) == "1.5e4"
    @test string(Float16(1000.0)) == "1.0e3"   # threshold boundary
end

@testset "Float16 scientific — small magnitudes (Issue #4807)" begin
    @test string(Float16(1.5e-5)) == "1.5e-5"
end

@testset "Float16 normal range stays fixed-point (Issue #4807)" begin
    @test string(Float16(0.5)) == "0.5"
    @test string(Float16(3.14)) == "3.14"
    @test string(Float16(0.0)) == "0.0"
    @test string(Float16(999.0)) == "999.0"     # still fixed
    @test string(Float16(0.0001)) == "0.0001"   # still fixed (1e-4 threshold)
    @test string(Float16(0.00015)) == "0.00015"
end

@testset "Float16 special values unchanged (Issue #4807)" begin
    @test string(Float16(NaN)) == "NaN"
    @test string(Float16(Inf)) == "Inf"
    @test string(Float16(-Inf)) == "-Inf"
    @test string(Float16(-0.0)) == "-0.0"
end

true
