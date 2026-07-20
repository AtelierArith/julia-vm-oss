using Test

@testset "tuple destructuring assignment in expression position (Issue #9792)" begin
    y = ((a, b) = (1, 2))
    @test y == (1, 2)
    @test (a, b) == (1, 2)

    nt_value = ((c, d) = (x = 3, y = 4))
    @test nt_value == (x = 3, y = 4)
    @test (c, d) == (3, 4)

    nested_value = ((p, (q, r)) = (5, (6, 7)))
    @test nested_value == (5, (6, 7))
    @test (p, q, r) == (5, 6, 7)

    function expression_body_9792(t)
        ((m, n) = t)
    end
    @test expression_body_9792((8, 9)) == (8, 9)

    lhs = [0, 0]
    indexed_value = ((lhs[1], lhs[2]) = (10, 11))
    @test indexed_value == (10, 11)
    @test lhs == [10, 11]

    @test_throws BoundsError ((r1, r2, r3, r4) = (a = 12, b = 13, c = 14))
end

true
