using Test

# Issue #9786: destructuring a NamedTuple binds its field values in declaration
# order (`x, y, z = nt` binds x = nt[1], y = nt[2], z = nt[3]) — a NamedTuple is
# an iterable whose iteration yields its values. This mirrors the REPL LV6-flip
# blocker where a value-carried NamedTuple global destructured across evals hit
# `TupleUnpack` with no `Value::NamedTuple` arm. This fixture pins the
# upstream-`julia` destructure / indexing / first-last semantics in a single
# program (verified against julia 1.12); the cross-eval Persistent regression
# lives in `tests/repl_differential_9199_tests.rs`.

@testset "NamedTuple destructure binds values in field order (Issue #9786)" begin
    nt = (a = 1, b = 2, c = 3)
    x, y, z = nt
    @test (x, y, z) == (1, 2, 3)

    # svd-shaped NamedTuple (the exact MWE shape from Issue #9786).
    F = (U = [1.0, 2.0], S = [3.0, 4.0], V = [5.0, 6.0])
    U, S, V = F
    @test U == [1.0, 2.0]
    @test S == [3.0, 4.0]
    @test V == [5.0, 6.0]
    @test U[1] == 1.0
    @test S[2] == 4.0
end

@testset "NamedTuple destructure arity semantics (Issue #9786)" begin
    # Too few targets: extra fields are ignored (no error), matching a Tuple.
    p, q = (a = 10, b = 20, c = 30)
    @test (p, q) == (10, 20)

    @test_throws BoundsError ((r, s, t, u) = (a = 1, b = 2, c = 3))
end

@testset "NamedTuple nested destructure (Issue #9786)" begin
    n2 = (m = (1, 2), n = 3)
    (p1, p2), q1 = n2
    @test (p1, p2, q1) == (1, 2, 3)
end

@testset "NamedTuple indexing and first/last iterate its values (Issue #9786)" begin
    nt = (p = 10, q = 20, r = 30)
    @test nt[1] == 10
    @test nt[2] == 20
    @test nt[3] == 30
    @test first(nt) == 10
    @test last(nt) == 30
    @test collect(nt) == [10, 20, 30]
end

@testset "NamedTuple iterates and splats its values (Issue #9786)" begin
    nt = (p = 10, q = 20, r = 30)
    # for-loop / comprehension iterate the values in order.
    acc = 0
    for v in nt
        acc += v
    end
    @test acc == 60
    @test [v for v in nt] == [10, 20, 30]
    @test sum(nt) == 60

    # Positional splat yields the field values: f(nt...) == f(nt[1], nt[2], nt[3]).
    add3(x, y, z) = x + y + z
    @test add3(nt...) == 60
end

true
