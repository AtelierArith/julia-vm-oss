# TypeError from failed type assertions: expected/got fields + upstream message
# parity (Issue #5146).
#
# Upstream (julia/base/boot.jl) stores the offending VALUE in `TypeError.got`
# (not its type), and formats the message in julia/base/errorshow.jl as
#   "TypeError: in typeassert, expected String, got a value of type Int64"
# via `"a value of type $(typeof(ex.got))"`. Previously sjulia stored
# `typeof(x)` in `got` and printed the type directly, diverging from upstream.

using Test

# `expr::T` outside a declaration lowers to `typeassert(expr, T)`.
assert_string(x) = x::String

@testset "TypeError expected/got and message parity (Issue #5146)" begin
    # typeassert form
    @test_throws TypeError typeassert(2, String)

    # `expr::T` form (lowers to typeassert) also throws TypeError
    @test_throws TypeError assert_string(1)

    # got holds the VALUE, not its type
    err = try
        typeassert(2, String)
        nothing
    catch e
        e
    end
    @test err isa TypeError
    @test err.func === :typeassert
    @test err.expected === String
    @test err.got === 2                 # value, not Int64
    @test err.got isa Int64

    # exact upstream message string
    @test sprint(showerror, err) ==
        "TypeError: in typeassert, expected String, got a value of type Int64"

    # `expr::T` form produces the same message
    err2 = try
        assert_string(1)
        nothing
    catch e
        e
    end
    @test err2 isa TypeError
    @test err2.got === 1
    @test sprint(showerror, err2) ==
        "TypeError: in typeassert, expected String, got a value of type Int64"
end

true
