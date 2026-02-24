# Compound assignment on mutable struct fields (Issue #2140)
# Verifies that obj.field += expr, obj.field -= expr, etc. work correctly.

using Test

mutable struct Counter
    count::Int64
end

mutable struct Vec2
    x::Float64
    y::Float64
end

function increment(c::Counter)
    c.count += 1
    return c.count
end

@testset "compound += on struct field (Issue #2140)" begin
    c = Counter(0)
    c.count += 1
    @test c.count == 1
    c.count += 5
    @test c.count == 6
end

@testset "compound -= on struct field (Issue #2140)" begin
    c = Counter(10)
    c.count -= 3
    @test c.count == 7
end

@testset "compound *= on struct field (Issue #2140)" begin
    c = Counter(5)
    c.count *= 3
    @test c.count == 15
end

@testset "compound /= on struct field (Issue #2140)" begin
    v = Vec2(10.0, 20.0)
    v.x /= 2.0
    @test v.x == 5.0
end

@testset "compound ^= on struct field (Issue #2140)" begin
    v = Vec2(3.0, 0.0)
    v.x ^= 2
    @test v.x == 9.0
end

@testset "compound assignment inside function (Issue #2140)" begin
    c = Counter(0)
    @test increment(c) == 1
    @test increment(c) == 2
    @test c.count == 2
end

true
