# Issue #5160: the VM's Rational numerator/denominator extraction was
# duplicated (with slightly diverging integer-type coverage) across
# `exec/conversion.rs` and `stack_ops.rs`. It is now centralized on a single
# `StructInstance::as_rational_parts_{i64,f64}` helper (mirroring the Complex
# `as_complex_parts` helper). This guards that the consolidated path keeps
# extracting Rationals of every supported integer field type correctly.
#
# `sqrt(::Rational)` reaches the helper via `pop_f64_or_i64` (the f64-coercion
# used by the numeric builtins), so it exercises the Rust fast path directly
# (unlike `float(::Rational)`, which is pure Julia).

using Test

@testset "Rational VM extraction is generic over integer field types (#5160)" begin
    # Default Rational{Int64} field representation.
    @test isapprox(sqrt(1 // 4), 0.5)
    @test isapprox(sqrt(1 // 2), 0.7071067811865476)

    # Narrow integer field representations (Int32 / Int16 / Int8) all extract
    # through the same consolidated helper.
    @test isapprox(sqrt(Rational{Int32}(1, 4)), 0.5)
    @test isapprox(sqrt(Rational{Int16}(1, 4)), 0.5)
    @test isapprox(sqrt(Rational{Int8}(1, 4)), 0.5)
    @test isapprox(float(Rational{Int8}(1, 2)), 0.5)
    @test isapprox(float(Rational{Int16}(3, 4)), 0.75)

    # Mixed Rational / Float arithmetic keeps coercing the Rational operand.
    @test isapprox(2.0 * (1 // 2), 1.0)
    @test isapprox((3 // 4) + 0.25, 1.0)
    @test isapprox(abs(float(-3 // 4)), 0.75)
end

true  # Test passed
