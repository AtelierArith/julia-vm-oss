# Test Int128/BigInt/BigFloat literal type preservation in inference (Issue #3530)
# Previously, infer_literal() narrowed:
#   Literal::Int128 / Literal::BigInt -> ConcreteType::Int64
#   Literal::BigFloat                -> ConcreteType::Float64
# This caused dispatch and `typeof`-via-inference paths to misclassify these
# literals as Int64/Float64 instead of preserving their actual types.

using Test

# typeof on direct big"..." integer literal
function test_bigint_literal_typeof()
    x = big"9223372036854775808"  # 2^63, beyond Int64 max
    return typeof(x)
end

# typeof on direct big"..." float literal
function test_bigfloat_literal_typeof()
    x = big"1.25"
    return typeof(x)
end

# typeof on Int128 literal-producing call
function test_int128_literal_typeof()
    x = Int128(1)
    return typeof(x)
end

# Method dispatch with BigInt literal
foo_3530(x::Int64)   = "int64"
foo_3530(x::BigInt)  = "bigint"
foo_3530(x::BigFloat) = "bigfloat"

function test_bigint_dispatch()
    x = big"9223372036854775808"
    return foo_3530(x)
end

function test_bigfloat_dispatch()
    x = big"1.25"
    return foo_3530(x)
end

@testset "Int128/BigInt/BigFloat literal inference (Issue #3530)" begin
    @test test_bigint_literal_typeof() == BigInt
    @test test_bigfloat_literal_typeof() == BigFloat
    @test test_int128_literal_typeof() == Int128

    @test test_bigint_dispatch() == "bigint"
    @test test_bigfloat_dispatch() == "bigfloat"
end

true
