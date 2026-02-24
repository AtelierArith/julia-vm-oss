# Test the difference between print and show behavior
# print: writes without quotes for String/Char
# show: writes with quotes for String/Char (Julia syntax representation)

using Test

@testset "print vs show difference" begin
    # For strings:
    # - show adds quotes (Julia syntax)
    # - print does not add quotes (human-readable)
    @test repr("hello") == "\"hello\""  # show adds quotes

    # For chars:
    # - show adds quotes (Julia syntax)
    # - print does not add quotes (human-readable)
    @test repr('a') == "'a'"  # show adds quotes

    # For numbers, show and print are the same
    @test repr(42) == "42"
    @test repr(3.14) == "3.14"

    # For symbols, show includes colon prefix
    @test repr(:sym) == ":sym"

    # For booleans
    @test repr(true) == "true"
    @test repr(false) == "false"

    # For nothing
    @test repr(nothing) == "nothing"

    # For tuples
    @test repr((1, 2, 3)) == "(1, 2, 3)"
    @test repr((1,)) == "(1,)"  # single-element tuple has trailing comma

    # For pairs
    @test repr(:a => 1) == ":a => 1"

    # For ranges
    @test repr(1:5) == "1:5"
    @test repr(1:2:10) == "1:2:10"
end

# Test that print to IOBuffer works correctly
@testset "print to IOBuffer" begin
    io = IOBuffer()
    print(io, "hello")
    print(io, " ")
    print(io, "world")
    @test take!(io) == "hello world"

    io = IOBuffer()
    print(io, 42)
    @test take!(io) == "42"

    io = IOBuffer()
    print(io, 'a')
    @test take!(io) == "a"  # No quotes for char
end

# Test that show to IOBuffer works correctly
@testset "show to IOBuffer" begin
    io = IOBuffer()
    show(io, "hello")
    @test take!(io) == "\"hello\""  # With quotes

    io = IOBuffer()
    show(io, 'a')
    @test take!(io) == "'a'"  # With quotes

    io = IOBuffer()
    show(io, 42)
    @test take!(io) == "42"  # Same as print for numbers
end

true
