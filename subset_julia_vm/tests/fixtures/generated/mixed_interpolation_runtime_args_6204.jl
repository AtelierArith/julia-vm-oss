using Test

# Issue #6204 / #5074: in generated returned code, `$a` / `$b` splice
# generated-time type objects, while bare `a` / `b` must still resolve against
# the runtime call frame.

@generated function generated_mixed_interpolation_tail_6204(a, b...)
    :(($a, $b, a, b))
end

@generated function generated_mixed_interpolation_return_6204(a, b...)
    ex = :(($a, $b, a, b))
    return ex
end

@generated generated_mixed_interpolation_short_6204(a, b...) = :(($a, $b, a, b))

const GENERATED_MIXED_EXPECTED_INT_6204 = (Int64, (Int64, Int64), 1, (2, 3))
const GENERATED_MIXED_EXPECTED_FLOAT_6204 = (Float64, (Int64, Int64), 1.5, (2, 3))

@testset "generated mixed interpolation/runtime args (Issue #6204)" begin
    @test generated_mixed_interpolation_tail_6204(1, 2, 3) == GENERATED_MIXED_EXPECTED_INT_6204
    @test generated_mixed_interpolation_tail_6204(1.5, 2, 3) == GENERATED_MIXED_EXPECTED_FLOAT_6204
    @test generated_mixed_interpolation_return_6204(1, 2, 3) == GENERATED_MIXED_EXPECTED_INT_6204
    @test generated_mixed_interpolation_return_6204(1.5, 2, 3) == GENERATED_MIXED_EXPECTED_FLOAT_6204
    @test generated_mixed_interpolation_short_6204(1, 2, 3) == GENERATED_MIXED_EXPECTED_INT_6204
    @test generated_mixed_interpolation_short_6204(1.5, 2, 3) == GENERATED_MIXED_EXPECTED_FLOAT_6204
end

generated_mixed_interpolation_tail_6204(1, 2, 3) == GENERATED_MIXED_EXPECTED_INT_6204 &&
    generated_mixed_interpolation_tail_6204(1.5, 2, 3) == GENERATED_MIXED_EXPECTED_FLOAT_6204 &&
    generated_mixed_interpolation_return_6204(1, 2, 3) == GENERATED_MIXED_EXPECTED_INT_6204 &&
    generated_mixed_interpolation_return_6204(1.5, 2, 3) == GENERATED_MIXED_EXPECTED_FLOAT_6204 &&
    generated_mixed_interpolation_short_6204(1, 2, 3) == GENERATED_MIXED_EXPECTED_INT_6204 &&
    generated_mixed_interpolation_short_6204(1.5, 2, 3) == GENERATED_MIXED_EXPECTED_FLOAT_6204
