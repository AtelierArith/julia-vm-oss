# Test show(io, x) for range types

using Test

@testset "show range types" begin
    # UnitRange
    buf = IOBuffer()
    show(buf, 1:10)
    @test take!(buf) == "1:10"

    # StepRange
    buf = IOBuffer()
    show(buf, 1:2:10)
    @test take!(buf) == "1:2:9"
end

true
