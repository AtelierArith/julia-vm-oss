using Test

# Issue #3707: Float16 values printed with the type wrapper (`Float16(5)`)
# instead of the bare numeric form Julia uses (`5.0`). The fix adds a
# `format_float16_julia` helper that strips the wrapper and emits the
# shortest decimal that round-trips through Float16 — matching Julia's
# Grisu/Ryu-style display algorithm for Float16 specifically.
@testset "Float16 print formatting (Issue #3707)" begin
    # Whole numbers get the .0 suffix (no type wrapper)
    @test sprint(print, Float16(5.0)) == "5.0"
    @test sprint(print, Float16(-5.0)) == "-5.0"
    @test sprint(print, Float16(0.0)) == "0.0"
    @test sprint(print, Float16(1.0)) == "1.0"

    # Signed zero is preserved
    @test sprint(print, Float16(-0.0)) == "-0.0"

    # Non-whole values: shortest representation that round-trips through Float16.
    # Julia stores Float16(3.14) as ~3.140625 internally, but `print` shows 3.14
    # because that decimal parses back to the same Float16 value.
    @test sprint(print, Float16(3.14)) == "3.14"
    @test sprint(print, Float16(1.5)) == "1.5"
    @test sprint(print, Float16(0.0625)) == "0.0625"

    # Special values
    @test sprint(print, Float16(NaN)) == "NaN"
    @test sprint(print, Float16(Inf)) == "Inf"
    @test sprint(print, Float16(-Inf)) == "-Inf"

    # `string()` should match too
    @test string(Float16(5.0)) == "5.0"
    @test string(Float16(3.14)) == "3.14"
end

true
