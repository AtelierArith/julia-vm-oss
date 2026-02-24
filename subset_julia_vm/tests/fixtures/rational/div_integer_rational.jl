using Test

@testset "div(Integer, Rational{T}) (Issue #2480)" begin
    # div(Int64, Rational{Int64})
    @test div(5, 3//2) == 3
    @test div(7, 3//2) == 4
    @test div(1, 1//2) == 2

    # div(Int32, Rational{Int32})
    r32 = Int32(3) // Int32(2)
    @test div(Int32(5), r32) == 3
    @test div(Int32(7), r32) == 4

    # div(Int16, Rational{Int16})
    r16 = Int16(3) // Int16(2)
    @test div(Int16(5), r16) == 3

    # div(Rational{T}, Integer)
    @test div(7//2, 2) == 1
    @test div(Int32(7) // Int32(2), Int32(2)) == 1

    # div(Rational{T}, Rational{T})
    @test div(7//2, 3//2) == 2
    @test div(Int32(7) // Int32(2), Int32(3) // Int32(2)) == 2
end

true
