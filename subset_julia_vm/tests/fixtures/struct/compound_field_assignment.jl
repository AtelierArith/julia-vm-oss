# Compound assignment on mutable struct fields (Issue #2139)
# obj.field += expr should be lowered to obj.field = obj.field + expr

using Test

mutable struct Counter
    count::Int64
end

mutable struct Accumulator
    total::Float64
    count::Int64
end

@testset "compound field assignment" begin
    # Basic +=
    c = Counter(0)
    c.count += 1
    @test c.count == 1
    c.count += 5
    @test c.count == 6

    # -= operator
    c2 = Counter(10)
    c2.count -= 3
    @test c2.count == 7

    # *= operator
    c3 = Counter(4)
    c3.count *= 3
    @test c3.count == 12

    # Multiple fields
    acc = Accumulator(0.0, 0)
    acc.total += 3.14
    acc.count += 1
    @test acc.total == 3.14
    @test acc.count == 1

    acc.total += 2.86
    acc.count += 1
    @test acc.total == 6.0
    @test acc.count == 2

    # /= operator
    acc2 = Accumulator(10.0, 0)
    acc2.total /= 2.0
    @test acc2.total == 5.0

    # ^= operator
    acc3 = Accumulator(3.0, 0)
    acc3.total ^= 2
    @test acc3.total == 9.0
end

true
