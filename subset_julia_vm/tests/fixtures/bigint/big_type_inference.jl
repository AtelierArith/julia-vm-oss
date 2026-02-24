# Test that big() return type is correctly inferred for method dispatch (Issue #1910)
# Previously, big(x) was inferred as JuliaType::Any, causing incorrect method dispatch
# in functions like gcd(a::BigInt, b::Int64) which call gcd(a, big(b))

using Test

# Test 1: BigInt() constructor type inference
# BigInt(x) should be inferred as BigInt for proper dispatch
function test_bigint_constructor()
    x = BigInt(42)
    return isa(x, BigInt)
end

# Test 2: big() function type inference
# big(x) should be inferred as BigInt (for integer args) for proper dispatch
function test_big_function()
    x = big(42)
    return isa(x, BigInt)
end

# Test 3: big() used in mixed-type function dispatch
# This was the original failure case: gcd(a::BigInt, b::Int64) calls gcd(a, big(b))
# which should dispatch to gcd(::BigInt, ::BigInt), not recurse infinitely
function test_big_dispatch()
    # Test gcd with mixed BigInt/Int64 types
    result = gcd(BigInt(48), 18)
    return result == BigInt(6)
end

# Test 4: big() with float argument should infer BigFloat
function test_big_float_inference()
    x = big(3.14)
    return isa(x, BigFloat)
end

@testset "big() type inference for method dispatch (Issue #1910)" begin
    @test test_bigint_constructor()
    @test test_big_function()
    @test test_big_dispatch()
    @test test_big_float_inference()
end

true
