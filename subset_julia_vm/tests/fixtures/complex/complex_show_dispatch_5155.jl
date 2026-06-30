# Issue #5155: Complex display/formatting delegated to Base.show.
# Verifies string / print / repr / show all dispatch to the pure-Julia
# `Base.show(io, ::Complex)` and match upstream Julia exactly, including
# the `Complex{Bool}` / imaginary-unit special cases.
using Test

# Helpers: capture `show(io, x)` / `print(io, x)` output into a String.
# (sprint(f, x) does not invoke f in the subset VM yet, so go via IOBuffer.)
function showstr(x)
    io = IOBuffer()
    show(io, x)
    return String(take!(io))
end

function printstr(x)
    io = IOBuffer()
    print(io, x)
    return String(take!(io))
end

@testset "Complex show dispatch (Issue #5155)" begin
    # Int64 complex
    @test string(1 + 2im) == "1 + 2im"
    @test string(1 - 2im) == "1 - 2im"
    @test string(0 + 0im) == "0 + 0im"

    # Float64 complex - must keep ".0"
    @test string(Complex{Float64}(1.0, 2.0)) == "1.0 + 2.0im"
    @test string(Complex{Float64}(1.0, -2.0)) == "1.0 - 2.0im"
    @test string(3.0 + 0.0im) == "3.0 + 0.0im"

    # Float32 complex
    @test string(Complex{Float32}(1.5f0, -2.5f0)) == "1.5f0 - 2.5f0im"

    # Imaginary unit and Complex{Bool} special cases (upstream parity)
    @test string(im) == "im"
    @test string(Complex{Bool}(false, true)) == "im"
    @test string(Complex{Bool}(true, false)) == "Complex(true,false)"
    @test string(Complex{Bool}(true, true)) == "Complex(true,true)"
    @test string(Complex{Bool}(false, false)) == "Complex(false,false)"

    # repr must agree with string for these scalars
    @test repr(1 + 2im) == "1 + 2im"
    @test repr(Complex{Float64}(1.0, -2.0)) == "1.0 - 2.0im"
    @test repr(im) == "im"
    @test repr(Complex{Bool}(true, false)) == "Complex(true,false)"

    # print(io, x) writes the same text as string
    @test printstr(1 + 2im) == "1 + 2im"
    @test printstr(Complex{Float64}(1.0, -2.0)) == "1.0 - 2.0im"
    @test printstr(im) == "im"

    # show(io, x) writes the same text as repr
    @test showstr(1 + 2im) == "1 + 2im"
    @test showstr(Complex{Float64}(1.0, -2.0)) == "1.0 - 2.0im"
    @test showstr(im) == "im"
    @test showstr(Complex{Bool}(true, false)) == "Complex(true,false)"
end

true
