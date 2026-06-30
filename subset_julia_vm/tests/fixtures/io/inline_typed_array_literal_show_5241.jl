# Issue #5241: an INLINE typed-array literal `T[...]` passed directly to
# `show` / `repr` / `sprint(show, ...)` / `string` must render its elements (with
# the correct eltype prefix from #5236), not the empty-constructor form
# `Vector{T}()`. The literal lowers to `Expr::Index { array: Var("T"), .. }`;
# `infer_expr_type` reported `Any` for it, so `resolve_sprint_function_ref` could
# not pick `show(io::IO, ::Array)` and fell back to the generic struct
# `show(io, x)` which printed `typeof(x)()` = `Vector{T}()`. The fix makes
# inference return `ValueType::ArrayOf(..)` for the typed literal, matching the
# value actually produced at runtime (and the variable-bound form).

using Test

struct Foo5241
    x::Int
end

@testset "inline typed-array literal: sprint(show, ...) (Issue #5241)" begin
    @test sprint(show, Int8[1, 2]) == "Int8[1, 2]"
    @test sprint(show, Int[1, 2, 3]) == "[1, 2, 3]"
    @test sprint(show, Float64[1.0]) == "[1.0]"
    @test sprint(show, Float64[1.5, 2.5]) == "[1.5, 2.5]"
    @test sprint(show, Any[1, "x"]) == "Any[1, \"x\"]"
    @test sprint(show, String["a", "b"]) == "[\"a\", \"b\"]"
    @test sprint(show, Char['a', 'b']) == "['a', 'b']"
    @test sprint(show, Bool[true, false]) == "Bool[1, 0]"
end

@testset "inline typed-array literal: repr (Issue #5241)" begin
    @test repr(Int[1, 2, 3]) == "[1, 2, 3]"
    @test repr(Int8[1, 2]) == "Int8[1, 2]"
    @test repr(Float64[1.5, 2.5]) == "[1.5, 2.5]"
    @test repr(Any[1, 2.0, "x"]) == "Any[1, 2.0, \"x\"]"
end

@testset "inline typed-array literal: string (Issue #5241)" begin
    @test string(Int8[1, 2]) == "Int8[1, 2]"
    @test string(Any[1, "x"]) == "Any[1, \"x\"]"
end

@testset "inline typed-array literal: user struct (Issue #5241)" begin
    @test sprint(show, Foo5241[Foo5241(1), Foo5241(2)]) == "Foo5241[Foo5241(1), Foo5241(2)]"
    @test repr(Foo5241[Foo5241(1), Foo5241(2)]) == "Foo5241[Foo5241(1), Foo5241(2)]"
end

@testset "variable-bound regression: must match inline (Issue #5241)" begin
    a = Int8[1, 2]
    @test sprint(show, a) == "Int8[1, 2]"
    b = Any[1, "x"]
    @test sprint(show, b) == "Any[1, \"x\"]"
    c = Foo5241[Foo5241(1), Foo5241(2)]
    @test sprint(show, c) == "Foo5241[Foo5241(1), Foo5241(2)]"
end

@testset "untyped literal regression (Issue #5241)" begin
    @test sprint(show, [1, 2, 3]) == "[1, 2, 3]"
    @test sprint(show, [1 => 2]) == "[1 => 2]"
    @test repr([1, 2, 3]) == "[1, 2, 3]"
end

true
