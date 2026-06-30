# Prevention matrix for the CartesianIndex-style show(io, ::T) mis-dispatch
# family (Issue #4739, follow-up to #4737). Each container / wrapper / index
# type must route show(io, x) to its own method instead of falling through to
# an unrelated AbstractUser-typed arm (e.g. show(io, ::CartesianIndex)) whose
# body would getfield() into the wrong shape and crash, or to the generic
# struct fallback which would print a wrong `T()` form.
#
# Every assertion below was verified against upstream Julia 1.12 first.
using Test

@testset "empty tuple shows as () not Tuple{}() (Issue #4739)" begin
    # Regression for the empty-tuple mis-dispatch: typeof(()) === Tuple{} must
    # stay in the Tuple family so show(io, ()) hits show(io, ::Tuple), not the
    # generic struct fallback (which printed "Tuple{}()").
    @test repr(()) == "()"
    @test string(()) == "()"
    io = IOBuffer()
    show(io, ())
    @test String(take!(io)) == "()"
    # nested empty tuples must also round-trip
    @test repr((1, (), 3)) == "(1, (), 3)"
    @test repr(((),)) == "((),)"
end

@testset "tuple show coverage (Issue #4739)" begin
    @test repr((1,)) == "(1,)"
    @test repr((1, 2, 3)) == "(1, 2, 3)"
    @test repr((1, (2, 3), "a")) == "(1, (2, 3), \"a\")"
end

@testset "named tuple show coverage (Issue #4739)" begin
    @test repr((x = 1, y = 2)) == "(x = 1, y = 2)"
    @test repr((x = 1,)) == "(x = 1,)"
    @test repr((a = (1, 2), b = "s")) == "(a = (1, 2), b = \"s\")"
end

@testset "pair show coverage (Issue #4739)" begin
    @test repr(1 => 2) == "1 => 2"
    @test repr("a" => [1, 2]) == "\"a\" => [1, 2]"
    @test repr(:x => (1, 2)) == ":x => (1, 2)"
end

@testset "range show coverage (Issue #4739)" begin
    @test repr(1:5) == "1:5"
    @test repr(1:2:9) == "1:2:9"
    @test repr(0.0:0.5:2.0) == "0.0:0.5:2.0"
end

@testset "dict / set show coverage (Issue #4739)" begin
    # single entry / element keeps the output order-deterministic
    @test repr(Dict(1 => "a")) == "Dict(1 => \"a\")"
    @test repr(Set([1])) == "Set([1])"
end

@testset "cartesian index show coverage (Issue #4739)" begin
    @test repr(CartesianIndex((2, 3))) == "CartesianIndex(2, 3)"
    @test repr(CartesianIndices((2, 2))) == "CartesianIndices((2, 2))"
end

@testset "arrays of containers show coverage (Issue #4739)" begin
    @test repr([(1, 2), (3, 4)]) == "[(1, 2), (3, 4)]"
    @test repr([1 => 2, 3 => 4]) == "[1 => 2, 3 => 4]"
end

@testset "complex / rational show coverage (Issue #4739)" begin
    @test repr(1 + 2im) == "1 + 2im"
    @test repr(3 // 4) == "3//4"
end

@testset "show(io, ::AbstractDict) does not mis-dispatch to CartesianIndex (Issue #4739)" begin
    # The original #4737 crash: show(io, ::Dict) picked up
    # show(io, ::CartesianIndex) and its getfield(d, 1) body crashed.
    io = IOBuffer()
    show(io, Dict(1 => 2))
    s = String(take!(io))
    @test occursin("Dict", s)
    @test !occursin("CartesianIndex", s)
end

true
