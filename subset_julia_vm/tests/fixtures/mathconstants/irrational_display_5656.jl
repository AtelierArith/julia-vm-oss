using Test

# Issue #5656: Irrational singletons (π, ℯ) must display as their bare symbol
# name (`π`, `ℯ`), not the generic struct-dump constructor form
# `Irrational{:π}()`. Covers every display path: print/string (formatting.rs
# format_struct_instance), show/sprint (pure-Julia show(::AbstractIrrational)),
# repr (pure-Julia repr → generic show fallback), and string interpolation.

@testset "Irrational singleton display (Issue #5656)" begin
    # print / string path
    @test string(π) == "π"
    @test string(ℯ) == "ℯ"

    # repr path
    @test repr(π) == "π"
    @test repr(ℯ) == "ℯ"

    # show via sprint (runtime dispatch)
    @test sprint(show, π) == "π"
    @test sprint(show, ℯ) == "ℯ"

    # show via sprint(print, ...)
    @test sprint(print, π) == "π"

    # string interpolation
    @test "$π" == "π"
    @test "value: $ℯ" == "value: ℯ"

    # runtime-typed (function param, not a const) still routes correctly
    f(z) = repr(z)
    @test f(π) == "π"
    @test f(ℯ) == "ℯ"

    # the symbol display does not break arithmetic / float conversion
    @test Float64(π) ≈ 3.141592653589793
    @test 2π ≈ 6.283185307179586
end

true
