# Test show(io, x) for basic types

using Test

@testset "show basic types" begin
    # Int64
    buf = IOBuffer()
    show(buf, 42)
    output = take!(buf)
    @test occursin("42", output)

    buf = IOBuffer()
    show(buf, -123)
    output = take!(buf)
    @test occursin("-123", output)

    # Float64
    buf = IOBuffer()
    show(buf, 3.14)
    output = take!(buf)
    @test occursin("3.14", output)

    # String
    buf = IOBuffer()
    show(buf, "hello")
    output = take!(buf)
    @test occursin("hello", output)

    # Symbol
    buf = IOBuffer()
    show(buf, :foo)
    output = take!(buf)
    @test occursin("foo", output)

    # Bool - uses string(x) so output is "true" or "false"
    buf = IOBuffer()
    show(buf, true)
    output = take!(buf)
    @test occursin("true", output)

    buf = IOBuffer()
    show(buf, false)
    output = take!(buf)
    @test occursin("false", output)

    # Nothing
    buf = IOBuffer()
    show(buf, nothing)
    output = take!(buf)
    @test occursin("nothing", output)

    # Char
    buf = IOBuffer()
    show(buf, 'a')
    output = take!(buf)
    @test occursin("a", output)
end

true
