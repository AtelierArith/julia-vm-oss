# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/array_empty_typed_eltype_5711.jl =====
module Agg_array_empty_typed_eltype_5711
using Test

# Issue #5711: the element type of an EMPTY typed array literal was dropped for
# several types — eltype(Symbol[]) and eltype(Regex[]) returned Any instead of the
# declared element type (String[]/Char[]/Int[]/Pair[] already worked). The empty
# literal `T[]` lowers to Expr::TypedEmptyArray, whose element-type match omitted
# Symbol / Regex / RegexMatch and fell through to Any.

@testset "empty typed array literal element type (Issue #5711)" begin
    @test eltype(Symbol[]) == Symbol
    @test eltype(Regex[]) == Regex
    @test eltype(RegexMatch[]) == RegexMatch
    @test typeof(Symbol[]) == Vector{Symbol}
    @test typeof(Regex[]) == Vector{Regex}

    # Still-correct control cases (no regression).
    @test eltype(String[]) == String
    @test eltype(Char[]) == Char
    @test eltype(Int[]) == Int
    @test typeof(Float64[]) == Vector{Float64}

    # Non-empty literals unaffected.
    @test eltype(Symbol[:a, :b]) == Symbol
    @test eltype(Regex[r"a"]) == Regex

    # push! preserves the declared element type.
    w = Symbol[]
    push!(w, :x); push!(w, :y)
    @test w == [:x, :y]
    @test eltype(w) == Symbol

    r = Regex[]
    push!(r, r"\d+")
    @test eltype(r) == Regex
    @test occursin(r[1], "a9b") == true
end
end # module Agg_array_empty_typed_eltype_5711

# ===== source: array/array_literal_splat_4793.jl =====
module Agg_array_literal_splat_4793
# Issue #4793: Splat (v...) inside array literals [a, v..., b] failed to lower.
# Splat already worked in tuple literals and function calls; the array
# literal lowering path now also flattens splats inline by lowering
# `[a, v..., b]` to `vcat([a], v, [b])` and reusing vcat's varargs body.

using Test

@testset "Array literal splat: middle (Issue #4793)" begin
    v = [1, 2, 3]
    a = [10, v..., 20]
    @test length(a) == 5
    @test a == [10, 1, 2, 3, 20]
end

@testset "Array literal splat: start (Issue #4793)" begin
    v = [1, 2, 3]
    b = [v..., 100]
    @test b == [1, 2, 3, 100]
end

@testset "Array literal splat: end (Issue #4793)" begin
    v = [1, 2, 3]
    c = [0, v...]
    @test c == [0, 1, 2, 3]
end

@testset "Array literal splat: lone splat (Issue #4793)" begin
    v = [1, 2, 3]
    d = [v...]
    @test d == [1, 2, 3]
end

@testset "Array literal splat: multiple splats (Issue #4793)" begin
    v = [1, 2, 3]
    w = [4, 5]
    e = [v..., w...]
    @test e == [1, 2, 3, 4, 5]
end

@testset "Array literal splat: interleaved with scalars (Issue #4793)" begin
    v = [1, 2, 3]
    w = [4, 5]
    f = [0, v..., 99, w..., 100]
    @test f == [0, 1, 2, 3, 99, 4, 5, 100]
end

@testset "Array literal splat: tuple splat (Issue #4793)" begin
    t = (4, 5)
    g = [1, 2, t..., 3]
    @test g == [1, 2, 4, 5, 3]
end

@testset "Array literal splat: range splat (Issue #4793)" begin
    h = [0, (1:3)..., 100]
    @test h == [0, 1, 2, 3, 100]
end

@testset "Array literal splat: empty splat (Issue #4793)" begin
    e_empty = Int[]
    i = [1, e_empty..., 3]
    @test i == [1, 3]
end

@testset "Array literal splat: float promotion (Issue #4793)" begin
    fv = [1.0, 2.0]
    j = [10, fv..., 20]
    @test j == [10.0, 1.0, 2.0, 20.0]
    @test eltype(j) == Float64
end
end # module Agg_array_literal_splat_4793

# ===== source: array/array_mutate_literal_arg_5674.jl =====
module Agg_array_mutate_literal_arg_5674
using Test

# Issue #5674: mutating builtins (insert!/deleteat!/pushfirst!/push!) rejected a
# non-variable (literal) array first argument with "first argument must be a
# variable". The mutation instructions leave the modified array on the stack, so a
# literal array value can be mutated and returned directly (no binding to store back).

@testset "mutating builtins on a literal array (Issue #5674)" begin
    @test insert!([1, 2, 3], 2, 99) == [1, 99, 2, 3]
    @test insert!([10, 20, 30], 1, 0) == [0, 10, 20, 30]
    @test deleteat!([1, 2, 3, 4], 2) == [1, 3, 4]
    @test deleteat!([1, 2, 3], 3) == [1, 2]
    @test pushfirst!([2, 3], 1) == [1, 2, 3]
    @test push!([1, 2], 3) == [1, 2, 3]
    @test push!(["a", "b"], "c") == ["a", "b", "c"]

    # Element-type handling matches the variable path.
    @test typeof(push!([1, 2], 3)[3]) == Int64        # Int array keeps Int
    @test push!([1.0, 2.0], 3) == [1.0, 2.0, 3.0]     # Float64 array widens
    @test typeof(push!(Float64[1, 2], 3)[3]) == Float64

    # The variable path is unchanged (regression).
    v = [1, 2, 3]
    insert!(v, 2, 99)
    @test v == [1, 99, 2, 3]
    push!(v, 7)
    @test v == [1, 99, 2, 3, 7]
end
end # module Agg_array_mutate_literal_arg_5674

# ===== source: array/array_untyped_param_index_5747.jl =====
module Agg_array_untyped_param_index_5747
# Issue #5747: `a[k]` where `k` is an UNTYPED parameter that holds a Range or
# Vector at runtime must index by whatever `k` is — a sub-array, not a scalar
# element. Previously the compiler treated an untyped-parameter index as scalar
# (it only recognized a LITERAL range/`:` as a slice), so it emitted a scalar
# `IndexLoad` and inferred the array element type (Int64); the genuine sub-array
# then hit `expected I64, got Range` (the load) or `ReturnI64`/`StoreI64`
# (the return/binding coercion).

using Test

@testset "untyped-param index a[k] with runtime Range/Vector (Issue #5747)" begin
    f(a, k) = a[k]

    # Range index -> sub-array
    @test f([10, 20, 30, 40], 2:3) == [20, 30]
    @test f([10, 20, 30, 40], 1:2:4) == [10, 30]

    # Vector index -> sub-array
    @test f([10, 20, 30, 40], [1, 3]) == [10, 30]

    # scalar index -> element (regression: must stay scalar)
    @test f([10, 20, 30, 40], 2) == 20

    # the sub-array result is a real array: bind it, index it, reduce it
    r = f([10, 20, 30, 40], 2:3)
    @test r == [20, 30]
    @test r[1] == 20
    @test length(f([10, 20, 30, 40], 2:3)) == 2
    @test sum(f([10, 20, 30, 40], 1:4)) == 100

    # Float array stays Float
    @test f([1.0, 2.0, 3.0], 2:3) == [2.0, 3.0]

    # typed-param forms still work (regression)
    g(a, k::AbstractRange) = a[k]
    @test g([10, 20, 30, 40], 2:3) == [20, 30]
end
end # module Agg_array_untyped_param_index_5747

# ===== source: array/complex_typed_array_literal_4605.jl =====
module Agg_complex_typed_array_literal_4605
using Test

@testset "Complex typed array literals preserve eltype (#4018, #4605)" begin
    f64_alias = ComplexF64[1 + 2im, 3 - 4im]
    @test typeof(f64_alias) == Vector{ComplexF64}
    @test eltype(f64_alias) == ComplexF64
    @test typeof(f64_alias[1]) == ComplexF64
    @test f64_alias[1] == 1 + 2im
    @test f64_alias[2] == 3 - 4im

    f64_parametric = Complex{Float64}[1 + 2im, 3 - 4im]
    @test typeof(f64_parametric) == Vector{ComplexF64}
    @test eltype(f64_parametric) == ComplexF64
    @test typeof(f64_parametric[1]) == ComplexF64
    @test f64_parametric[1] == 1 + 2im
    @test f64_parametric[2] == 3 - 4im

    f32_parametric = Complex{Float32}[1 + 2im, 3 + 4im]
    @test typeof(f32_parametric) == Vector{ComplexF32}
    @test eltype(f32_parametric) == ComplexF32
    @test typeof(f32_parametric[1]) == ComplexF32
    @test typeof(real(f32_parametric[1])) == Float32
    @test typeof(imag(f32_parametric[1])) == Float32
    @test real(f32_parametric[1]) == Float32(1)
    @test imag(f32_parametric[1]) == Float32(2)
end
end # module Agg_complex_typed_array_literal_4605

# ===== source: array/matrix_literal_tuple_elements_9437.jl =====
module Agg_matrix_literal_tuple_elements_9437
using Test

# Issue #9437: in a matrix/hcat row, whitespace before `(` starts a new
# scalar tuple element instead of a spaced call on the previous tuple.

@testset "matrix literal tuple elements (#9437)" begin
    m = [(1, 2) (3, 4)]
    @test m == reshape([(1, 2), (3, 4)], 1, 2)
    @test typeof(m) === Matrix{Tuple{Int64, Int64}}
    @test size(m) == (1, 2)
    @test eltype(m) === Tuple{Int64, Int64}
    @test m[1, 1] === (1, 2)
    @test m[1, 2] === (3, 4)

    typed = Tuple{Int64, Int64}[(1, 2) (3, 4)]
    @test typed == m
    @test typeof(typed) === Matrix{Tuple{Int64, Int64}}
    @test size(typed) == (1, 2)
    @test typed[1, 1] === (1, 2)
    @test typed[1, 2] === (3, 4)

    v = [(1, 2); (3, 4)]
    @test v == [(1, 2), (3, 4)]
    @test typeof(v) === Vector{Tuple{Int64, Int64}}
    @test size(v) == (2,)
    @test eltype(v) === Tuple{Int64, Int64}
    @test v[1] === (1, 2)
    @test v[2] === (3, 4)
end

function array_dynamic_helper_source_9820(x)
    boxed = Any[x]
    return boxed[1]
end

@testset "typed tuple literals keep Array helper dispatch through Any slots (#9820)" begin
    m = Tuple{Int64, Int64}[(1, 2) (3, 4)]
    a = array_dynamic_helper_source_9820(m)

    @test typeof(a) === Matrix{Tuple{Int64, Int64}}
    @test size(a) == (1, 2)
    @test size(a, 1) == 1
    @test size(a, 2) == 2
    @test length(a) == 2
    @test a[1, 1] === (1, 2)
    @test a[1, 2] === (3, 4)

    reshaped = reshape(a, (2, 1))
    @test typeof(reshaped) === Matrix{Tuple{Int64, Int64}}
    @test size(reshaped) == (2, 1)
    @test length(reshaped) == 2
    @test reshaped[1, 1] === (1, 2)
    @test reshaped[2, 1] === (3, 4)

    similar_matrix = similar(a, (2, 1))
    @test typeof(similar_matrix) === Matrix{Tuple{Int64, Int64}}
    @test size(similar_matrix) == (2, 1)
    @test eltype(similar_matrix) === Tuple{Int64, Int64}
    similar_matrix[1, 1] = (5, 6)
    similar_matrix[2, 1] = (7, 8)
    @test similar_matrix[1, 1] === (5, 6)
    @test similar_matrix[2, 1] === (7, 8)
end
end # module Agg_matrix_literal_tuple_elements_9437

# ===== source: array/pair_typed_undef_allocation_4635.jl =====
module Agg_pair_typed_undef_allocation_4635
using Test

@testset "Pair typed undef allocation preserves parameters (#4018, #4635)" begin
    a = Array{Pair{Int64, Int8}}(undef, 2)
    @test typeof(a) === Vector{Pair{Int64, Int8}}
    @test eltype(a) === Pair{Int64, Int8}
    a[1] = Pair(1, Int8(2))
    @test a[1][1] == 1
    @test a[1][2] == Int8(2)

    b = similar(Array{Pair{String, Int16}}, (2,))
    @test typeof(b) === Vector{Pair{String, Int16}}
    @test eltype(b) === Pair{String, Int16}
    b[1] = Pair("x", Int16(3))
    @test b[1][1] == "x"
    @test b[1][2] == Int16(3)
end
end # module Agg_pair_typed_undef_allocation_4635

# ===== source: array/runtime_datatype_typed_array_constructor_4606.jl =====
module Agg_runtime_datatype_typed_array_constructor_4606
using Test

function typed_vector_from_runtime_type(T)
    T[1, 2]
end

function typed_vector_from_runtime_getindex(T)
    getindex(T, 3, 4)
end

@testset "runtime DataType typed array constructor (#4606)" begin
    int16_values = typed_vector_from_runtime_type(Int16)
    @test typeof(int16_values) === Vector{Int16}
    @test int16_values == Int16[1, 2]

    float32_values = typed_vector_from_runtime_type(Float32)
    @test typeof(float32_values) === Vector{Float32}
    @test length(float32_values) == 2
    @test float32_values[1] === Float32(1)
    @test float32_values[2] === Float32(2)

    getindex_values = typed_vector_from_runtime_getindex(UInt8)
    @test typeof(getindex_values) === Vector{UInt8}
    @test length(getindex_values) == 2
    @test getindex_values[1] === UInt8(3)
    @test getindex_values[2] === UInt8(4)

    real_values = Real[1, 1.5, Float32(2.5)]
    @test typeof(real_values) === Vector{Real}
    @test eltype(real_values) === Real
    @test real_values[1] == 1
    @test real_values[2] === 1.5
    @test real_values[3] === Float32(2.5)

    number_values = Number[1, 1.5, 1 + 2im]
    @test typeof(number_values) === Vector{Number}
    @test eltype(number_values) === Number
    @test number_values[1] == 1
    @test number_values[2] === 1.5
    @test number_values[3] == 1 + 2im
end
end # module Agg_runtime_datatype_typed_array_constructor_4606

# ===== source: array/tuple_collect_abstract_numeric_eltype_4662.jl =====
module Agg_tuple_collect_abstract_numeric_eltype_4662
using Test

function check_collect_tuple_eltype(t, expected_type)
    result = collect(t)
    ok = typeof(result) === Vector{expected_type}
    ok = ok && eltype(result) === expected_type
    ok = ok && length(result) == length(t)
    for i in 1:length(t)
        ok = ok && result[i] == t[i]
        ok = ok && typeof(result[i]) === typeof(t[i])
    end
    ok
end

function check_typed_undef_eltype(T)
    result = Vector{T}(undef, 2)
    ok = typeof(result) === Vector{T}
    ok = ok && eltype(result) === T
    ok = ok && length(result) == 2
    ok
end

@testset "tuple collect abstract numeric eltype (Issues #4018/#4662)" begin
    @test check_collect_tuple_eltype((Int8(1), Int16(2)), Signed)
    @test check_collect_tuple_eltype((UInt8(1), UInt16(2)), Unsigned)
    @test check_collect_tuple_eltype((Int8(1), UInt8(2)), Integer)
    @test check_collect_tuple_eltype((Float32(1), Float64(2)), AbstractFloat)
    @test check_collect_tuple_eltype((Int8(1), Float64(2)), Real)

    @test check_typed_undef_eltype(Number)
    @test check_typed_undef_eltype(Real)
    @test check_typed_undef_eltype(Integer)
    @test check_typed_undef_eltype(Signed)
    @test check_typed_undef_eltype(Unsigned)
    @test check_typed_undef_eltype(AbstractFloat)
end
end # module Agg_tuple_collect_abstract_numeric_eltype_4662

# ===== source: array/typed_array_literal_uint_convert_7953.jl =====
module Agg_typed_array_literal_uint_convert_7953
# Issue #7953: a typed array literal `T[elems...]` must `convert(T, x)` each
# element to the declared element type `T` before storing, exactly like upstream
# Julia (whose `T[a, b, ...]` lowers to `a = Vector{T}(undef, n); a[i] = vals[i]`,
# and `setindex!` does `convert(T, x)`).
#
# Plain Int literals already match `Int64` storage, so the missing per-element
# convert was invisible until a UInt-family hex literal (`0x30::UInt8`,
# `0x663::UInt16`, ...) is mixed into a signed/float typed literal: sjulia tried
# to store the raw `UInt8` into an `Int64` array and failed with
#   "Cannot store U8 in I64 array".
#
# Routing each element through `convert(T, x)` also makes out-of-range elements
# raise `InexactError` (matching upstream) instead of silently truncating.
using Test

@testset "Issue #7953: typed array literal converts UInt hex elements" begin
    # The exact repro from the issue.
    @test Int[0x30, 0x39] == [48, 57]
    @test Int[0x30, 0x39] isa Vector{Int64}
    @test eltype(Int[0x30, 0x39]) === Int64

    # Wider hex literals (UInt16) convert into the declared Int element type.
    @test Int[0x663] == [1635]
    @test Int[0x663] isa Vector{Int64}

    # Mixed hex (UInt8) and decimal (Int64) elements.
    @test Int[0x30, 49] == [48, 49]
    @test Int[0x30, 49] isa Vector{Int64}

    # Narrower signed targets still convert in-range hex elements.
    @test Int8[0x30] == Int8[48]
    @test Int8[0x30] isa Vector{Int8}
    @test Int32[0x30] == Int32[48]
    @test Int32[0x30] isa Vector{Int32}
    @test Int128[0x30] == Int128[48]
    @test eltype(Int128[0x30]) === Int128

    # Unsigned targets: decimal Int literals convert into the UInt element type.
    @test UInt8[1, 2] == UInt8[0x01, 0x02]
    @test UInt8[1, 2] isa Vector{UInt8}
    # ... and a mix of narrower hex literals widens into UInt64.
    @test UInt[0x30, 0x663] == UInt64[0x30, 0x663]
    @test UInt[0x30, 0x663] isa Vector{UInt64}

    # Float targets convert hex elements too.
    @test Float64[0x30] == [48.0]
    @test Float64[0x30] isa Vector{Float64}
    @test Float32[0x30] == Float32[48.0]
    @test Float32[0x30] isa Vector{Float32}

    # 2-D typed matrix literals convert per element as well.
    M = Int[0x1 0x2; 0x3 0x4]
    @test M == [1 2; 3 4]
    @test M isa Matrix{Int64}

    # Regression: pure decimal literals keep working unchanged.
    @test Int[48, 57] == [48, 57]
    @test Int[48, 57] isa Vector{Int64}

    # Out-of-range elements raise InexactError (faithful convert semantics),
    # instead of silently truncating.
    @test_throws InexactError Int[0xffffffffffffffff]
    @test_throws InexactError Int8[0xc8]
    @test_throws InexactError UInt8[300]
end
end # module Agg_typed_array_literal_uint_convert_7953

# ===== source: array/typed_comprehension_nonnumeric_eltypes_5040.jl =====
module Agg_typed_comprehension_nonnumeric_eltypes_5040
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
end # module Agg_typed_comprehension_nonnumeric_eltypes_5040

# ===== source: array/typed_matrix_literal_4575_4629.jl =====
module Agg_typed_matrix_literal_4575_4629
using Test

@testset "typed matrix literal lowering (#4575, #4629)" begin
    f32 = Float32[1 2; 3 4]
    @test typeof(f32) === Matrix{Float32}
    @test eltype(f32) === Float32
    @test size(f32) == (2, 2)
    @test typeof(f32[1, 1]) === Float32
    @test f32[1, 2] == Float32(2)
    @test f32[2, 1] == Float32(3)

    real_values = Real[1 2.5; 3 Float32(4.5)]
    @test typeof(real_values) === Matrix{Real}
    @test eltype(real_values) === Real
    @test size(real_values) == (2, 2)
    @test typeof(real_values[1, 1]) === Int64
    @test typeof(real_values[1, 2]) === Float64
    @test real_values[1, 2] === 2.5
    @test typeof(real_values[2, 2]) === Float32
    @test real_values[2, 2] === Float32(4.5)
end
end # module Agg_typed_matrix_literal_4575_4629

# ===== source: array/typed_vector_any_conversion_4818.jl =====
module Agg_typed_vector_any_conversion_4818
# Issue #4818 (sibling to #4811/#4816): Vector{Any}(::Vector{S})
# returned the source vector unchanged instead of materializing a
# Vector{Any}. Same compile-time intercept in
# `compile_array_constructor` as #4811/#4816, but for T == Any.
#
# Fix: the prior typed-comprehension synthesis (#4815/#4817) cannot be
# reused for T == Any because `Any[x for x in arr]` lowers to a body
# wrapped in `Any(x)`, which is not a defined Julia constructor (and
# raises "Unknown function: Any" — tracked as #4819). Instead the
# intercept routes through a Pure-Julia helper
# `_vector_any_collect(arr)` that allocates `Vector{Any}(undef, n)`
# and copies each element via plain indexed store, which boxes each
# value to Any as a side effect of the Vector{Any} backing store.

using Test

@testset "Vector{Any}(::Vector{Int}) — boxes to Any (Issue #4818)" begin
    v = Vector{Any}([1, 2, 3])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test length(v) == 3
    @test v[1] == 1
    @test v[2] == 2
    @test v[3] == 3
end

@testset "Vector{Any}(::Vector{Float64}) — boxes to Any (Issue #4818)" begin
    v = Vector{Any}([1.0, 2.0, 3.0])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v[1] == 1.0
end

@testset "Vector{Any}(::Vector{Any}) — same eltype copy (Issue #4818)" begin
    src = Vector{Any}([1, 2.0, "three"])
    v = Vector{Any}(src)
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test length(v) == 3
end

@testset "Vector{Any}() empty regression (Issue #4818)" begin
    # Empty Vector{Any}() stays on the empty-array path,
    # not on the new helper-call branch.
    v = Vector{Any}()
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

@testset "Vector{Any}(undef, n) regression (Issue #4818)" begin
    # The undef pattern stays on the existing args.len()==2 branch.
    v = Vector{Any}(undef, 3)
    @test typeof(v) === Vector{Any}
    @test length(v) == 3
end

@testset "Vector{Int64}(::Vector{Int64}) regression — fast path (Issue #4818)" begin
    # Non-Any same-eltype case must keep the no-op fast path, not
    # accidentally regress into the Any-helper branch.
    src = [10, 20, 30]
    v = Vector{Int64}(src)
    @test typeof(v) === Vector{Int64}
    @test v == [10, 20, 30]
end
end # module Agg_typed_vector_any_conversion_4818

# ===== source: array/typed_vector_comprehension_cluster_parity_4824.jl =====
module Agg_typed_vector_comprehension_cluster_parity_4824
# Issue #4824 (prevention): typed-vector / typed-comprehension intercept cluster.
#
# Covers the cluster of bugs #4811 / #4816 / #4818 / #4819 / #4822, all of which
# shared one root: compile-time *intercepts* in
# `compile/expr/collection.rs` (`compile_array_constructor`,
# `compile_comprehension`) that short-circuit method dispatch with hardcoded
# happy-path assumptions and previously produced a wrong-typed result when the
# assumption failed. Each cluster bug has its own per-issue regression fixture;
# this is the *combinatorial parity probe* asked for in #4824 — a single sweep
# over (target T) x (argument shape) that locks in the correct value AND
# `typeof` so a future change to the intercept path cannot silently break any
# cell without tripping this probe.
#
# Every assertion below was verified to match upstream Julia 1.12 for both the
# resulting value and `typeof`. Coverage matrix:
#   target T   : Int64, Int8, Float64, Float32, Any, String, Char, Symbol
#   arg shapes : UnitRange, StepRange, StepRangeLen (int+float step),
#                Vector{S} (S in {Int,Float64,String,Char,Symbol}),
#                empty array, plus T[expr for x in iter] typed comprehension
#                and the plain Any-body comprehension fallback (#4822).
#
# NOTE ON SCOPE: the #4824 cluster of *fixed* bugs is strictly the numeric and
# Any element types (Int*/Float*/Any) over range/array/empty shapes. While
# probing for this fixture, separate, out-of-cluster divergences were found for
# Bool/Char/Symbol/String *typed comprehensions* (`Bool[...]`, `Char[...]`,
# `Symbol[...]`, `String[...]`) and for `Vector{T}(::Tuple)`; those are NOT part
# of the #4811/#4816/#4818/#4819/#4822 cluster and are tracked as their own
# issues. They are deliberately excluded here so this probe stays focused on the
# cluster it guards.
#
# UPDATE: the Bool/Char/Symbol/String typed-comprehension divergence (#5040) has
# since been fixed and is covered by its own dedicated regression fixture
# `array/typed_comprehension_nonnumeric_eltypes_5040.jl` (single- and
# multi-iterator, filter, empty iterator, and Int->Char convert cells). It is
# kept separate from this cluster probe by design. `Vector{T}(::Tuple)` (#5041)
# remains out of scope.

using Test

# ---- #4811: Vector{T}(::AbstractRange) typed range constructor ----
@testset "Vector{T}(range): UnitRange Int -> Float64 (#4811)" begin
    v = Vector{Float64}(1:3)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Vector{T}(range): UnitRange Int -> Int64 identity (#4811)" begin
    v = Vector{Int64}(1:3)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end
@testset "Vector{T}(range): UnitRange Int -> Int8 (#4811)" begin
    v = Vector{Int8}(1:3)
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end
@testset "Vector{T}(range): UnitRange Int -> Float32 (#4811)" begin
    v = Vector{Float32}(1:3)
    @test typeof(v) === Vector{Float32}
    @test v == Float32[1.0, 2.0, 3.0]
end
@testset "Vector{T}(range): StepRange Int -> Float64 (#4811)" begin
    v = Vector{Float64}(1:2:9)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 3.0, 5.0, 7.0, 9.0]
end
@testset "Vector{T}(range): StepRangeLen Float -> Float64 (#4811)" begin
    v = Vector{Float64}(1.0:0.5:3.0)
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 1.5, 2.0, 2.5, 3.0]
end
@testset "Vector{T}(range): Float range -> Int64 (#4811)" begin
    v = Vector{Int64}(1.0:3.0)
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end
@testset "Vector{T}(range): UnitRange -> Any boxes (#4818/#4811)" begin
    v = Vector{Any}(1:3)
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1, 2, 3]
end

# ---- #4816: Vector{T}(::Vector{S}) eltype conversion ----
@testset "Vector{T}(arr): Int -> Float64 (#4816)" begin
    v = Vector{Float64}([1, 2, 3])
    @test typeof(v) === Vector{Float64}
    @test eltype(v) === Float64
    @test v == [1.0, 2.0, 3.0]
end
@testset "Vector{T}(arr): Float -> Int64 (#4816)" begin
    v = Vector{Int64}([1.0, 2.0])
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2]
end
@testset "Vector{T}(arr): Float64 -> Float32 (#4816)" begin
    v = Vector{Float32}([1.0, 2.0])
    @test typeof(v) === Vector{Float32}
    @test v == Float32[1.0, 2.0]
end
@testset "Vector{T}(arr): Int -> Int8 (#4816)" begin
    v = Vector{Int8}([1, 2, 3])
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end
@testset "Vector{T}(arr): same eltype Int64 fast path (#4816)" begin
    v = Vector{Int64}([1, 2, 3])
    @test typeof(v) === Vector{Int64}
    @test v == [1, 2, 3]
end
@testset "Vector{T}(arr): same eltype Float64 fast path (#4816)" begin
    v = Vector{Float64}([1.0, 2.0])
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0]
end

# ---- #4818: Vector{Any}(::Vector{S}) boxing ----
@testset "Vector{Any}(arr): Int -> Any (#4818)" begin
    v = Vector{Any}([1, 2, 3])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1, 2, 3]
end
@testset "Vector{Any}(arr): Float -> Any (#4818)" begin
    v = Vector{Any}([1.0, 2.0, 3.0])
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1.0, 2.0, 3.0]
end

# ---- non-numeric eltype constructor path (verified matching on main) ----
@testset "Vector{String}(arr) identity (#4816 path)" begin
    v = Vector{String}(["a", "b"])
    @test typeof(v) === Vector{String}
    @test v == ["a", "b"]
end
@testset "Vector{Char}(arr) identity (#4816 path)" begin
    v = Vector{Char}(['a', 'b'])
    @test typeof(v) === Vector{Char}
    @test v == ['a', 'b']
end
@testset "Vector{Symbol}(arr) identity (#4816 path)" begin
    v = Vector{Symbol}([:a, :b])
    @test typeof(v) === Vector{Symbol}
    @test v == [:a, :b]
end

# ---- empty array argument shape ----
@testset "Vector{Float64}(empty Int[]) (#4816)" begin
    v = Vector{Float64}(Int[])
    @test typeof(v) === Vector{Float64}
    @test length(v) == 0
end
@testset "Vector{Any}(empty Int[]) (#4818)" begin
    v = Vector{Any}(Int[])
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

# ---- #4819: Any[expr for x in iter] typed-Any comprehension ----
@testset "Any[x for x in array] (#4819)" begin
    v = Any[x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Any}
    @test eltype(v) === Any
    @test v == [1, 2, 3]
end
@testset "Any[x for x in range] (#4819)" begin
    v = Any[x for x in 1:3]
    @test typeof(v) === Vector{Any}
    @test v == [1, 2, 3]
end
@testset "Any[x*2 for x in array] non-identity body (#4819)" begin
    v = Any[x * 2 for x in [1, 2, 3]]
    @test typeof(v) === Vector{Any}
    @test v == [2, 4, 6]
end
@testset "Any[x for x in empty range] (#4819)" begin
    v = Any[x for x in 1:0]
    @test typeof(v) === Vector{Any}
    @test length(v) == 0
end

# ---- typed comprehension for concrete numeric T (intercept path) ----
@testset "Float64[x for x in range] (#4819 regression guard)" begin
    v = Float64[x for x in 1:3]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Float64[x for x in array] (#4816/#4819)" begin
    v = Float64[x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
@testset "Int8[x for x in range] (#4819)" begin
    v = Int8[x for x in 1:3]
    @test typeof(v) === Vector{Int8}
    @test v == Int8[1, 2, 3]
end

# ---- #4822: Any-body comprehension must not silently coerce to Float64 ----
@testset "[convert(Any,x) for x in Int array] no Float coercion (#4822)" begin
    v = [convert(Any, x) for x in [1, 2, 3]]
    # Upstream infers Vector{Int64}; sjulia preserves losslessly as Vector{Any}.
    # Both are acceptable per #4822 — what is NOT acceptable is Vector{Float64}
    # with silent Float coercion. Assert values and forbid the F64 coercion.
    @test eltype(v) !== Float64
    @test v[1] === 1
    @test v[2] === 2
    @test v[3] === 3
    @test v == [1, 2, 3]
end
@testset "[convert(Any,x) for x in String array] no Float coercion (#4822)" begin
    v = [convert(Any, x) for x in ["a", "b"]]
    @test eltype(v) !== Float64
    @test v == ["a", "b"]
end

# ---- plain comprehension regressions (intercept inference) ----
@testset "untyped Int comprehension stays Vector{Int64} (#4822)" begin
    v = [x for x in [1, 2, 3]]
    @test typeof(v) === Vector{Int64}
end
@testset "untyped Float comprehension stays Vector{Float64} (#4822)" begin
    v = [Float64(x) for x in [1, 2, 3]]
    @test typeof(v) === Vector{Float64}
    @test v == [1.0, 2.0, 3.0]
end
end # module Agg_typed_vector_comprehension_cluster_parity_4824

# ===== source: array/typed_vector_equality_4639.jl =====
module Agg_typed_vector_equality_4639
using Test

@testset "typed vector equality dispatch (#4639)" begin
    @test Int16[1, 2] == Int16[1, 2]
    @test !(Int16[1, 2] == Int16[1, 3])
    @test Int16[1, 2] != Int16[1, 3]

    xs = Int16[1, 2]
    ys = Int16[1, 2]
    zs = Int16[1, 3]
    @test xs == ys
    @test xs != zs
    @test typeof(xs == ys) === Bool
end
end # module Agg_typed_vector_equality_4639

# ===== source: array/typed_vector_tuple_method_error_5041.jl =====
module Agg_typed_vector_tuple_method_error_5041
# Issue #5041: Vector{T}(::Tuple) / Array{T}(::Tuple) must raise MethodError.
#
# Upstream Julia has *no* `Array{T}(::Tuple)` / `Vector{T}(::Tuple)` constructor
# method — `Vector{Int}((1, 2, 3))` raises a `MethodError` (the correct spellings
# are `collect((1,2,3))` or `Int[(1,2,3)...]`). sjulia's single-arg
# `compile_array_constructor` intercept previously treated any iterable-ish
# argument as an array/range to materialize and silently built a vector from a
# tuple — an undocumented out-of-cluster divergence from the resolved
# #4811/#4816/#4818/#4819/#4822 set (those covered Range / Array / empty-array /
# Any shapes, never Tuple).
#
# The fix guards the Tuple argument shape and synthesizes the same catchable
# runtime `MethodError(ctor, (tuple,))` upstream raises (rendering, for the typed
# form, exactly `no method matching Vector{Int64}(::Tuple{...})`). The legitimate
# Range / Array / undef-dims / collect / comprehension paths are unaffected.
#
# Every assertion below was verified to match upstream Julia 1.12.

using Test

# ---- now-erroring cases: Tuple argument has no constructor method (#5041) ----
@testset "Vector{T}(::Tuple) raises MethodError (#5041)" begin
    @test_throws MethodError Vector{Int64}((1, 2, 3))
    @test_throws MethodError Vector{Float64}((1, 2, 3))
    @test_throws MethodError Vector{Any}((1, 2, 3))
    @test_throws MethodError Vector{Int64}((1,))
end

@testset "Array{T}/bare-alias (::Tuple) raises MethodError (#5041)" begin
    @test_throws MethodError Array{Int64,1}((1, 2, 3))
    @test_throws MethodError Vector((1, 2, 3))
    @test_throws MethodError Array((1, 2, 3))
end

# ---- regression guard: tuple-from-array constructions still work (#5041) ----
@testset "collect(tuple) and tuple comprehension still work (#5041)" begin
    @test collect((1, 2, 3)) == [1, 2, 3]
    @test typeof(collect((1, 2, 3))) === Vector{Int64}
    @test [x for x in (1, 2, 3)] == [1, 2, 3]
    @test [2x for x in (10, 20, 30)] == [20, 40, 60]
end

# ---- regression guard: valid Vector/Array constructors untouched (#5041) ----
@testset "valid Vector/Array constructors untouched (#5041)" begin
    # undef-sized allocation
    @test length(Vector{Int64}(undef, 3)) == 3
    # range argument (materialize + convert eltype)
    @test Vector{Float64}(1:3) == [1.0, 2.0, 3.0]
    @test typeof(Vector{Float64}(1:3)) === Vector{Float64}
    @test Vector{Int64}(1:3) == [1, 2, 3]
    # array argument (convert eltype / box to Any)
    @test Vector{Float64}([1, 2, 3]) == [1.0, 2.0, 3.0]
    @test typeof(Vector{Float64}([1, 2, 3])) === Vector{Float64}
    @test Vector{Any}([1, 2, 3]) == Any[1, 2, 3]
    @test typeof(Vector{Any}([1, 2, 3])) === Vector{Any}
    # tuple as DIMS arg to undef allocation is valid (tuple is dims, not data)
    @test size(Array{Int64}(undef, (2, 3))) == (2, 3)
end
end # module Agg_typed_vector_tuple_method_error_5041

true
