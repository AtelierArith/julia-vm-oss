# Issue #4776: multi-line tuple/array/call literals with a trailing
# comma — `(1,\n 2,\n 3,\n)` — failed to parse. Single-line trailing
# comma `(1, 2, 3,)` already worked.
#
# Two parser bugs were responsible:
#
# 1. In `parse_parenthesized_or_tuple` and `parse_call_expression`,
#    the trailing-comma RParen check ran BEFORE skipping the
#    newline that followed the trailing comma. The parser then
#    tried to `parse_expression` on the closing paren and failed
#    with "expected expression".
#
# 2. `parse_array_or_comprehension` did not skip newlines after
#    the opening `[` (the tuple entry point already did), so
#    `[\n 1,\n 2,\n]` failed at `[\n`.
#
# 3. None of the three callers (`parse_tuple_rest`,
#    `parse_vector_rest`, `parse_call_expression`) skipped
#    newlines before `expect(RParen)` / `expect(RBracket)`, so
#    even the no-trailing-comma multi-line shape
#    `(\n 1,\n 2,\n 3\n)` failed at `3\n`.
#
# All three fixes ride in the same PR.

using Test

@testset "Tuple multi-line trailing comma (Issue #4776)" begin
    function f()
        return (
            1,
            2,
            3,
        )
    end
    @test f() === (1, 2, 3)
end

@testset "Vector multi-line trailing comma (Issue #4776)" begin
    v = [
        1,
        2,
        3,
    ]
    @test v == [1, 2, 3]
end

@testset "Call multi-line trailing comma (Issue #4776)" begin
    function add3(a, b, c)
        return a + b + c
    end
    @test add3(
        1,
        2,
        3,
    ) == 6
end

@testset "Named tuple multi-line trailing comma (Issue #4776)" begin
    nt = (
        a = 1,
        b = 2,
    )
    @test nt.a == 1
    @test nt.b == 2
end

@testset "Multi-line without trailing comma also works (Issue #4776)" begin
    v = [
        1,
        2,
        3
    ]
    @test v == [1, 2, 3]
end

@testset "Single-element multi-line vector with trailing comma (Issue #4776)" begin
    v = [
        42,
    ]
    @test v == [42]
end

@testset "Single-element multi-line tuple with trailing comma (Issue #4776)" begin
    t = (
        42,
    )
    @test t === (42,)
end

@testset "Single-line trailing comma still works (regression Issue #4776)" begin
    @test (1, 2, 3,) === (1, 2, 3)
    @test [1, 2, 3,] == [1, 2, 3]
end

@testset "Multi-line call with mixed args (Issue #4776)" begin
    function mix(a, b, c)
        return (a, b, c)
    end
    @test mix(
        "x",
        42,
        true,
    ) === ("x", 42, true)
end

true
