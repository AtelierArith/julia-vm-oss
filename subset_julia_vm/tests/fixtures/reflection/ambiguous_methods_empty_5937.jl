using Test

# Two methods that are individually applicable to `(Int, Int)` but neither of
# which is strictly more specific than the other form an *ambiguity* at that
# concrete signature. Reflection must report this the way upstream Julia does:
#   methods(g,(Int,Int))            -> empty
#   return_types(g,(Int,Int))       -> Any[]
#   infer_return_type(g,(Int,Int))  -> Union{}
#   which(g,Tuple{Int,Int})         -> error
# (Issue #5937)
g(x::Int, y) = 1
g(x, y::Int) = 2

# Distinct function used for the "ambiguity resolved by a more-specific method"
# regression guard. (sjulia uses a global method table, so adding the resolving
# method to `g` would also affect the ambiguous queries above — a separate name
# keeps the scenarios independent.)
h(x::Int, y) = 1
h(x, y::Int) = 2
h(x::Int, y::Int) = 3

@testset "reflection ambiguity filter (Issue #5937)" begin
    # The ambiguous signature reports no applicable method.
    @test isempty(methods(g, (Int, Int)))
    # All reflection channels cascade from the empty match set.
    @test Base.return_types(g, (Int, Int)) == Any[]
    @test Base.infer_return_type(g, Tuple{Int, Int}) === Union{}
    @test_throws ErrorException which(g, Tuple{Int, Int})

    # Regression: a non-ambiguous signature (only `g(x::Int, y)` applies, since
    # `Float64` is not a subtype of `Int`) still resolves to exactly one method.
    @test length(methods(g, (Int, Float64))) == 1
    # Regression: when a strictly-more-specific third method exists, the pair is
    # no longer ambiguous and exactly that method resolves.
    @test length(methods(h, (Int, Int))) == 1
end

# Final value: conjunction of the checks (harness verifies only the file's final
# expression; bare `@test` failures do not abort).
isempty(methods(g, (Int, Int))) &&
    (Base.return_types(g, (Int, Int)) == Any[]) &&
    (Base.infer_return_type(g, Tuple{Int, Int}) === Union{}) &&
    (try
        which(g, Tuple{Int, Int})
        false
    catch
        true
    end) &&
    (length(methods(g, (Int, Float64))) == 1) &&
    (length(methods(h, (Int, Int))) == 1)
