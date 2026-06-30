using Test

# Issue #7343 (follow-up to #7338): quoting a `for` loop with multiple comma-
# separated bindings (`for a = …, b = …`) previously failed with the lowering error
# "quote of for: multiple bindings not yet supported". Upstream Julia represents such
# a loop as `Expr(:for, Expr(:block, :(a=…), :(b=…)), body)`, so the quote→constructor
# path must produce that block-of-bindings form. A single binding stays inline as
# `Expr(:for, :(a=…), body)`. This is a generic `quote` feature; it also unblocks
# `Interact.@manipulate for a = …, b = …` (multi-control manipulate, #7344).

@testset "quote of single-binding for keeps an inline binding" begin
    e = :(for a = 1:3
        a
    end)
    @test e.head == :for
    @test e.args[1].head == :(=)
    @test e.args[1].args[1] == :a
end

@testset "quote of multi-binding for wraps bindings in a block" begin
    e = :(for a = 1:2, b = 1:3
        a + b
    end)
    @test e.head == :for
    # args[1] is `Expr(:block, :(a=1:2), :(b=1:3))`.
    @test e.args[1].head == :block
    @test length(e.args[1].args) == 2
    @test e.args[1].args[1].head == :(=)
    @test e.args[1].args[1].args[1] == :a
    @test e.args[1].args[2].head == :(=)
    @test e.args[1].args[2].args[1] == :b
    # The body block is the third positional element of the for-Expr.
    @test e.args[2].head == :block
end

@testset "quote of three-binding for wraps all three" begin
    e = :(for i = 1:2, j = 1:2, k = 1:2
        i
    end)
    @test e.args[1].head == :block
    @test length(e.args[1].args) == 3
    @test [b.args[1] for b in e.args[1].args] == [:i, :j, :k]
end

true
