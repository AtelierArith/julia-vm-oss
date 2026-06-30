using Test

# Issue #7830: a macro that returns a parametric type expression containing a
# caller `where` type parameter (e.g. Expr(:curly, :Vector, :T)) must resolve T
# from the caller method's instantiation at runtime, not stringify it into a
# static TypeOf("Vector{T}") literal. The macro-return curly converter therefore
# routes through curly_expr_from_values -> DynamicTypeConstruct rather than the
# static type-name fast path.

macro caller_vector_type()
    esc(Expr(:curly, :Vector, :T))
end

function vector_type_for(x::T) where {T}
    @caller_vector_type()
end

# The quote form must behave identically.
macro quoted_vector_type()
    esc(:(Vector{T}))
end

quoted_vector_type_for(x::T) where {T} = @quoted_vector_type()

# A multi-parameter type that uses the same caller param twice.
macro caller_dict_type()
    esc(Expr(:curly, :Dict, :T, :T))
end

dict_type_for(x::T) where {T} = @caller_dict_type()

@testset "macro-returned curly resolves caller where type param (Issue #7830)" begin
    @test vector_type_for(1) == Vector{Int64}
    @test vector_type_for(1.0) == Vector{Float64}
    @test vector_type_for("a") == Vector{String}

    @test quoted_vector_type_for(1) == Vector{Int64}
    @test quoted_vector_type_for(1.0) == Vector{Float64}

    @test dict_type_for(1) == Dict{Int64, Int64}
    @test dict_type_for("a") == Dict{String, String}
end

true
