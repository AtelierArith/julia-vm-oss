using Test

# Issue #8884: repr/show strings drift for Float16 and Float32 Inf/NaN.
# Float16 and Float32 special values should format as "Inf16", "-Inf16",
# "NaN16", "Inf32", "-Inf32", "NaN32" — matching upstream Julia.

let
    # Float32 special values
    @test repr(Inf32) == "Inf32"
    @test repr(-Inf32) == "-Inf32"
    @test repr(NaN32) == "NaN32"

    # Float16 special values
    @test repr(Float16(Inf)) == "Inf16"
    @test repr(Float16(-Inf)) == "-Inf16"
    @test repr(Float16(NaN)) == "NaN16"

    # Normal values should still work
    @test repr(Float32(1.5)) == "1.5f0"
    @test repr(Float16(1.5)) == "Float16(1.5)"
end

true
