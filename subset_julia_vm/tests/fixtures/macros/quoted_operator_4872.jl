# Issue #4872: a bare operator inside a quoted expression (`:(%)`,
# `:(+)`, `:(*)`, etc., or an operator used as a value like
# `:(foo(%, x))`) was rejected during lowering with
# `UnsupportedExpression("quote for operator not yet supported")`.
# Upstream Julia treats a quoted operator as a `Symbol`, identical to
# a quoted identifier.
#
# Fix: in `subset_julia_vm/src/lowering/expr/quote/cst_to_constructor.rs`,
# add a top-level `NodeKind::Operator` arm that mirrors the existing
# `NodeKind::Identifier` arm — wrap the operator's text in
# `BuiltinOp::SymbolNew`, producing `Symbol(text)`.

using Test

@testset "bare quoted operators become Symbols (Issue #4872)" begin
    # Each `:(<op>)` should equal the directly-written `:<op>` Symbol.
    @test :(%) == :%
    @test :(+) == :+
    @test :(*) == :*
    @test :(/) == :/
    @test :(-) == :-
    @test :(==) == :(==)
    @test :(<) == :<
    @test :(>) == :>
    @test :(<=) == :<=
    @test :(>=) == :>=
end

@testset "quoted operators have Symbol type (Issue #4872)" begin
    @test :(%) isa Symbol
    @test :(+) isa Symbol
    @test :(*) isa Symbol
    @test :(==) isa Symbol
end

@testset "quoted operator equals the upstream shorthand (Issue #4872)" begin
    # `:%`, `:+`, etc. are the canonical Symbol forms; `:(%)` should
    # quote to the same value.
    @test :(%) === :%
    @test :(+) === :+
    @test :(*) === :*
    @test :(/) === :/
end

@testset "quoted-identifier path stays intact (regression guard)" begin
    # The fix mirrors the Identifier arm; pin the original behavior so
    # the new Operator arm doesn't shadow or regress it.
    @test :(foo) == :foo
    @test :(bar) === :bar
    @test :(some_name) === :some_name
end

true
