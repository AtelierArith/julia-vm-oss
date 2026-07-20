using Test

# Issue #5666: == and === on ranges were broken — `(1:5) == (1:5)` was a compile
# error ("Cannot convert Range to I64") and `===` on ranges returned false. Ranges
# are AbstractArrays, so == compares element-wise; ranges are immutable, so === is
# structural.

@testset "== and === on ranges (Issue #5666)" begin
    # == (element-wise)
    @test (1:5) == (1:5)
    @test !((1:5) == (1:6))
    @test (1:2:9) == (1:2:9)
    @test !((1:5) == (1:2:9))
    @test (1.0:0.5:3.0) == (1.0:0.5:3.0)
    @test (1:0) == (1:0)              # empty ranges

    # != 
    @test (1:5) != (1:6)
    @test !((1:5) != (1:5))

    # range vs array (both directions)
    @test (1:5) == [1, 2, 3, 4, 5]
    @test [1, 2, 3] == (1:3)
    @test !((1:5) == [1, 2, 3])

    # range vs non-array scalar → false (identity fallback)
    @test !((1:5) == 3)
    @test !(3 == (1:5))

    # === (structural for immutable ranges)
    @test (1:5) === (1:5)
    @test (1:2:9) === (1:2:9)
    @test !((1:5) === (1:6))
    @test !((1:5) === (2:6))
    @test !((1:1:5) === (1:5))
    @test !((UInt8(1):UInt8(3)) === (1:3))

    # explicit step 1 still equals (== is element-wise)
    @test (1:1:5) == (1:5)
    @test (UInt8(1):UInt8(3)) == (1:3)
end

true
