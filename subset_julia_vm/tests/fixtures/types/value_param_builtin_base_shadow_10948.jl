using Test

short_value_base_10948(Vector::Type) = Vector{Int64}

function full_value_base_10948(Vector::Type)
    Vector{Int64}
end

keyword_value_base_10948(; Vector::Type=Set) = Vector{Int64}

assigned_arrow_value_base_10948 = (Vector::Type) -> Vector{Int64}

destructured_value_base_10948((Vector,)) = Vector{Int64}

original_value_base_10948(x::Float64, Vector::Type) where Float64<:Real = Vector{Float64}

@testset "builtin-spelled value parameter stays lexical as parametric base (Issue #10948)" begin
    @test short_value_base_10948(Set) === Set{Int64}
    @test full_value_base_10948(Set) === Set{Int64}
    @test keyword_value_base_10948(Vector=Set) === Set{Int64}
    @test assigned_arrow_value_base_10948(Set) === Set{Int64}
    @test ((Vector::Type) -> Vector{Int64})(Set) === Set{Int64}
    @test destructured_value_base_10948((Set,)) === Set{Int64}

    # The original adversarial MWE combines an ordinary value-parameter base
    # with the independent builtin-spelled where-binder fixed by Issue #10934.
    @test original_value_base_10948(1, Set) === Set{Int64}
    @test original_value_base_10948(1.0, Dict) === Dict{Float64}

    # Static global type constructors remain on the literal path when no
    # lexical value parameter shadows their name.
    @test Vector{Int64} === Vector{Int64}
end

true
