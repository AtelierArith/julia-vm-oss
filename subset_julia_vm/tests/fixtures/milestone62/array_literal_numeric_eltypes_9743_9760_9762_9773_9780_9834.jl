using Test

@testset "numeric array literal eltypes" begin
    @test typeof([UInt8(1), 2]) == Vector{Int64}
    @test typeof([1f0, 2.0]) == Vector{Float64}
    narrow = 1f0
    wide = 2.0
    @test typeof([narrow, wide]) == Vector{Float64}
    @test typeof([1 // 2, 2]) == Vector{Rational{Int64}}
    @test typeof(Rational{Int64}[1, 2]) == Vector{Rational{Int64}}
    @test typeof(Rational{Int64}[1, 2][1]) == Rational{Int64}

    @test typeof([Base.MathConstants.e, Base.MathConstants.e]) == Vector{Irrational{:ℯ}}
    @test typeof([pi, Base.MathConstants.e]) == Vector{Float64}
    @test typeof([true, pi]) == Vector{Float64}

    @test typeof(Float32[1, 2]) == Vector{Float32}
    @test typeof(BigFloat[1, 2]) == Vector{BigFloat}
    @test typeof([complex(1, 2), complex(3, 4)]) == Vector{Complex{Int64}}
end

true
