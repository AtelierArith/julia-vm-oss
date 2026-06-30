# Issue #5040: typed comprehension `T[expr for x in iter]` for the non-numeric
# element types `Bool`, `Char`, `Symbol`, `String`.
#
# Upstream Julia stores each comprehension element through `setindex!`, which
# calls `convert(T, expr)` — NOT the `T(expr)` *constructor*. The previous
# `wrap_comprehension_body_with_call` lowering wrapped the body in `T(expr)`,
# which for these element types either was unreachable as a function in the VM
# (`Bool` / `Symbol` -> "Unknown function"), forced the wrong element slot
# (`Char` was rejected into an I64 slot), or left the result eltype as `Any`
# (`String` produced `Vector{Any}`). The fix rewrites the body to
# `convert(T, expr)` and forces the comprehension result element type to `T`.
#
# Every assertion below was verified against upstream Julia 1.12 for both value
# and `typeof`. (`repr`/`show` of a `Vector{Bool}` differs cosmetically in
# sjulia and is an unrelated, pre-existing show formatting difference, so this
# fixture asserts on value equality + `typeof`, never on `repr`.)

using Test

# ---- Bool ----
@testset "Bool[...] from comparison (#5040)" begin
    v = Bool[x > 0 for x in [1, 0, 1]]
    @test typeof(v) === Vector{Bool}
    @test v == [true, false, true]
end
@testset "Bool[...] identity over Bool source (#5040)" begin
    v = Bool[x for x in [true, false]]
    @test typeof(v) === Vector{Bool}
    @test v == [true, false]
end
@testset "Bool[...] with filter (#5040)" begin
    v = Bool[x > 0 for x in [1, -1, 2] if x != -1]
    @test typeof(v) === Vector{Bool}
    @test v == [true, true]
end

# ---- Char ----
@testset "Char[...] identity over Char source (#5040)" begin
    v = Char[x for x in ['a', 'b']]
    @test typeof(v) === Vector{Char}
    @test v == ['a', 'b']
end
@testset "Char[...] convert Int codepoint -> Char (#5040)" begin
    v = Char[97 for x in 1:2]
    @test typeof(v) === Vector{Char}
    @test v == ['a', 'a']
end

# ---- Symbol ----
@testset "Symbol[...] identity over Symbol source (#5040)" begin
    v = Symbol[x for x in [:a, :b]]
    @test typeof(v) === Vector{Symbol}
    @test v == [:a, :b]
end

# ---- String ----
@testset "String[...] identity over String source (#5040)" begin
    v = String[x for x in ["a", "b"]]
    @test typeof(v) === Vector{String}
    @test v == ["a", "b"]
end

# ---- empty iterators preserve element type ----
@testset "Bool[...] over empty iterator (#5040)" begin
    v = Bool[x > 0 for x in Int[]]
    @test typeof(v) === Vector{Bool}
    @test length(v) == 0
end
@testset "String[...] over empty iterator (#5040)" begin
    v = String[x for x in String[]]
    @test typeof(v) === Vector{String}
    @test length(v) == 0
end

# ---- multi-iterator typed comprehension builds a Matrix{T} ----
@testset "Char[...] multi-iterator -> Matrix{Char} (#5040)" begin
    v = Char[c for c in ['a', 'b'], k in 1:2]
    @test typeof(v) === Matrix{Char}
    @test size(v) == (2, 2)
    @test v == ['a' 'a'; 'b' 'b']
end
@testset "Symbol[...] multi-iterator -> Matrix{Symbol} (#5040)" begin
    v = Symbol[s for s in [:a, :b], k in 1:2]
    @test typeof(v) === Matrix{Symbol}
    @test v == [:a :a; :b :b]
end

# ---- numeric/Any cluster regression guard (must stay green) ----
@testset "Float64[...] comprehension unchanged (#5040 guard)" begin
    v = Float64[x for x in 1:3]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Int8[...] comprehension unchanged (#5040 guard)" begin
    v = Int8[x for x in 1:3]
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end

true
