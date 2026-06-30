using Test

# Issue #6892: tuple `==` over mixed BigFloat / Float64 / Int elements returned
# `false` even when the element-wise comparisons were `true`. The tuple `==`
# folds element comparisons through the `TupleEquals` builtin
# (values_equal_tristate); BigFloat-vs-Float64/Int pairs fell through to the
# Debug-string fallback (which compares unequal across representations) instead
# of promoting both operands to BigFloat the way scalar `==` does. Now the
# tristate fold coerces any BigFloat-involving numeric pair to BigFloat and
# compares by value. Verified vs julia 1.12.6.

@testset "tuple == BigFloat vs Float64 (Issue #6892)" begin
    @test (big(2.0), big(1.0)) == (2.0, 1.0)
    @test (big(2.0),) == (2.0,)
    @test (2.0, 1.0) == (big(2.0), big(1.0))
end

@testset "tuple == BigFloat vs Int (Issue #6892)" begin
    @test (big(2.0), big(1.0)) == (2, 1)
    @test (big(3.0),) == (3,)
end

@testset "tuple == BigFloat mixed elements (Issue #6892)" begin
    @test (big(2.0), 1) == (2.0, big(1.0))
    @test (big(2.0), big(1.0)) == (big(2.0), big(1.0))
end

@testset "tuple == BigFloat inequality stays false (Issue #6892)" begin
    @test ((big(2.0),) == (3.0,)) == false
    @test ((big(2.0), big(1.0)) == (2.0, 9.0)) == false
end

@testset "divrem tuple == over BigFloat (Issue #6892 / #6801)" begin
    t = divrem(big(7.0), big(3.0))
    @test t[1] == 2.0 && t[2] == 1.0
    @test t == (2.0, 1.0)
end

@testset "scalar == still agrees (control, Issue #6892)" begin
    @test big(2.0) == 2.0
    @test (1.0, 2.0) == (1, 2)
end

true
