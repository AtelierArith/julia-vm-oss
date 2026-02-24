# Test show(io, x) for numeric types (Complex, Rational)

using Test

@testset "show numeric types" begin
    # Complex with positive imaginary
    buf = IOBuffer()
    show(buf, 1 + 2im)
    @test take!(buf) == "1 + 2im"

    # Complex with negative imaginary
    buf = IOBuffer()
    show(buf, 3 - 4im)
    @test take!(buf) == "3 - 4im"

    # Rational
    buf = IOBuffer()
    show(buf, 1//2)
    @test take!(buf) == "1//2"

    buf = IOBuffer()
    show(buf, 3//4)
    @test take!(buf) == "3//4"
end

true
