# Issue #7614: `getfield`/`getproperty` on an `Expr` value must resolve the
# `head`/`args` fields, matching upstream Julia.
#
# The `.head`/`.args` property syntax is compile-time special-cased to a
# dedicated `GetExprField` instruction, but explicit `getfield(ex, :head)` and
# `getproperty(ex, :head)` calls (which a macro helper hits when the receiver is
# carried through an `Any`-typed parameter, e.g. `MacroTools.splitdef`) routed
# to the generic reflection `getfield`, which rejected `Expr`.

using Test

@testset "getfield/getproperty on Expr (Issue #7614)" begin
    ex = :(x + 1)

    # Field access by Symbol name.
    @test getfield(ex, :head) == :call
    @test getfield(ex, :args) == Any[:+, :x, 1]

    # Field access by 1-based integer index (1 => head, 2 => args).
    @test getfield(ex, 1) == :call
    @test getfield(ex, 2) == Any[:+, :x, 1]

    # getproperty mirrors getfield for the default (no custom getproperty) case.
    @test getproperty(ex, :head) == :call
    @test getproperty(ex, :args) == Any[:+, :x, 1]

    # The same access through an `Any`-typed parameter (the actual failing
    # scenario inside MacroTools helpers).
    head_of(e) = getfield(e, :head)
    args_of(e) = getfield(e, :args)
    @test head_of(ex) == :call
    @test args_of(ex) == Any[:+, :x, 1]

    # `args` returns the shared backing array (reference identity), so mutating
    # it through `getfield` updates the owning Expr — matching upstream's
    # `args::Array{Any,1}` reference semantics.
    @test getfield(ex, :args) === ex.args
    @test getfield(ex, 2) === ex.args
end

true
