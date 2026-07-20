# Array-show eltype prefix (Issues #5236 / #5237)
#
# Upstream Julia's array show emits a `T[...]` type prefix when the eltype is
# *non-implicit* (`typeinfo_prefix`/`typeinfo_implicit`, base/arrayshow.jl) and
# nothing for implicit eltypes (Int64/Float64/Char/String/Symbol and implicit
# Tuple/Pair). `Any`-eltype arrays render the elements with an `Any[...]` prefix
# (heterogeneous) or bare for a homogeneous implicit run. This fixture asserts
# `print`/`string`/`repr`/`sprint(show, ...)` parity for the cases where
# sjulia's inferred eltype already matches upstream Julia 1.12 exactly.
#
# Inference-divergent eltypes (sjulia infers `Any` where upstream infers
# `Pair{...}`, or `Complex{Float64}` where upstream keeps `Complex{Int64}`) are
# documented in docs/vm/UNIMPLEMENTED.md and intentionally not asserted here.

using Test

struct ShowPrefixFoo
    x::Int
end

@testset "Array-show eltype prefix (Issues #5236 / #5237)" begin
    # --- implicit eltypes: NO prefix -------------------------------------
    @test string([1, 2, 3]) == "[1, 2, 3]"
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test sprint(show, [1, 2, 3]) == "[1, 2, 3]"

    @test string([1.0, 2.0]) == "[1.0, 2.0]"
    @test repr([1.0, 2.0]) == "[1.0, 2.0]"

    @test repr(['a', 'b']) == "['a', 'b']"
    @test repr([:a, :b]) == "[:a, :b]"
    @test repr(["a", "b"]) == "[\"a\", \"b\"]"
    @test repr([(1, 2), (3, 4)]) == "[(1, 2), (3, 4)]"

    # Pair literal: upstream infers `Pair{Int64,Int64}` (implicit) and sjulia
    # widens to `Any`, but the value-driven prefix derivation keeps both bare.
    @test repr([1 => 1, 2 => 4]) == "[1 => 1, 2 => 4]"
    @test sprint(show, [1 => 2]) == "[1 => 2]"
    @test sprint(show, [1 => 2 3 => 4; 5 => 6 7 => 8]) == "[1 => 2 3 => 4; 5 => 6 7 => 8]"

    # --- non-implicit numeric eltypes: T[...] prefix ---------------------
    @test eltype(Int8[1, 2]) == Int8
    @test string(Int8[1, 2]) == "Int8[1, 2]"
    @test repr(Int8[1, 2]) == "Int8[1, 2]"

    @test eltype(Int128[1, 2]) == Int128
    @test repr(Int128[1, 2]) == "Int128[1, 2]"

    @test eltype(Float32[1.5, 2.25]) == Float32
    @test string(Float32[1.5, 2.25]) == "Float32[1.5, 2.25]"
    @test repr(Float32[1.5, 2.25]) == "Float32[1.5, 2.25]"

    # Bool eltype (Issue #5159 regression): prefix + 1/0 elements.
    @test repr([true, false]) == "Bool[1, 0]"
    @test string([true, false]) == "Bool[1, 0]"
    @test repr(Bool[true false; false true]) == "Bool[1 0; 0 1]"

    # --- user struct eltype: Foo[...] prefix (#5236) ---------------------
    fa = [ShowPrefixFoo(1), ShowPrefixFoo(2)]
    @test eltype(fa) == ShowPrefixFoo
    @test string(fa) == "ShowPrefixFoo[ShowPrefixFoo(1), ShowPrefixFoo(2)]"
    @test repr(fa) == "ShowPrefixFoo[ShowPrefixFoo(1), ShowPrefixFoo(2)]"
    @test sprint(show, fa) == "ShowPrefixFoo[ShowPrefixFoo(1), ShowPrefixFoo(2)]"

    fm = [ShowPrefixFoo(1) ShowPrefixFoo(2); ShowPrefixFoo(3) ShowPrefixFoo(4)]
    @test repr(fm) ==
          "ShowPrefixFoo[ShowPrefixFoo(1) ShowPrefixFoo(2); ShowPrefixFoo(3) ShowPrefixFoo(4)]"

    backing = Memory{ShowPrefixFoo}(undef, 4)
    ref = memoryref(backing)
    @test typeof(ref) == MemoryRef{ShowPrefixFoo}
    @test ref isa MemoryRef{ShowPrefixFoo}

    # --- Any eltype, heterogeneous: Any[...] prefix (#5237) --------------
    ax = Any[1, "x"]
    @test eltype(ax) == Any
    @test string(ax) == "Any[1, \"x\"]"
    @test repr(ax) == "Any[1, \"x\"]"
    @test sprint(show, ax) == "Any[1, \"x\"]"

    # --- nested arrays of implicit eltype: bare (no prefix) --------------
    # Upstream `typeinfo_implicit` treats `Array{T,N}` of an implicit eltype as
    # implicit, so a vector-of-vectors prints bare. Guards against emitting a
    # spurious `Any[...]` / `Vector{Int64}[...]` outer prefix.
    @test string([[1, 2], [3, 4]]) == "[[1, 2], [3, 4]]"
    @test repr([[1, 2], [3, 4]]) == "[[1, 2], [3, 4]]"
    @test repr([["a", "b"], ["c"]]) == "[[\"a\", \"b\"], [\"c\"]]"

    # --- empty typed arrays keep the T[] / Matrix{T}(undef, ...) forms ---
    @test repr(Int8[]) == "Int8[]"
    @test repr(Any[]) == "Any[]"
    @test repr(Float64[]) == "Float64[]"
end

true  # Test passed
