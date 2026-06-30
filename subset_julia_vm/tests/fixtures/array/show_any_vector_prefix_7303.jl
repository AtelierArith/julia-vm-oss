# Issue #7303: `show`/`print`/`string`/`repr` of a genuine `Vector{Any}` must
# keep the `Any[...]` element-type prefix, even when the elements all happen to
# print as a narrower implicit type.
#
# Upstream Julia's `typeinfo_prefix` (base/arrayshow.jl) is type-driven: a
# `Vector{Any}` always prints the `Any[...]` prefix because `typeinfo_implicit(Any)`
# is `false`. sjulia previously derived the prefix from the element *values*, so
# `Any[1, 2, 3]` (homogeneous Int) dropped to bare `[1, 2, 3]`. The value-driven
# path is retained only for the inference-widened composite eltypes (`Pair`/`Tuple`/
# nested arrays) that sjulia stores under the `Any` tag where upstream would infer a
# precise eltype; a homogeneous run of a *scalar* implicit type under an `Any` tag
# means an explicit `Any[...]` and keeps the prefix.
#
# Verified against upstream Julia 1.12.6.

using Test

@testset "Vector{Any} keeps Any[...] prefix (Issue #7303)" begin
    # Homogeneous scalar elements but explicit `Any` eltype: prefix kept.
    @test typeof(Any[1, 2, 3]) === Vector{Any}
    @test string(Any[1, 2, 3]) == "Any[1, 2, 3]"
    @test repr(Any[1, 2, 3]) == "Any[1, 2, 3]"
    @test sprint(show, Any[1, 2, 3]) == "Any[1, 2, 3]"

    @test string(Any[1.0, 2.0]) == "Any[1.0, 2.0]"
    @test repr(Any["a", "b"]) == "Any[\"a\", \"b\"]"
    @test repr(Any[:a, :b]) == "Any[:a, :b]"
    @test repr(Any['a', 'b']) == "Any['a', 'b']"

    # Heterogeneous `Any` array: still `Any[...]` (regression of #5237).
    @test string(Any[1, "x"]) == "Any[1, \"x\"]"
    @test repr(Any[1, "x"]) == "Any[1, \"x\"]"
    @test sprint(show, Any[1, "x"]) == "Any[1, \"x\"]"
    @test repr(Any[1, 2.0, "x"]) == "Any[1, 2.0, \"x\"]"

    # Single-element `Any` vector.
    @test repr(Any[1]) == "Any[1]"
    @test repr(Any["x"]) == "Any[\"x\"]"
end

@testset "narrow eltypes still print bare / prefixed (Issue #7303 regression)" begin
    # Implicit narrow scalar eltypes: NO prefix.
    @test string([1, 2, 3]) == "[1, 2, 3]"
    @test repr(Int[1, 2]) == "[1, 2]"
    @test string(Int[1, 2]) == "[1, 2]"
    @test repr([1.0, 2.0]) == "[1.0, 2.0]"
    @test repr(["a", "b"]) == "[\"a\", \"b\"]"

    # Non-implicit precise eltypes: prefix kept.
    @test repr(Int8[1, 2]) == "Int8[1, 2]"
    @test repr(Real[1, 2]) == "Real[1, 2]"
    @test repr([true, false]) == "Bool[1, 0]"

    # Inference-widened composites under the Any tag still print bare.
    @test repr([1 => 1, 2 => 4]) == "[1 => 1, 2 => 4]"
    @test repr([(1, 2), (3, 4)]) == "[(1, 2), (3, 4)]"
    @test repr([[1, 2], [3, 4]]) == "[[1, 2], [3, 4]]"
end

true
