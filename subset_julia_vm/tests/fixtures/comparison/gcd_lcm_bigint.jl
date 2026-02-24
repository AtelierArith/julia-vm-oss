# Test gcd and lcm with BigInt types
# Issue #505: gcd(::BigInt, ::BigInt) was not supported

using Test

@testset "gcd and lcm with BigInt types (Issue #505)" begin

    # Int64 gcd and lcm
    println(gcd(48, 18))        # 6
    println(lcm(12, 18))        # 36

    # BigInt gcd and lcm using BigInt constructor
    println(gcd(BigInt(48), BigInt(18)))    # 6
    println(lcm(BigInt(12), BigInt(18)))    # 36

    # BigInt gcd and lcm using big() function
    println(gcd(big(48), big(18)))          # 6
    println(lcm(big(12), big(18)))          # 36

    # Mixed types: BigInt and Int64
    println(gcd(BigInt(48), 18))            # 6
    println(gcd(48, BigInt(18)))            # 6
    println(lcm(BigInt(12), 18))            # 36
    println(lcm(12, BigInt(18)))            # 36

    # Large numbers
    println(gcd(big(1000000000000000000), big(123456789)))  # 9
    println(lcm(big(123), big(456)))                         # 18696

    # Verification
    @test (gcd(48, 18) == 6 && gcd(BigInt(48), BigInt(18)) == big(6) && lcm(12, 18) == 36)
end

true  # Test passed
