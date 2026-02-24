# Nested field assignment (Issue #2309)
# obj.inner.field = value and obj.inner.field += value

using Test

mutable struct Inner
    value::Int64
end

mutable struct Outer
    inner::Inner
end

mutable struct Deep
    outer::Outer
end

@testset "nested field assignment" begin
    # Basic nested assignment: obj.inner.field = value
    o = Outer(Inner(10))
    @test o.inner.value == 10
    o.inner.value = 20
    @test o.inner.value == 20

    # Compound nested assignment: obj.inner.field += value
    o2 = Outer(Inner(10))
    o2.inner.value += 5
    @test o2.inner.value == 15

    # Compound nested -=
    o3 = Outer(Inner(10))
    o3.inner.value -= 3
    @test o3.inner.value == 7

    # Compound nested *=
    o4 = Outer(Inner(4))
    o4.inner.value *= 3
    @test o4.inner.value == 12

    # Deep nesting (3 levels): obj.outer.inner.field = value
    d = Deep(Outer(Inner(100)))
    @test d.outer.inner.value == 100
    d.outer.inner.value = 200
    @test d.outer.inner.value == 200

    # Deep nesting compound assignment
    d2 = Deep(Outer(Inner(50)))
    d2.outer.inner.value += 25
    @test d2.outer.inner.value == 75

    # Multiple assignments to different nested fields
    o5 = Outer(Inner(0))
    o5.inner.value = 1
    o5.inner.value += 2
    o5.inner.value *= 3
    @test o5.inner.value == 9
end

true
