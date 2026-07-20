# Test println(io, ...) writing to IOBuffer (Issue #3573).
# Before the fix, `println(::IOBuffer, ...)` dumped the IO value onto stdout
# because the compiler's `println` form ignored the IO first-arg. This pins
# the IOBuffer round-trip so a regression in the println compile path is
# caught immediately.

using Test

@testset "println(io::IOBuffer, ...) (#3573)" begin
    # Single-arg form: writes "msg\n" to the buffer.
    io = IOBuffer()
    println(io, "msg")
    result = String(take!(io))
    @test result == "msg\n"

    # Empty form: writes just "\n".
    io = IOBuffer()
    println(io)
    result = String(take!(io))
    @test result == "\n"

    # Multiple args concatenate without separators, then newline.
    io = IOBuffer()
    println(io, "a", 1, "b")
    result = String(take!(io))
    @test result == "a1b\n"

    # Successive println calls accumulate in the buffer.
    io = IOBuffer()
    println(io, "first")
    println(io, "second")
    result = String(take!(io))
    @test result == "first\nsecond\n"

    # Mixed print + println: the println still appends a newline; the print
    # does not.
    io = IOBuffer()
    print(io, "a")
    println(io, "b")
    print(io, "c")
    result = String(take!(io))
    @test result == "ab\nc"
end

true
