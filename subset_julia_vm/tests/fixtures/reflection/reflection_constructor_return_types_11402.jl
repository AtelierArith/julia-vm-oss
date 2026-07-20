# Direct constructor reflection resolves the concrete instantiated result
# structurally, matching upstream: the applied parametric spelling
# constructs exactly itself, a bare family infers its type parameters from
# the argument types, and unmatched arities report Union{} — instead of the
# widened Any / empty results the function-name path produced
# (Issue #11402, tech-debt #11447).
using Test

struct DirectCtorGap11402{T}
    x::T
end

struct ExplicitInnerGap11402{T}
    x::T
    ExplicitInnerGap11402{T}(x) where {T} = new{T}(x)
end

struct PlainCtor11402
    a::Int64
    b::String
end

f11402(x::Int) = x + 1

@testset "constructor reflection (Issue #11402)" begin
    @test Base.infer_return_type(DirectCtorGap11402, Tuple{Int64}) ==
          DirectCtorGap11402{Int64}
    @test Base.return_types(DirectCtorGap11402, Tuple{Int64}) ==
          Any[DirectCtorGap11402{Int64}]
    @test Base.infer_return_type(DirectCtorGap11402{Int64}, Tuple{Int64}) ==
          DirectCtorGap11402{Int64}
    @test Base.return_types(DirectCtorGap11402{Int64}, Tuple{Int64}) ==
          Any[DirectCtorGap11402{Int64}]
    @test Base.infer_return_type(DirectCtorGap11402, Tuple{Float64}) ==
          DirectCtorGap11402{Float64}

    @test Base.infer_return_type(ExplicitInnerGap11402{Int64}, Tuple{Int64}) ==
          ExplicitInnerGap11402{Int64}
    @test Base.return_types(ExplicitInnerGap11402{Int64}, Tuple{Int64}) ==
          Any[ExplicitInnerGap11402{Int64}]

    @test Base.infer_return_type(PlainCtor11402, Tuple{Int64, String}) == PlainCtor11402

    # Unmatched arities dispatch to no constructor.
    @test Base.infer_return_type(PlainCtor11402, Tuple{Int64}) == Union{}
    @test Base.infer_return_type(DirectCtorGap11402{Int64}, Tuple{Int64, Int64}) == Union{}

    # Ordinary function reflection is untouched.
    @test Base.infer_return_type(f11402, Tuple{Int64}) == Int64
    @test Base.infer_return_type(+, Tuple{Int64, Float64}) == Float64
end

true
