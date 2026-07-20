using Test

body_vector_10934(x::Float64) where Float64<:Real = Vector{Float64}

body_nested_10934(x::Float64) where Float64<:Real =
    Tuple{Vector{Float64}, Dict{String, Float64}}

body_any_10934(x::Any) where Any<:Real = Vector{Any}

function body_full_10934(x::Float64) where Float64<:Real
    Vector{Float64}
end

body_dynamic_base_10934(x::Float64, F::Type) where Float64<:Real = F{Float64}

function body_dynamic_nested_10934(x::Float64) where Float64<:Real
    F = Vector
    F{Vector{Float64}}
end

body_anonymous_bound_10934(x::Float64) where Float64<:Real = Vector{<:Float64}

body_nested_dynamic_base_10934(::Type{A}) where A = A{A{Int64}}

const BodyAlias10934 = Vector
body_alias_shadowed_base_10934(::Type{BodyAlias10934}) where BodyAlias10934 =
    BodyAlias10934{Int64}

^(x::Float64, y::String) where Float64<:Real = Vector{Float64}

@testset "builtin-named where binder stays lexical in function body (Issue #10934)" begin
    @test body_vector_10934(1) === Vector{Int64}
    @test body_vector_10934(1.0) === Vector{Float64}

    @test body_nested_10934(1) === Tuple{Vector{Int64}, Dict{String, Int64}}
    @test body_nested_10934(1.0) ===
          Tuple{Vector{Float64}, Dict{String, Float64}}

    @test body_any_10934(1) === Vector{Int64}
    @test body_any_10934(1.0) === Vector{Float64}

    @test body_full_10934(1) === Vector{Int64}
    @test body_full_10934(1.0) === Vector{Float64}

    @test body_dynamic_base_10934(1, Vector) === Vector{Int64}
    @test body_dynamic_base_10934(1.0, Vector) === Vector{Float64}

    @test body_dynamic_nested_10934(1) === Vector{Vector{Int64}}
    @test body_dynamic_nested_10934(1.0) === Vector{Vector{Float64}}

    @test body_anonymous_bound_10934(1) == Vector{<:Int64}
    @test body_anonymous_bound_10934(1.0) == Vector{<:Float64}

    @test body_nested_dynamic_base_10934(Vector) === Vector{Vector{Int64}}
    @test body_nested_dynamic_base_10934(Set) === Set{Set{Int64}}

    @test body_alias_shadowed_base_10934(Vector) === Vector{Int64}
    @test body_alias_shadowed_base_10934(Set) === Set{Int64}

    @test ^(1, "operator") === Vector{Int64}
    @test ^(1.0, "operator") === Vector{Float64}
end

true
