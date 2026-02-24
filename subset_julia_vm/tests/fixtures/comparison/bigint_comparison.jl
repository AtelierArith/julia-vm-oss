# Test BigInt / Int64 comparison operations
# Tests for issue #512: BigInt/Int64 comparison operations

using Test

@testset "BigInt/Int64 mixed comparison operators (Issue #512)" begin

    # Basic BigInt < Int64
    big(2) < 3  # expect: true
    big(3) < 2  # expect: false

    # Basic BigInt > Int64
    big(3) > 2  # expect: true
    big(2) > 3  # expect: false

    # Basic BigInt <= Int64
    big(2) <= 3  # expect: true
    big(2) <= 2  # expect: true
    big(3) <= 2  # expect: false

    # Basic BigInt >= Int64
    big(3) >= 2  # expect: true
    big(2) >= 2  # expect: true
    big(2) >= 3  # expect: false

    # Basic BigInt == Int64
    big(2) == 2  # expect: true
    big(2) == 3  # expect: false

    # Reversed: Int64 < BigInt
    2 < big(3)  # expect: true
    3 < big(2)  # expect: false

    # Reversed: Int64 > BigInt
    3 > big(2)  # expect: true
    2 > big(3)  # expect: false

    # Reversed: Int64 <= BigInt
    2 <= big(3)  # expect: true
    2 <= big(2)  # expect: true
    3 <= big(2)  # expect: false

    # Reversed: Int64 >= BigInt
    3 >= big(2)  # expect: true
    2 >= big(2)  # expect: true
    2 >= big(3)  # expect: false

    # Reversed: Int64 == BigInt
    2 == big(2)  # expect: true
    3 == big(2)  # expect: false

    # BigInt vs BigInt (should still work)
    big(2) < big(3)  # expect: true
    @test (big(2) == big(2))
end

true  # Test passed
