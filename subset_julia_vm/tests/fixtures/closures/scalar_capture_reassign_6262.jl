# Issue #6262: a closure capturing a scalar local observed a stale value after
# the local was reassigned in the enclosing scope. sjulia captured by value
# snapshot (`CreateClosure`) instead of Julia's `Core.Box` cell semantics, so
# `counter = 0; f = () -> counter; counter = 5; f()` returned `0`.
#
# Fix: a post-lowering pass boxes a local as `Ref` when it is captured by a
# closure AND reassigned at least twice at its scope's top level, rewriting the
# binding to `v = Ref(init)`, reads to `v[]`, and reassignments to `v[] = x` in
# both the defining scope and the capturing closure. sjulia's `Ref` is already
# reference-semantic on capture, so all references then share one cell.
#
# Single-assignment captures stay unboxed (and keep working unchanged).

using Test

function reads_after_reassign()
    counter = 0
    get_counter = () -> counter
    a = get_counter()
    counter = 5
    b = get_counter()
    (a, b)
end

@testset "closure observes reassignment of captured scalar (Issue #6262)" begin
    @test reads_after_reassign() == (0, 5)
end

function multi_reassign()
    x = 1
    f = () -> x
    x = 2
    x = 3
    f()
end

@testset "closure observes the latest of several reassignments (Issue #6262)" begin
    @test multi_reassign() == 3
end

function two_closures_share()
    v = 10
    g1 = () -> v
    g2 = () -> v + 1
    v = 100
    (g1(), g2())
end

@testset "two closures share one boxed cell (Issue #6262)" begin
    @test two_closures_share() == (100, 101)
end

function single_capture()
    y = 7
    h = () -> y * 2
    h()
end

@testset "single-assignment capture stays correct (regression, Issue #6262)" begin
    @test single_capture() == 14
end

function nested_reads()
    n = 0
    outer = () -> (() -> n)()
    n = 42
    outer()
end

@testset "nested closure reads the boxed cell (Issue #6262)" begin
    @test nested_reads() == 42
end

function box_value_string()
    s = "a"
    f = () -> s
    s = "b"
    f()
end

@testset "boxed cell preserves a String value (Issue #6262)" begin
    @test box_value_string() == "b"
end

true
