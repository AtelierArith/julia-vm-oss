# Symbolics subset: `Matrix{Num}` / `Vector{Num}` display routes each element
# through the registered scalar `Base.show(io, ::Num)` rather than the generic
# struct-field dump (Issue #7893).
#
# Before the fix, `println([x y; x x])` produced
# `Symbolics.Num[Num(Sym(:x)) Num(Sym(:y)); ...]` (the struct debug repr) because
# the array-display path never invoked the per-element user `show`. Now every
# textual array path (`print`/`println`/`string`/`repr`/`show(io, ·)`) renders
# the elements symbolically (`… [x y; x x]`).
#
# The assertions match the element rendering via `endswith(…, "[x y; x x]")` so
# they pass under BOTH upstream julia (`Num[x y; x x]`) and sjulia, whose eltype
# prefix is currently the fully-qualified `Symbolics.Num[` — a secondary
# alias/import display detail tracked separately. The primary guarantee here is
# the symbolic element rendering (no `Num(Sym(...))` struct dump).

using Test
using Symbolics

# Render through `show(io, ·)` directly (the path `repr` uses).
function repr_via_show(e)
    io = IOBuffer()
    show(io, e)
    String(take!(io))
end

@testset "Symbolics Matrix{Num} display renders elements symbolically" begin
    @variables x y
    A = [x y; x x]
    # Elements are the symbols `x`/`y`, never the `Num(Sym(:x))` struct dump.
    @test endswith(string(A), "[x y; x x]")
    @test endswith(repr(A), "[x y; x x]")
    @test endswith(repr_via_show(A), "[x y; x x]")
    # No struct-field dump leaks into any path.
    @test !occursin("Sym(", string(A))
    @test !occursin("Num(", repr(A))
end

@testset "Symbolics Vector{Num} display renders elements symbolically" begin
    @variables x y
    v = [x, y, x]
    @test endswith(string(v), "[x, y, x]")
    @test endswith(repr(v), "[x, y, x]")
    @test endswith(repr_via_show(v), "[x, y, x]")
    @test !occursin("Sym(", string(v))
end

@testset "Symbolics array elements keep their infix rendering" begin
    @variables x y
    B = [x * x - y * x, y]
    s = string(B)
    @test endswith(s, "[x*x - y*x, y]")
    @test occursin("x*x - y*x", s)
end

@testset "Numeric array display is unchanged" begin
    # The fix must not perturb arrays whose elements have no user `show`.
    @test string([1, 2, 3]) == "[1, 2, 3]"
    @test string([1 2; 3 4]) == "[1 2; 3 4]"
    @test string([1.0 2.0; 3.0 4.0]) == "[1.0 2.0; 3.0 4.0]"
    @test string(["a", "b"]) == "[\"a\", \"b\"]"
    @test string([:x, :y]) == "[:x, :y]"
    @test repr([1, 2, 3]) == "[1, 2, 3]"
end

true
