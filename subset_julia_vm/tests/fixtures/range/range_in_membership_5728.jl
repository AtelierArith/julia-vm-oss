using Test

# Issue #5728: `x in range` (membership in a Range) errored — the `in` builtin
# did not accept a Range as the container. Now membership is tested arithmetically.

@testset "membership in a range (Issue #5728)" begin
    @test (2 in 1:5) == true
    @test (7 in 1:5) == false
    @test (0 in 1:5) == false
    @test (5 in 1:5) == true
    @test in(3, 1:5) == true

    # step ranges
    @test (3 in 1:2:9) == true
    @test (4 in 1:2:9) == false
    @test (3 in 1:1:5) == true
    @test (10 in 1:1:5) == false

    # negative step
    @test (2 in 5:-1:1) == true
    @test (6 in 5:-1:1) == false

    # float element vs integer range
    @test (2.0 in 1:5) == true
    @test (2.5 in 1:5) == false

    # float range
    @test (1.5 in 1.0:0.5:3.0) == true
    @test (1.25 in 1.0:0.5:3.0) == false

    # char range
    @test ('c' in 'a':'e') == true
    @test ('z' in 'a':'e') == false

    # in a predicate / ∈
    @test count(x -> x in 2:4, 1:6) == 3
    @test (3 ∈ 1:5) == true
    @test (3 ∉ 1:5) == false
end

true
