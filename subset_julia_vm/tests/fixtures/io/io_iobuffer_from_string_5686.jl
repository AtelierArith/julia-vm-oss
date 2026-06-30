using Test

# Issue #5686: IOBuffer(s::AbstractString) constructs a readable buffer holding `s`.
# Previously `IOBuffer("...")` errored ("IOBuffer() takes no arguments"). The
# content is read back via read(io, String) or String(take!(io)).

@testset "IOBuffer(string) readable buffer (Issue #5686)" begin
    io = IOBuffer("hello world")
    @test read(io, String) == "hello world"

    io2 = IOBuffer("abc")
    @test String(take!(io2)) == "abc"

    @test read(IOBuffer(""), String) == ""

    @test read(IOBuffer("multi\nline"), String) == "multi\nline"

    # The empty (writable) IOBuffer() form is unaffected.
    io3 = IOBuffer()
    write(io3, "xyz")
    @test String(take!(io3)) == "xyz"

    # Works inside a function body (Any-typed argument).
    function rd(s)
        b = IOBuffer(s)
        return read(b, String)
    end
    @test rd("data") == "data"

    @test IOBuffer("z") isa IO
end

true
