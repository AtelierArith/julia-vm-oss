# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/abstractarray_subtype_equality_8229.jl =====
module Agg_abstractarray_subtype_equality_8229
# `==` / `isequal` element-compare against an AbstractArray-subtype operand that
# is neither a native array nor a StaticArrays carrier — a user
# `struct <: AbstractVector` and a `SubArray` view (Issue #8229). Previously
# these returned object-identity `false` because the equality builtin could not
# read the operand, and the operand also did not match `::AbstractArray` method
# parameters (so the Pure-Julia element-wise `isequal` was unreachable).
using Test

struct MyVec <: AbstractVector{Float64}
    data::Vector{Float64}
end
Base.size(v::MyVec) = size(v.data)
Base.getindex(v::MyVec, i::Int) = v.data[i]

@testset "user struct <: AbstractVector equality" begin
    v = MyVec([1.0, 2.0, 3.0])

    # Element-wise, not object identity.
    @test isequal(v, [1.0, 2.0, 3.0])
    @test v == [1.0, 2.0, 3.0]
    @test [1.0, 2.0, 3.0] == v
    @test v == MyVec([1.0, 2.0, 3.0])
    @test isequal(v, MyVec([1.0, 2.0, 3.0]))

    # Distinct contents compare unequal.
    @test !(v == MyVec([1.0, 2.0, 9.0]))
    @test v != MyVec([1.0, 2.0, 9.0])
    @test v != [1.0, 2.0, 9.0]
    @test !isequal(v, [1.0, 2.0, 9.0])

    # Shape mismatch is unequal, not an error.
    @test v != [1.0, 2.0]
    @test !isequal(v, [1.0, 2.0, 3.0, 4.0])
end

@testset "SubArray view equality" begin
    w = view([1, 2, 3, 4], 1:3)
    @test isequal(w, [1, 2, 3])
    @test w == [1, 2, 3]
    @test [1, 2, 3] == w
    @test w != [1, 2, 9]
    @test !(w == [1, 2, 9])
end

@testset "AbstractArray-subtype struct dispatches to ::AbstractArray methods" begin
    # The struct's declared supertype reaches AbstractArray only through the
    # built-in grandparent link AbstractVector{T} <: AbstractArray; static
    # dispatch must resolve it instead of raising a MethodError.
    onlyarray(x::AbstractArray) = "abstractarray"
    v = MyVec([1.0, 2.0, 3.0])
    w = view([1, 2, 3, 4], 1:3)
    @test onlyarray(v) == "abstractarray"
    @test onlyarray(w) == "abstractarray"
    @test v isa AbstractArray
    @test w isa AbstractArray
end
end # module Agg_abstractarray_subtype_equality_8229

# ===== source: array/array_accumulate_init_5701.jl =====
module Agg_array_accumulate_init_5701
using Test

# Issue #5701: accumulate(op, A; init=x) seeds the accumulation via the `init`
# keyword (only the positional 3-arg form existed, and type-specific fast paths
# shadowed it without accepting the keyword).

@testset "accumulate with init keyword (Issue #5701)" begin
    @test accumulate(+, [1, 2, 3]; init=10) == [11, 13, 16]
    @test accumulate(*, [1, 2, 3, 4]; init=2) == [2, 4, 12, 48]
    @test accumulate(max, [1, 3, 2, 5]; init=0) == [1, 3, 3, 5]
    @test accumulate(+, 1:3; init=10) == [11, 13, 16]
    @test accumulate(+, [1.0, 2.0, 3.0]; init=0.5) == [1.5, 3.5, 6.5]

    # No init: unchanged (incl. the type-specific fast paths).
    @test accumulate(+, [1, 2, 3]) == [1, 3, 6]
    @test accumulate(*, [1, 2, 3, 4]) == [1, 2, 6, 24]
    @test accumulate(+, Float64[1, 2, 3]) == [1.0, 3.0, 6.0]
end
end # module Agg_array_accumulate_init_5701

# ===== source: array/array_any_preserves_narrow_integer_4646.jl =====
module Agg_array_any_preserves_narrow_integer_4646
using Test

@testset "Array{Any}/Array{Real} preserve boxed numeric values (#4646)" begin
    any_values = Array{Any}(undef, 8)
    any_values[1] = Int8(1)
    any_values[2] = Int16(2)
    any_values[3] = Int32(3)
    any_values[4] = Int64(4)
    any_values[5] = UInt8(5)
    any_values[6] = UInt16(6)
    any_values[7] = UInt32(7)
    any_values[8] = Float32(8)

    @test typeof(any_values) == Vector{Any}
    @test eltype(any_values) == Any
    @test typeof(any_values[1]) == Int8
    @test typeof(any_values[2]) == Int16
    @test typeof(any_values[3]) == Int32
    @test typeof(any_values[4]) == Int64
    @test typeof(any_values[5]) == UInt8
    @test typeof(any_values[6]) == UInt16
    @test typeof(any_values[7]) == UInt32
    @test typeof(any_values[8]) == Float32

    real_values = Array{Real}(undef, 2)
    real_values[1] = Int8(1)
    real_values[2] = Float32(2)

    @test typeof(real_values) == Vector{Real}
    @test eltype(real_values) == Real
    @test typeof(real_values[1]) == Int8
    @test typeof(real_values[2]) == Float32

    float_values = Vector{Float64}(undef, 1)
    float_values[1] = Int32(3)
    @test typeof(float_values[1]) == Float64
    @test float_values[1] == 3.0
end
end # module Agg_array_any_preserves_narrow_integer_4646

# ===== source: array/array_field_access_6804.jl =====
module Agg_array_field_access_6804
using Test

# Issue #6804: array field access (`a.size`, `a.ref`) — Array{T,N} is a Pure
# Julia mutable struct with fields `ref::MemoryRef{T}` and `size::NTuple{N,Int}`,
# so these must resolve to the struct fields. Top-level access already worked
# once arrays became the faithful Array wrapper; the remaining failure was
# `a.size` reached through a function parameter, where the lazy specializer
# mis-typed the wrapper's parametric `size`/`ref` fields and wrongly coerced the
# tuple result. Array field access is now left to the interpreter.

@testset "array .size / .ref top level (Issue #6804)" begin
    a = [1, 2, 3]
    @test a.size == (3,)
    @test typeof(a.size) == Tuple{Int64}
    @test typeof(a.ref) == MemoryRef{Int64}

    m = [1 2; 3 4]
    @test m.size == (2, 2)
    @test typeof(m.size) == Tuple{Int64, Int64}
end

@testset "array .size / .ref through function parameter (Issue #6804)" begin
    f(x) = x.size
    @test f([1, 2, 3]) == (3,)
    @test f([10, 20, 30, 40]) == (4,)
    v = [1, 2, 3]
    @test f(v) == (3,)

    g(x) = x.ref
    @test typeof(g([1, 2, 3])) == MemoryRef{Int64}
    @test typeof(g([1.0, 2.0])) == MemoryRef{Float64}
end

@testset "array .size after operations (Issue #6804)" begin
    v = Int[]
    push!(v, 5)
    push!(v, 6)
    @test v.size == (2,)
    @test [1.0, 2.0, 3.0].size == (3,)
end
end # module Agg_array_field_access_6804

# ===== source: array/array_repeat_inner_outer_5699.jl =====
module Agg_array_repeat_inner_outer_5699
using Test

# Issue #5699: repeat(v; inner=k, outer=m) — repeat each element `inner` times,
# then the whole result `outer` times. Only the positional repeat(v, n) existed.

@testset "repeat(v; inner, outer) (Issue #5699)" begin
    @test repeat([1, 2, 3], inner=2) == [1, 1, 2, 2, 3, 3]
    @test repeat([1, 2, 3], outer=2) == [1, 2, 3, 1, 2, 3]
    @test repeat([1, 2], inner=2, outer=2) == [1, 1, 2, 2, 1, 1, 2, 2]
    @test repeat([1, 2], inner=3) == [1, 1, 1, 2, 2, 2]
    @test repeat(["a", "b"], inner=2) == ["a", "a", "b", "b"]
    @test repeat([1, 2]) == [1, 2]                  # no kwargs: copy
    @test typeof(repeat([1, 2, 3], inner=2)) === Vector{Int64}

    # Positional repeat(v, n) and matrix repeat are unchanged.
    @test repeat([1, 2], 3) == [1, 2, 1, 2, 1, 2]
    @test repeat([1 2; 3 4], 2, 1) == [1 2; 3 4; 1 2; 3 4]
end
end # module Agg_array_repeat_inner_outer_5699

# ===== source: array/array_reverse_subrange_5693.jl =====
module Agg_array_reverse_subrange_5693
using Test

# Issue #5693: reverse(v, start[, stop]) reverses only the subrange [start, stop]
# of a vector (stop defaults to the last index), returning a copy.

@testset "reverse(v, start, stop) reverses a subrange (Issue #5693)" begin
    @test reverse([1, 2, 3, 4], 2, 3) == [1, 3, 2, 4]
    @test reverse([1, 2, 3, 4, 5], 2, 4) == [1, 4, 3, 2, 5]
    @test reverse([10, 20, 30], 1, 3) == [30, 20, 10]
    @test reverse(["a", "b", "c", "d"], 2, 3) == ["a", "c", "b", "d"]
    @test reverse([1, 2, 3], 2, 2) == [1, 2, 3]   # single element: no change

    # 2-arg form: reverse from start to the end.
    @test reverse([1, 2, 3, 4], 2) == [1, 4, 3, 2]
    @test reverse([1, 2, 3, 4], 1) == [4, 3, 2, 1]

    # Non-mutating, and type-preserving.
    v = [1, 2, 3, 4]
    @test reverse(v, 2, 3) == [1, 3, 2, 4]
    @test v == [1, 2, 3, 4]
    @test typeof(reverse([1, 2, 3, 4], 2, 3)) === Vector{Int64}

    # Whole-vector reverse is unchanged.
    @test reverse([1, 2, 3, 4]) == [4, 3, 2, 1]
end
end # module Agg_array_reverse_subrange_5693

# ===== source: array/bitarray_alias_surface_5498.jl =====
module Agg_bitarray_alias_surface_5498
using Test

@testset "BitArray alias surface (Issue #5498)" begin
    @test BitVector === BitArray{1}
    @test BitMatrix === BitArray{2}
    @test BitVector <: AbstractVector{Bool}
    @test BitMatrix <: AbstractMatrix{Bool}

    @test typeof(falses(3)) === BitVector
    @test typeof(trues(3)) === BitVector
    @test typeof(falses(2, 2)) === BitMatrix
    @test typeof(trues(2, 2)) === BitMatrix
    @test typeof(trues()) === BitArray{0}
    @test typeof(falses(2, 1, 2)) === BitArray{3}
    @test typeof(trues(2, 1, 1, 1)) === BitArray{4}

    @test falses(3) == Bool[false, false, false]
    @test trues(2, 2) == reshape(Bool[true, true, true, true], 2, 2)
    @test size(trues()) == ()
    @test size(falses(2, 1, 2)) == (2, 1, 2)

    @test typeof(copy(falses(3))) === BitVector
    @test typeof(copy(falses(2, 2))) === BitMatrix
    @test typeof(similar(falses(3))) === BitVector
    @test typeof(similar(falses(2, 2))) === BitMatrix
    @test typeof(similar(falses(3), Bool)) === BitVector
    @test typeof(similar(falses(3), Bool, 2)) === BitVector
    @test typeof(similar(falses(2, 2), Bool, (2, 1, 1))) === BitArray{3}
    @test typeof(similar(falses(3), Int64)) === Vector{Int64}

    @test typeof([1, 2, 3] .== 2) === BitVector
    @test typeof(reshape([1, 2, 3, 4], 2, 2) .== 2) === BitMatrix
    @test typeof(reshape([0, 1, 0, 2], 2, 1, 2) .== 0) === BitArray{3}
    @test typeof(iszero.([0, 1, 0])) === BitVector
end
end # module Agg_bitarray_alias_surface_5498

# ===== source: array/findall_hof_chain.jl =====
module Agg_findall_hof_chain
# Tests for 1-arg functions with scalar overloads after HOF chains (Issue #2296)
# Verifies that findall(A::Array) is selected over findall(x::Bool) when
# the argument comes from a filter/map chain with compile-time type Any.

using Test

# Helper predicate
ispositive(x) = x > 0

@testset "findall after filter (Issue #2296)" begin
    # Basic case: filter returns Vector{Bool} at runtime but may have Any type at compile time
    bools = filter(x -> x, [false, true, false, true])
    result = findall(bools)
    @test result == [1, 2]

    # Chain with explicit predicate function
    data = [true, false, true, false, true]
    filtered = filter(identity, data)
    indices = findall(filtered)
    @test length(indices) == length(filtered)
    @test indices == [1, 2, 3]

    # Empty filter result
    empty_filtered = filter(x -> false, [true, false, true])
    empty_result = findall(empty_filtered)
    @test length(empty_result) == 0
end

@testset "findall after map (Issue #2296)" begin
    # map returns type-inferred array, test dispatch still works
    nums = [1, -2, 3, -4, 5]
    bool_mapped = map(ispositive, nums)
    result = findall(bool_mapped)
    @test result == [1, 3, 5]

    # map with anonymous function
    mapped = map(x -> x > 0, [-1, 0, 1, 2, -3])
    indices = findall(mapped)
    @test indices == [3, 4]
end

@testset "findall with nested HOF chains (Issue #2296)" begin
    # map on filter result
    data = [1, 2, 3, 4, 5, 6]
    filtered = filter(x -> x > 2, data)  # [3, 4, 5, 6]
    bool_map = map(x -> x % 2 == 0, filtered)  # [false, true, false, true]
    result = findall(bool_map)
    @test result == [2, 4]
end

@testset "Multiple 1-arg functions in chain" begin
    # Verify length, sum, etc. also work with filter results
    bools = [true, false, true, true, false]
    filtered = filter(identity, bools)

    # findall should dispatch to Array version
    idx = findall(filtered)
    @test length(idx) == 3

    # Verify the result is usable
    @test idx[1] == 1
    @test idx[end] == 3
end
end # module Agg_findall_hof_chain

# ===== source: array/hcat_vcat_flatten_elements_7203.jl =====
module Agg_hcat_vcat_flatten_elements_7203
using Test

# Issue #7203: in a matrix/hcat/vcat literal, a row element that is itself an
# array or range (not a scalar) must be flattened/materialized into the result
# the way upstream Julia does, rather than boxed as an `Any` element.

@testset "matrix-literal concatenation flattens array/range elements (#7203)" begin
    g = [1 2 3]

    # hcat: row-matrix + scalar element -> 1x4 Int matrix (not Any[[1 2 3] 4]).
    gh = [g 4]
    @test gh == [1 2 3 4]
    @test typeof(gh) === Matrix{Int64}
    @test size(gh) == (1, 4)
    @test eltype(gh) === Int64

    # hcat: range elements are materialized column-wise.
    r = [1:2 3:4]
    @test r == [1 3; 2 4]
    @test typeof(r) === Matrix{Int64}
    @test size(r) == (2, 2)

    # hcat: space-separated bracketed matrices ([[1 2] [3 4]]) concatenate
    # horizontally instead of raising a BoundsError.
    mm = [[1 2] [3 4]]
    @test mm == [1 2 3 4]
    @test typeof(mm) === Matrix{Int64}
    @test size(mm) == (1, 4)

    # hcat: three bracketed matrices.
    mmm = [[1 2] [3 4] [5 6]]
    @test mmm == [1 2 3 4 5 6]
    @test size(mmm) == (1, 6)

    # vcat: stacking a row-matrix on top of another row.
    v = [g; [4 5 6]]
    @test v == [1 2 3; 4 5 6]
    @test typeof(v) === Matrix{Int64}
    @test size(v) == (2, 3)

    # vcat: range + scalar flattens to a 1-D Vector (not an N x 1 matrix).
    vv = [1:2; 3]
    @test vv == [1, 2, 3]
    @test typeof(vv) === Vector{Int64}
    @test size(vv) == (3,)

    # hvcat: 2x2 grid of 2x2 matrix blocks.
    a = [1 2; 3 4]
    b = [5 6; 7 8]
    c = [9 10; 11 12]
    d = [13 14; 15 16]
    block = [a b; c d]
    @test block == [1 2 5 6; 3 4 7 8; 9 10 13 14; 11 12 15 16]
    @test typeof(block) === Matrix{Int64}
    @test size(block) == (4, 4)

    # Mixed eltype hcat promotes like upstream.
    pm = [1.0 2; 3 4]
    @test pm == [1.0 2.0; 3.0 4.0]
    @test typeof(pm) === Matrix{Float64}

    # Plain scalar matrix literals are unaffected (fast path preserved).
    s = [1 2; 3 4]
    @test s == [1 2; 3 4]
    @test typeof(s) === Matrix{Int64}
    @test size(s) == (2, 2)

    # Scalars before an array element are flattened too.
    pre = [4 g]
    @test pre == [4 1 2 3]
    @test size(pre) == (1, 4)
end
end # module Agg_hcat_vcat_flatten_elements_7203

# ===== source: array/matrix_equality_4653.jl =====
module Agg_matrix_equality_4653
using Test

@testset "typed and boxed matrix equality (#4653)" begin
    @test Int64[1 3; 2 4] == Int64[1 3; 2 4]
    @test Int16[1 3; 2 4] == Int16[1 3; 2 4]
    @test Float32[1 3; 2 4] == Float32[1 3; 2 4]
    @test Any["a" "c"; "b" "d"] == Any["a" "c"; "b" "d"]

    @test Int16[1 3; 2 4] != Int16[1 3; 2 5]
    @test Any["a" "c"; "b" "d"] != Any["a" "c"; "b" "x"]
end
end # module Agg_matrix_equality_4653

# ===== source: array/reduce_dims.jl =====
module Agg_reduce_dims
# Test sum, prod, maximum, minimum, extrema with dims keyword argument

using Test

A = [1.0 2.0 3.0; 4.0 5.0 6.0; 7.0 8.0 9.0]

@testset "sum with dims" begin
    # dims=1: sum each column → 1×3 result
    S1 = sum(A; dims=1)
    @test S1[1, 1] == 12.0  # 1+4+7
    @test S1[1, 2] == 15.0  # 2+5+8
    @test S1[1, 3] == 18.0  # 3+6+9

    # dims=2: sum each row → 3×1 result
    S2 = sum(A; dims=2)
    @test S2[1, 1] == 6.0   # 1+2+3
    @test S2[2, 1] == 15.0  # 4+5+6
    @test S2[3, 1] == 24.0  # 7+8+9
end

@testset "prod with dims" begin
    B = [1.0 2.0; 3.0 4.0]

    # dims=1: product each column
    P1 = prod(B; dims=1)
    @test P1[1, 1] == 3.0   # 1*3
    @test P1[1, 2] == 8.0   # 2*4

    # dims=2: product each row
    P2 = prod(B; dims=2)
    @test P2[1, 1] == 2.0   # 1*2
    @test P2[2, 1] == 12.0  # 3*4
end

@testset "maximum with dims" begin
    # dims=1: maximum each column
    M1 = maximum(A; dims=1)
    @test M1[1, 1] == 7.0
    @test M1[1, 2] == 8.0
    @test M1[1, 3] == 9.0

    # dims=2: maximum each row
    M2 = maximum(A; dims=2)
    @test M2[1, 1] == 3.0
    @test M2[2, 1] == 6.0
    @test M2[3, 1] == 9.0
end

@testset "minimum with dims" begin
    # dims=1: minimum each column
    M1 = minimum(A; dims=1)
    @test M1[1, 1] == 1.0
    @test M1[1, 2] == 2.0
    @test M1[1, 3] == 3.0

    # dims=2: minimum each row
    M2 = minimum(A; dims=2)
    @test M2[1, 1] == 1.0
    @test M2[2, 1] == 4.0
    @test M2[3, 1] == 7.0
end

@testset "extrema with dims" begin
    # dims=1: extrema each column → array of (min, max) tuples
    E1 = extrema(A; dims=1)
    @test E1[1] == (1.0, 7.0)
    @test E1[2] == (2.0, 8.0)
    @test E1[3] == (3.0, 9.0)

    # dims=2: extrema each row → array of (min, max) tuples
    E2 = extrema(A; dims=2)
    @test E2[1] == (1.0, 3.0)
    @test E2[2] == (4.0, 6.0)
    @test E2[3] == (7.0, 9.0)
end
end # module Agg_reduce_dims

# ===== source: array/reducers_finders_pure_julia_6745.jl =====
module Agg_reducers_finders_pure_julia_6745
# Issue #6745: the non-mutating reducers/finders
# collect / findfirst / findall / argmin / argmax / prod / minimum / maximum
# (and array iteration) are pure Julia (base/array.jl). The vestigial Rust
# BuiltinId variants (Prod/Minimum/Maximum/Argmin/Argmax/FindFirst/FindAll)
# were dead (never emitted) and removed; this pins that the pure-Julia dispatch
# keeps matching upstream julia 1.12 across element types.

using Test

@testset "reducers match upstream (Issue #6745)" begin
    a = [3, 1, 2, 5, 4]
    @test prod(a) == 120
    @test minimum(a) == 1
    @test maximum(a) == 5
    @test argmin(a) == 2
    @test argmax(a) == 4

    # narrow integer reductions promote to Int (matching upstream)
    @test prod(Int8[2, 3, 4]) === 24
    @test prod(Int[]) === 1
    @test minimum([2.5, 1.5, 3.5]) === 1.5
    @test maximum(Float32[1, 9, 4]) === 9.0f0
    @test argmin([3.0, 1.0, 2.0]) == 2
    @test maximum(["a", "c", "b"]) == "c"
end

@testset "finders / collect / iterate (Issue #6745)" begin
    a = [3, 1, 2, 5, 4]
    @test findfirst(==(2), a) == 3
    @test findfirst(>(10), a) === nothing
    @test findall(>(2), a) == [1, 4, 5]
    @test collect(1:3) == [1, 2, 3]
    @test collect(x^2 for x in 1:4) == [1, 4, 9, 16]

    # array iteration (for-loop / comprehension) drives the same path
    s = 0
    for x in a
        s += x
    end
    @test s == 15

    # first-class function values keep resolving after the BuiltinId removal
    f = argmax
    @test f([10, 40, 20]) == 2
    @test map(minimum, [[3, 1], [9, 2, 7]]) == [1, 2]
end
end # module Agg_reducers_finders_pure_julia_6745

# ===== source: array/reshape_range_5758.jl =====
module Agg_reshape_range_5758
using Test

# Issue #5758: reshape of a range materializes and reshapes it. Previously failed
# with "reshape: expected Array, got Range". (Julia returns a lazy ReshapedArray;
# sjulia materializes a Matrix — values and display match, which is what we test.)

@testset "reshape(range, dims) (Issue #5758)" begin
    # Column-major fill, varargs dims
    @test reshape(1:6, 2, 3) == [1 3 5; 2 4 6]
    @test reshape(1:6, 3, 2) == [1 4; 2 5; 3 6]

    # Tuple dims
    @test reshape(1:6, (2, 3)) == [1 3 5; 2 4 6]

    # Zero-based and step ranges
    @test reshape(0:5, 2, 3) == [0 2 4; 1 3 5]
    @test reshape(1:2:12, 2, 3) == [1 5 9; 3 7 11]

    # Reshape to a 1×N / N×1
    @test reshape(1:4, 1, 4) == [1 2 3 4]
    @test reshape(1:4, 4, 1) == reshape([1, 2, 3, 4], 4, 1)

    # Result matches reshaping the collected vector
    @test reshape(1:6, 2, 3) == reshape(collect(1:6), 2, 3)

    # Array reshape is unchanged (regression guard)
    @test reshape([1, 2, 3, 4, 5, 6], 2, 3) == [1 3 5; 2 4 6]
end
end # module Agg_reshape_range_5758

# ===== source: array/show_any_vector_prefix_7303.jl =====
module Agg_show_any_vector_prefix_7303
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
end # module Agg_show_any_vector_prefix_7303

# ===== source: array/sort_keywords.jl =====
module Agg_sort_keywords
# sort/sort! keyword arguments: by, rev, lt (Issue #2011)
# In Julia, sort supports keyword arguments for custom comparisons and transforms.

using Test

myabs(x) = abs(x)
mygt(a, b) = a > b

@testset "sort keyword arguments (Issue #2011)" begin
    # Default sort (ascending)
    @test sort([3, 1, 4, 1, 5]) == [1.0, 1.0, 3.0, 4.0, 5.0]

    # rev=true (descending)
    @test sort([3, 1, 4, 1, 5], rev=true) == [5.0, 4.0, 3.0, 1.0, 1.0]

    # by=abs (sort by absolute value)
    @test sort([-3, 1, -2], by=abs) == [1.0, -2.0, -3.0]

    # by + rev combined
    @test sort([-3, 1, -2], by=abs, rev=true) == [-3.0, -2.0, 1.0]

    # by with named function
    @test sort([-3, 1, -2], by=myabs) == [1.0, -2.0, -3.0]

    # lt with named function (custom comparison: descending)
    @test sort([3, 1, 4, 1, 5], lt=mygt) == [5.0, 4.0, 3.0, 1.0, 1.0]

    # sort! in-place with rev
    a = [5, 3, 1, 4, 2]
    sort!(a, rev=true)
    @test a == [5.0, 4.0, 3.0, 2.0, 1.0]

    # sort! in-place with by
    b = [-3, 1, -2, 4]
    sort!(b, by=abs)
    @test b == [1.0, -2.0, -3.0, 4.0]
end
end # module Agg_sort_keywords

# ===== source: array/stack_ragged.jl =====
module Agg_stack_ragged
using Test

# Regression test for Issue #3592:
# `stack([[1,2], [3]])` (ragged input) must raise a DimensionMismatch-style
# user-facing error before any indexing happens, rather than leaking an
# internal "Index [N] out of bounds" runtime error.

@testset "stack ragged validation (#3592)" begin
    # Ragged: shorter second slice — should raise an ErrorException with a
    # dimension-mismatch message (Julia raises DimensionMismatch; we use
    # error(...) since the VM's exception system is simpler).
    @test_throws Exception stack([[1, 2], [3]])

    # Ragged: longer second slice
    @test_throws Exception stack([[1], [2, 3]])

    # Uniform: still works
    m = stack([[1, 2], [3, 4]])
    @test size(m) == (2, 2)
    @test m[1, 1] == 1
    @test m[2, 1] == 2
    @test m[1, 2] == 3
    @test m[2, 2] == 4

    # Single slice: trivially uniform
    s = stack([[1, 2, 3]])
    @test size(s) == (3, 1)
end
end # module Agg_stack_ragged

# ===== source: array/subarray_abstractvector_dispatch_9776.jl =====
module Agg_subarray_abstractvector_dispatch_9776
using Test

array_subarray_eltype_9776(x::AbstractVector{T}) where T = T

array_subarray_dispatch_9776(x::AbstractVector{T}) where T = :absvec
array_subarray_dispatch_9776(x::Vector{T}) where T = :vec

@testset "SubArray dispatches as AbstractVector with bound element type" begin
    a = [1.0, 2.0]
    v = view(a, 1:2)

    @test v isa AbstractVector{Float64}
    @test array_subarray_eltype_9776(v) === Float64
    @test array_subarray_dispatch_9776(a) == :vec
    @test array_subarray_dispatch_9776(v) == :absvec
end
end # module Agg_subarray_abstractvector_dispatch_9776

# ===== source: array/test_ndims_type_5118.jl =====
module Agg_test_ndims_type_5118
# Type-level ndims (Issue #5118): ndims(T) reads the dimension parameter N
# from an array type, and ndims(::Type{<:Number}) == 0. Value forms unchanged.

using Test

@testset "ndims type-level (Issue #5118)" begin
    # Array type forms: ndims(Array{T,N}) === N
    @test ndims(Vector{Int}) === 1
    @test ndims(Matrix{Int}) === 2
    @test ndims(Array{Int,3}) === 3
    @test ndims(Vector{Float64}) === 1
    @test ndims(Matrix{Float64}) === 2
    @test ndims(Array{Float64,4}) === 4
    @test ndims(Array{Bool,5}) === 5

    # Value forms still work and agree with their types
    @test ndims([1, 2, 3]) === 1
    @test ndims([1 2; 3 4]) === 2
    @test ndims(zeros(2, 3, 4)) === 3

    # Type form matches value form
    @test ndims(Vector{Int}) === ndims([1, 2, 3])
    @test ndims(Matrix{Int}) === ndims([1 2; 3 4])

    # ndims(::Type{<:Number}) === 0
    @test ndims(Int) === 0
    @test ndims(Float64) === 0
    @test ndims(Number) === 0
    @test ndims(Bool) === 0

    # Scalar value forms are 0-dimensional
    @test ndims(1) === 0
    @test ndims(3.14) === 0
end
end # module Agg_test_ndims_type_5118

# ===== source: array/vectorof_specificity.jl =====
module Agg_vectorof_specificity
# Tests for VectorOf specificity-based dispatch (Issue #2352)
# Vector{Int64} should be preferred over Vector{Any}

using Test

# Overlapping methods: specific vs general vector
function process_vec(v::Vector{Any})
    return "any"
end

function process_vec(v::Vector{Int64})
    return "int64"
end

function process_vec(v::Vector{Float64})
    return "float64"
end

@testset "VectorOf specificity dispatch (Issue #2352)" begin
    # Specific method should be selected over general
    @test process_vec([1, 2, 3]) == "int64"
    @test process_vec([1.0, 2.0, 3.0]) == "float64"

    # Parametric invariance (Issue #9107): Vector{String} is NOT a subtype of
    # Vector{Any}, so upstream Julia throws MethodError here — no method matches.
    @test_throws MethodError process_vec(["a", "b", "c"])
end

# Test with abstract type hierarchy
function process_num(v::Vector{Any})
    return "any"
end

function process_num(v::Vector{Number})
    return "number"
end

function process_num(v::Vector{Real})
    return "real"
end

function process_num(v::Vector{Int64})
    return "int64"
end

@testset "VectorOf abstract type specificity" begin
    # Most specific concrete type should be selected
    @test process_num([1, 2, 3]) == "int64"
end

# Test that Vector dispatch doesn't interfere with non-Vector
function mixed_dispatch(x::Any)
    return "any_value"
end

function mixed_dispatch(v::Vector{Int64})
    return "vector_int64"
end

@testset "Vector vs non-Vector dispatch" begin
    # Vector should match Vector{Int64}
    @test mixed_dispatch([1, 2, 3]) == "vector_int64"

    # Non-vector should match Any
    @test mixed_dispatch(42) == "any_value"
    @test mixed_dispatch("hello") == "any_value"
    @test mixed_dispatch((1, 2)) == "any_value"
end
end # module Agg_vectorof_specificity

# ===== source: array/vectorof_type_locals.jl =====
module Agg_vectorof_type_locals
# Tests for VectorOf type tracking through reassignment (Issue #2319)
# julia_type_locals should track VectorOf types from variable reassignment

using Test

# Target functions with parametric vector dispatch
function vec_sum(v::Vector{Int64})
    return sum(v)
end

function vec_product(v::Vector{Float64})
    s = 1.0
    for x in v
        s = s * x
    end
    return s
end

# Variable reassignment preserves Int vector type
v1 = [1, 2, 3, 4]
v2 = v1

# Variable reassignment preserves Float vector type
f1 = [2.0, 3.0, 4.0]
f2 = f1

# Chain reassignment
v3 = v2

@testset "VectorOf type preservation through reassignment (Issue #2319)" begin
    # Direct literal (baseline)
    @test vec_sum([1, 2, 3, 4]) == 10
    @test vec_product([2.0, 3.0, 4.0]) == 24.0

    # Variable reassignment preserves type
    @test vec_sum(v2) == 10
    @test vec_product(f2) == 24.0

    # Chain reassignment (v3 = v2 = v1)
    @test vec_sum(v3) == 10
end

# Vector from conditional expression
v4 = if true
    [10, 20, 30]
else
    [40, 50, 60]
end

@testset "VectorOf from conditional expression (Issue #2319)" begin
    @test vec_sum(v4) == 60
end
end # module Agg_vectorof_type_locals

true
