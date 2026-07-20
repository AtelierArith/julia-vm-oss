using Test

function bigfloat_cmp_expect_8607(a, b, lt, le, gt, ge, eq, ne)
    return ((a < b) == lt) &&
        ((a <= b) == le) &&
        ((a > b) == gt) &&
        ((a >= b) == ge) &&
        ((a == b) == eq) &&
        ((a != b) == ne)
end

function bigfloat_zero_matrix_8607(pos, neg, z)
    return bigfloat_cmp_expect_8607(pos, z, false, false, true, true, false, true) &&
        bigfloat_cmp_expect_8607(z, pos, true, true, false, false, false, true) &&
        bigfloat_cmp_expect_8607(neg, z, true, true, false, false, false, true) &&
        bigfloat_cmp_expect_8607(z, neg, false, false, true, true, false, true) &&
        bigfloat_cmp_expect_8607(z, z, false, true, false, true, true, false)
end

function bigfloat_any_slot_8607(x)
    xs = Any[x]
    return xs[1]
end

function bigfloat_real_slot_8607(x)
    xs = Real[x]
    return xs[1]
end

@testset "BigFloat zero-boundary comparison matrix (Issue #8607)" begin
    tiny_pos = big"1e-78"
    tiny_neg = big"-1e-78"
    zero_big = big"0.0"

    @test bigfloat_zero_matrix_8607(tiny_pos, tiny_neg, zero_big)
    @test bigfloat_zero_matrix_8607(
        bigfloat_any_slot_8607(tiny_pos),
        bigfloat_any_slot_8607(tiny_neg),
        bigfloat_any_slot_8607(zero_big),
    )
    @test bigfloat_zero_matrix_8607(
        bigfloat_real_slot_8607(tiny_pos),
        bigfloat_real_slot_8607(tiny_neg),
        bigfloat_real_slot_8607(zero_big),
    )
end

true
