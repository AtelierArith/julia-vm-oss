# Test show(io, x) for container types

using Test

@testset "show container types" begin
    # Tuple
    buf = IOBuffer()
    show(buf, (1, 2, 3))
    @test take!(buf) == "(1, 2, 3)"

    # Single-element tuple (with trailing comma)
    buf = IOBuffer()
    show(buf, (42,))
    @test take!(buf) == "(42,)"

    # Empty tuple
    buf = IOBuffer()
    show(buf, ())
    @test take!(buf) == "()"

    # Pair
    buf = IOBuffer()
    show(buf, :a => 1)
    @test take!(buf) == ":a => 1"
end

true
