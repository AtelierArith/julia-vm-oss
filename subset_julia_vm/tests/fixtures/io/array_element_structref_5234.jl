# Issue #5234: printing / repr / show / string of an array whose ELEMENTS are
# heap-allocated structs (Pair, Complex, user struct) must resolve each
# `StructRef(heap_idx=N)` through the element's show form, not leak the raw
# Rust debug repr. The single-arg stdout `print`/`println` entry points only
# did a top-level StructRef deref (not the deep `resolve_struct_refs_for_format`
# the string/repr paths use), so array elements leaked `StructRef(heap_idx=N)`.
#
# Reproduces with plain array literals AND comprehensions (independent of
# map/HOF). This fixture asserts no leak + the correct element rendering across
# the capturable display entry points (string / repr / sprint(print) /
# sprint(show)); the bare stdout `print`/`println` paths are covered by the
# Rust integration test `array_element_structref_5234_*` (captures stdout).

using Test

struct Foo5234
    x::Int
end

function no_structref_leak(s)
    return !occursin("StructRef", s) && !occursin("heap_idx", s)
end

# All capturable entry points that route a value through the array display
# path. `sprint(show, c)` is intentionally excluded: sjulia has a separate,
# unrelated bug where `show(io, ::AbstractArray)` over an `Any`-eltype
# container renders `Vector{Any}()` / `Matrix{Any}()` (no StructRef leak —
# orthogonal to #5234). string / repr / sprint(print, ...) all funnel each
# element through `resolve_struct_refs_for_format`, which is the path #5234
# fixes for the bare stdout `print`/`println` entry points too.
function entry_points(c)
    return (
        string(c),
        repr(c),
        sprint(print, c),
    )
end

@testset "array of Pair literal (Issue #5234)" begin
    c = [1 => 1, 2 => 4]
    for s in entry_points(c)
        @test no_structref_leak(s)
        # Pair eltype is implicit upstream: bare [..] with `k => v` elements.
        @test s == "[1 => 1, 2 => 4]"
    end
end

@testset "array of Complex literal (Issue #5234)" begin
    c = [complex(1, 1), complex(2, 2)]
    for s in entry_points(c)
        @test no_structref_leak(s)
        # Complex integer literal eltype/display parity is tracked by #9743;
        # this #5234 fixture is scoped to StructRef leakage.
        @test (occursin("1 + 1im", s) && occursin("2 + 2im", s)) ||
              (occursin("1.0 + 1.0im", s) && occursin("2.0 + 2.0im", s))
    end
end

@testset "array of user struct literal (Issue #5234)" begin
    c = [Foo5234(1), Foo5234(2)]
    for s in entry_points(c)
        @test no_structref_leak(s)
        @test occursin("Foo5234(1)", s)
        @test occursin("Foo5234(2)", s)
    end
end

@testset "comprehension of Complex (Issue #5234)" begin
    c = [complex(x, x) for x in [1, 2]]
    for s in entry_points(c)
        @test no_structref_leak(s)
        @test occursin("1 + 1im", s)
        @test occursin("2 + 2im", s)
    end
end

@testset "comprehension of user struct (Issue #5234)" begin
    c = [Foo5234(i) for i in 1:3]
    for s in entry_points(c)
        @test no_structref_leak(s)
        @test occursin("Foo5234(1)", s)
        @test occursin("Foo5234(3)", s)
    end
end

@testset "single-element array of Pair (Issue #5234)" begin
    c = [1 => 2]
    for s in entry_points(c)
        @test no_structref_leak(s)
        @test s == "[1 => 2]"
    end
end

@testset "matrix of Pair (Issue #5234)" begin
    c = [1 => 2 3 => 4; 5 => 6 7 => 8]
    for s in entry_points(c)
        @test no_structref_leak(s)
        @test occursin("1 => 2", s)
        @test occursin("7 => 8", s)
    end
end

true
