using Test

# Issue #5701: accumulate(op, A; init=x) seeds the accumulation via the `init`
# keyword (only the positional 3-arg form existed, and type-specific fast paths
# shadowed it without accepting the keyword).

@testset "accumulate with init keyword (Issue #5701)" begin
    @test accumulate(+, [1, 2, 3]; init=10) == [11, 13, 16]
    @test accumulate(*, [1, 2, 3, 4]; init=2) == [2, 4, 12, 48]
    @test accumulate(max, [1, 3, 2, 5]; init=0) == [1, 3, 3, 5]
    @test accumulate(+, 1:3; init=10) == [11, 13, 16]
    @test accumulate(+, [1.0, 2.0, 3.0]; init=0.5) == [1.5, 3.5, 6.5]

    # No init: unchanged (incl. the type-specific fast paths).
    @test accumulate(+, [1, 2, 3]) == [1, 3, 6]
    @test accumulate(*, [1, 2, 3, 4]) == [1, 2, 6, 24]
    @test accumulate(+, Float64[1, 2, 3]) == [1.0, 3.0, 6.0]
end

true
