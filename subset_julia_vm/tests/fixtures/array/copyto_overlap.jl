using Test

# Regression test for Issue #3595:
# `copyto!(dest, dstart, src, sstart, n)` (and the related 2/3/4-arg variants)
# must handle overlapping source/destination ranges like memmove. Forward-only
# iteration corrupts data when dest === src && dstart > sstart.

@testset "copyto! overlap (#3595)" begin
    # MWE from Issue: forward-overlap on the same array
    a = [1, 2, 3, 4]
    copyto!(a, 2, a, 1, 3)
    @test a == [1, 1, 2, 3]

    # Reverse-overlap on the same array (forward iteration is correct here)
    b = [1, 2, 3, 4]
    copyto!(b, 1, b, 2, 3)
    @test b == [2, 3, 4, 4]

    # Full self-copy (dstart == sstart) — no-op
    c = [1, 2, 3, 4]
    copyto!(c, 1, c, 1, 4)
    @test c == [1, 2, 3, 4]

    # Non-overlapping copy between distinct arrays — unchanged behavior
    d1 = [1, 2, 3]
    d2 = [10, 20, 30]
    copyto!(d1, 1, d2, 1, 3)
    @test d1 == [10, 20, 30]

    # 3-arg form: copyto!(dest, dstart, src) with non-overlapping arrays
    f = [1, 2, 3, 4]
    copyto!(f, 2, [10, 20])
    @test f == [1, 10, 20, 4]

    # 2-arg form: copyto!(dest, src) with distinct arrays
    g = [0, 0, 0]
    copyto!(g, [10, 20, 30])
    @test g == [10, 20, 30]

    # 4-arg with reverse overlap on same array
    h = [10, 20, 30, 40, 50]
    copyto!(h, 1, h, 3)        # copy h[3:end] to h[1:end-2]
    @test h == [30, 40, 50, 40, 50]

    # 3-arg dstart=1, dest === src — no-op (length mismatch caveat: dest must fit src)
    k = [1, 2, 3]
    copyto!(k, 1, k)
    @test k == [1, 2, 3]
end

true
