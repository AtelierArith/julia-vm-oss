# Issue #7176: two textually-identical allocating calls (`zeros(n)`, `ones(n)`,
# `fill`, `similar`, `copy`, `collect`) must each return a fresh, independent
# array. They were classified as fully `:consistent`/pure, so CSE merged
# `a = zeros(n); b = zeros(n)` into a single shared allocation — mutating `a`
# also changed `b`. This produced a straight line instead of a Barnsley fern,
# because `xs = zeros(n); ys = zeros(n)` aliased the same buffer.
using Test

function two_zeros(n)
    a = zeros(n)
    b = zeros(n)
    a[1] = 5.0
    return (a[1], b[1])
end

function two_ones()
    a = ones(3)
    b = ones(3)
    a[1] = 7.0
    return b[1]
end

function two_fill()
    a = fill(2.0, 3)
    b = fill(2.0, 3)
    a[1] = 7.0
    return b[1]
end

function two_collect()
    a = collect(1:3)
    b = collect(1:3)
    a[1] = 99
    return b[1]
end

function two_copy()
    src = [1.0, 2.0, 3.0]
    a = copy(src)
    b = copy(src)
    a[1] = 7.0
    return (b[1], src[1])
end

@testset "Issue #7176: allocating calls are not CSE-merged" begin
    a1, b1 = two_zeros(3)
    @test a1 == 5.0
    @test b1 == 0.0            # b must stay untouched
    @test two_ones() == 1.0
    @test two_fill() == 2.0
    @test two_collect() == 1
    cb, cs = two_copy()
    @test cb == 1.0            # copy is independent of the mutated copy
    @test cs == 1.0            # and of the source
end

true
