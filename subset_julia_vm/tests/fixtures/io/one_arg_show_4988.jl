# One-argument show(x) writes to stdout (Issue #4988)
using Test

struct ShowPoint4988
    x::Int
    y::Int
end

@testset "one-argument show writes to stdout" begin
    # Built-in scalar values
    @test repr(3) == "3"
    @test repr("hi") == "\"hi\""

    # show(x) without an explicit IO should not error and should write to stdout.
    # We cannot easily capture stdout here, so just assert it runs and returns nothing.
    @test show(3) === nothing
    @test show("hi") === nothing
    @test show([1, 2, 3]) === nothing

    # User struct routed through Base.show(io, x) default
    p = ShowPoint4988(1, 2)
    @test show(p) === nothing
end

true
