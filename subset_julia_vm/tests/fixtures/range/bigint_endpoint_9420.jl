using Test

# Issue #9420: a BigInt endpoint must promote the range element type to BigInt
# (upstream `(:)(start, stop)` promotes both endpoints, and
# `promote_type(BigInt, <:Integer) == BigInt`), so `1:big(3)` is a
# `UnitRange{BigInt}` whose eltype is `BigInt` and whose elements materialize
# as `BigInt` — instead of silently falling back to `UnitRange{Int64}`.
#
# Also covered: typed narrow-int unit ranges index at their element type
# (`(UInt8(1):UInt8(3))[2] isa UInt8`, upstream `getindex(::UnitRange{T})`);
# the previous VM fast path narrowed every integer unit-range element to Int64.
@testset "range/BigInt endpoint promotion (Issue #9420)" begin
    r = 1:big(3)
    @test eltype(r) == BigInt
    @test typeof(r) == UnitRange{BigInt}
    @test eltype(1:big(3)) == BigInt
    @test typeof(1:big(3)) == UnitRange{BigInt}
    @test typeof(big(1):3) == UnitRange{BigInt}

    # Elements materialize as BigInt: first/last, indexing, iteration.
    @test first(r) isa BigInt
    @test last(r) isa BigInt
    @test r[2] isa BigInt
    @test r[2] == 2
    @test all(i -> i isa BigInt, r)

    # Iteration in a for head (both the variable and the literal-range form).
    s = 0
    for i in r
        s += i
    end
    @test s == 6
    @test s isa BigInt
    t = 0
    for i in 1:big(4)
        t += i
    end
    @test t == 10
    @test t isa BigInt

    # Aggregates and membership.
    @test length(r) == 3
    @test sum(r) == 6
    @test big(2) in r
    @test !(big(4) in r)

    # collect materializes BigInt elements. The container is Vector{BigInt}
    # upstream; sjulia has no dedicated BigInt array storage yet (Issue #9517),
    # so only the element values/types are asserted here.
    c = collect(r)
    @test c == [1, 2, 3]
    @test all(x -> x isa BigInt, c)

    # Explicit-step form: a BigInt operand promotes the element type.
    @test eltype(big(1):2:big(9)) == BigInt
    @test collect(big(1):2:big(9)) == [1, 3, 5, 7, 9]

    # Typed narrow-int unit ranges index at their element type (Issue #9420
    # generalized the VM's unit-range Int64 narrowing to tag-aware indexing).
    @test (UInt8(1):UInt8(3))[2] isa UInt8
    @test (Int8(1):Int8(3))[2] isa Int8
    # Plain Int ranges are untouched.
    @test (1:5)[2] === 2
end

true
