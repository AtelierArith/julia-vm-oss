using Test

@testset "String/Char multiplication Union vararg dispatch" begin
    @test hasmethod(*, Tuple{String, Char})
    @test hasmethod(*, Tuple{String, Char, String})
    @test hasmethod(*, Tuple{Char, Char, String, Char})

    @test (*)("a", 'b', "c") == "abc"
    @test (*)('a', 'b', "cd", 'e') == "abcde"

    function join_parts(x::Union{String, Char}, ys::Union{String, Char}...)
        return string(x, ys...)
    end

    @test join_parts("x", 'y', "z") == "xyz"
    @test join_parts('x', "y", 'z', "!") == "xyz!"
end

true
