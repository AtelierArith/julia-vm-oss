# Symbolics subset: infix pretty-printing via `show`/`string` (Issue #6572).
# Display is "loose" vs. upstream (no canonical ordering, `2*x` not `2x`); the
# fixture pins the subset's own output. `string`/`print`/`show(io, ·)` route
# through the registered user `show` method; the bare REPL / iOS result echo
# struct-dumps user types (tracked separately as Issue #7168).

using Test
using Symbolics

# Render through `show(io, ·)` directly (the path `print` uses), to confirm it
# matches `string`.
function repr_via_show(e)
    io = IOBuffer()
    show(io, e)
    String(take!(io))
end

@testset "Symbolics show: atoms and binary operators" begin
    @variables x y
    @test string(x) == "x"
    @test string(x + y) == "x + y"
    @test string(x - y) == "x - y"
    @test string(x * y) == "x*y"
    @test string(x / y) == "x/y"
    @test string(x^2) == "x^2"
    @test string(2x) == "2*x"
    @test string(-x) == "-x"
end

@testset "Symbolics show: precedence parenthesization" begin
    @variables x y
    @test string(x^2 + 2x + 1) == "x^2 + 2*x + 1"
    @test string(x * (y + 1)) == "x*(y + 1)"
    @test string((x + y) * 2) == "(x + y)*2"
    @test string(x + y * 2) == "x + y*2"          # no parens: * binds tighter
    @test string(x - (y - x)) == "x - (y - x)"    # right operand of - needs parens
    @test string(x^(y + 1)) == "x^(y + 1)"        # sum under ^ needs parens
end

@testset "Symbolics show: function applications" begin
    @variables x y
    @test string(sin(x)) == "sin(x)"
    @test string(cos(x) + sin(y)) == "cos(x) + sin(y)"
    @test string(exp(x * y)) == "exp(x*y)"
    @test string(sqrt(x + 1)) == "sqrt(x + 1)"
end

@testset "Symbolics show: show into IOBuffer agrees with string" begin
    @variables x
    e = x^2 + 1
    @test repr_via_show(e) == string(e)
end

true
