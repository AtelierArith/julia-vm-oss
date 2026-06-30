using Test

# Issue #5205: narrow-integer arithmetic (Int8/Int16/Int32 and unsigned) must
# wrap (modular), matching upstream Julia, in every path: flattened chains,
# parenthesized chains, and runtime dynamic dispatch inside untyped functions
# (the path vararg `map(+, ...)` exercises). The fix must NOT regress #5192:
# explicit `convert(T, x)` to a narrow integer still throws InexactError when
# the value is out of range.

@testset "narrow-int arithmetic wraps modularly (#5205)" begin
    a = Int8(2)
    b = Int8(20)
    c = Int8(110)

    # Flattened chain: a + b + c lowers to +(a, b, c).
    chained = a + b + c
    @test chained == Int8(-124)
    @test typeof(chained) == Int8

    # Parenthesized chain reaches the same result.
    parenthesized = (a + b) + c
    @test parenthesized == Int8(-124)
    @test typeof(parenthesized) == Int8

    # Same single overflowing add wraps and preserves Int8.
    one_step = Int8(22) + Int8(110)
    @test one_step == Int8(-124)
    @test typeof(one_step) == Int8

    # Runtime dynamic dispatch: untyped params force Int8 + Int8 through the
    # dynamic binary-both fallback. It must still wrap and keep Int8.
    function add_three(x, y, z)
        v = x + y
        v = v + z
        return v
    end
    dyn = add_three(a, b, c)
    @test dyn == Int8(-124)
    @test typeof(dyn) == Int8

    # Subtraction and multiplication wrap too.
    @test Int8(-100) - Int8(100) == Int8(56)
    @test typeof(Int8(-100) - Int8(100)) == Int8
    @test Int8(100) * Int8(2) == Int8(-56)
    @test typeof(Int8(100) * Int8(2)) == Int8

    # Unsigned narrow ints wrap, preserving the unsigned type.
    @test UInt8(200) + UInt8(100) == UInt8(44)
    @test typeof(UInt8(200) + UInt8(100)) == UInt8
    @test UInt8(16) * UInt8(16) == UInt8(0)
    @test typeof(UInt8(16) * UInt8(16)) == UInt8

    # Int16 wraps.
    @test Int16(30000) + Int16(30000) == Int16(-5536)
    @test typeof(Int16(30000) + Int16(30000)) == Int16

    # Callable operator values take the runtime intrinsic fold. Same-type narrow
    # inputs still wrap and preserve their narrow type (Issue #6512).
    plus_value = +
    callable_sum = plus_value(Int8(1), Int8(10), Int8(100))
    @test callable_sum == Int8(111)
    @test typeof(callable_sum) == Int8

    times_value = *
    callable_product = times_value(Int8(100), Int8(2), Int8(2))
    @test callable_product == Int8(-112)
    @test typeof(callable_product) == Int8

    # vararg map over Int8 wraps element-wise (the original #5205 repro).
    mapped = map(+, Int8[1, 2], Int8[10, 20], Int8[100, 110])
    @test mapped == Int8[111, -124]
    @test typeof(mapped) == Vector{Int8}
    @test eltype(mapped) == Int8

    mapped_product = map(*, UInt8[16, 9], UInt8[16, 30], UInt8[2, 2])
    @test mapped_product == UInt8[0, 28]
    @test typeof(mapped_product) == Vector{UInt8}
    @test eltype(mapped_product) == UInt8
end

# Issue #6597: re-evaluation of the #6512 `::Function` carve-out. After PR #6524
# removed the legacy exact-name carve-out and routed `::Function` matching
# through the shared subtype engine, the f6adade84 (#6529) guard keeps empty
# narrow-int / Bool reductions on the type-specialized Base method instead of
# the broad `reduce(op::Function, itr)` catch-all. This block pins the three
# carve-out re-evaluation cases together: (a) direct callable operators,
# (b) `map(op, ...)`, and CRITICALLY (c) empty narrow-int / Bool reductions
# (the #6528 regression), which must NOT throw "reducing over an empty
# collection".
@testset "::Function carve-out re-evaluation: empty narrow-int/Bool reductions (#6597)" begin
    # (a) direct callable operators still wrap and preserve the narrow type.
    @test (+)(Int8(1), Int8(10), Int8(100)) == Int8(111)
    @test typeof((*)(Int8(100), Int8(2), Int8(2))) == Int8

    # (b) map(op, ...) over narrow ints wraps element-wise.
    @test map(+, Int8[1, 2], Int8[10, 20]) == Int8[11, 22]

    # (c) empty narrow-int / Bool reduce + mapreduce must not throw and keep the
    # element type (Int8 → Int8, Bool → Int64), matching upstream julia 1.12.
    @test reduce(+, Int8[]) == Int8(0)
    @test typeof(reduce(+, Int8[])) == Int8
    @test reduce(+, UInt8[]) == UInt8(0)
    @test typeof(reduce(+, UInt8[])) == UInt8
    @test reduce(+, Bool[]) == 0
    @test typeof(reduce(+, Bool[])) == Int64

    @test mapreduce(identity, +, Int8[]) == Int8(0)
    @test typeof(mapreduce(identity, +, Int8[])) == Int8
    @test mapreduce(identity, +, Bool[]) == 0
    @test typeof(mapreduce(identity, +, Bool[])) == Int64
end

@testset "explicit convert to narrow int still throws (#5192 preserved)" begin
    @test_throws InexactError convert(Int8, 300)
    @test_throws InexactError convert(Int8, 132)
    @test_throws InexactError convert(Int8, 128)
    @test_throws InexactError convert(UInt8, 300)
    @test_throws InexactError Int8(300)
    @test_throws InexactError Int8(132)

    # In-range conversions still succeed.
    @test convert(Int8, 100) == Int8(100)
    @test typeof(convert(Int8, 100)) == Int8
end

true
