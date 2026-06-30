using Test

# Tuple-literal positional splat (Issue #7741).
# A tuple literal containing a splatted operand, e.g. `(A, B, xs...)`, must
# splice the splatted iterable's elements into the parent tuple instead of
# nesting it as a single element, matching upstream Julia's `Core.tuple(...)` /
# `Core._apply_iterate` lowering. The bug was first observed inside a struct
# constructor body, where `(A, B, xs...)` produced `(2, 2, (1, 2, 3, 4))`.

struct TupleSplatCtor7741{A,B,T}
    data::Tuple
end

function TupleSplatCtor7741{A,B}(xs...) where {A,B}
    return TupleSplatCtor7741{A,B,typeof(xs[1])}((A, B, xs...))
end

struct WrapTuple7741
    data::Tuple
end

@testset "tuple literal splat (#7741)" begin
    # The original constructor-body MWE: `xs...` must splice into the tuple.
    x = TupleSplatCtor7741{2,2}(1, 2, 3, 4)
    @test x.data == (2, 2, 1, 2, 3, 4)
    @test typeof(x) == TupleSplatCtor7741{2,2,Int64}

    # varargs splat inside a plain function body
    f(xs...) = (10, 20, xs...)
    @test f(1, 2, 3, 4) == (10, 20, 1, 2, 3, 4)
    @test f() == (10, 20)

    # splat of a local tuple variable
    g() = begin
        ys = (1, 2, 3)
        (10, 20, ys...)
    end
    @test g() == (10, 20, 1, 2, 3)

    # top-level splat of a tuple variable
    ys = (1, 2, 3)
    @test (10, 20, ys...) == (10, 20, 1, 2, 3)

    # leading splat
    zs = (7, 8)
    @test (zs..., 9) == (7, 8, 9)

    # splat in the middle
    @test (1, ys..., 99) == (1, 1, 2, 3, 99)

    # single-element splat with trailing comma
    one = (9,)
    @test (one...,) == (9,)

    # splat of a range
    @test (1, (2:4)..., 5) == (1, 2, 3, 4, 5)

    # splat of an array inside a tuple literal
    @test (0, [7, 8]..., 9) == (0, 7, 8, 9)

    # empty (zero-length) splat
    empty = ()
    @test (1, empty..., 2) == (1, 2)

    # multiple splats
    @test (zs..., ys...) == (7, 8, 1, 2, 3)

    # nested tuple literal containing a splat element
    @test ((ys...,), ys...) == ((1, 2, 3), 1, 2, 3)

    # splat of a varargs param spread into a struct `::Tuple` field via `tuple`
    mk(xs...) = WrapTuple7741((1, 2, xs...))
    @test mk(3, 4, 5).data == (1, 2, 3, 4, 5)

    # plain (no-splat) tuples and named tuples are unaffected
    @test (1, 2, 3) == (1, 2, 3)
    @test (a = 1, b = 2) == (a = 1, b = 2)
end

true
