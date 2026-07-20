# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 pilot).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: array/array_utilities.jl =====
# Array utility functions test - Pure Julia implementations
# Tests for reverse, reverse!, copy, fill!, circshift, circshift!


@testset "Array utility functions" begin
    @testset "reverse (non-mutating)" begin
        # Basic reverse
        arr = [1, 2, 3, 4, 5]
        rev = reverse(arr)
        @test rev == [5, 4, 3, 2, 1]
        @test arr == [1, 2, 3, 4, 5]  # Original unchanged

        # Single element
        @test reverse([42]) == [42]

        # Float array
        arr_f = [1.0, 2.0, 3.0]
        @test reverse(arr_f) == [3.0, 2.0, 1.0]
    end

    @testset "reverse! (mutating)" begin
        # Basic in-place reverse
        arr = [1, 2, 3, 4, 5]
        reverse!(arr)
        @test arr == [5, 4, 3, 2, 1]

        # Even length array
        arr2 = [1, 2, 3, 4]
        reverse!(arr2)
        @test arr2 == [4, 3, 2, 1]

        # Single element
        arr3 = [42]
        reverse!(arr3)
        @test arr3 == [42]

        # reverse! with range
        arr4 = [1, 2, 3, 4, 5]
        reverse!(arr4, 2, 4)  # Reverse indices 2 to 4
        @test arr4 == [1, 4, 3, 2, 5]
    end

    @testset "copy (non-mutating)" begin
        # Basic copy
        arr = [1, 2, 3]
        arr_copy = copy(arr)
        @test arr_copy == arr

        # Verify it's a new array (modification doesn't affect original)
        arr_copy[1] = 100
        @test arr[1] == 1
        @test arr_copy[1] == 100

        # Float array copy
        arr_f = [1.5, 2.5, 3.5]
        @test copy(arr_f) == arr_f
    end

    @testset "fill! (mutating)" begin
        # Fill with integer
        arr = [1, 2, 3, 4, 5]
        fill!(arr, 0)
        @test arr == [0, 0, 0, 0, 0]

        # Fill with float
        arr_f = [1.0, 2.0, 3.0]
        fill!(arr_f, 3.14)
        @test arr_f == [3.14, 3.14, 3.14]

        # Fill returns the modified array
        arr2 = [1, 2, 3]
        result = fill!(arr2, 42)
        @test result == [42, 42, 42]
        @test result === arr2  # Same array reference
    end

    @testset "circshift (non-mutating)" begin
        # Shift right by positive k
        arr = [1, 2, 3, 4, 5]
        @test circshift(arr, 1) == [5, 1, 2, 3, 4]
        @test circshift(arr, 2) == [4, 5, 1, 2, 3]
        @test arr == [1, 2, 3, 4, 5]  # Original unchanged

        # Shift left by negative k
        @test circshift(arr, -1) == [2, 3, 4, 5, 1]
        @test circshift(arr, -2) == [3, 4, 5, 1, 2]

        # No shift
        @test circshift(arr, 0) == [1, 2, 3, 4, 5]

        # Full cycle (shift by length)
        @test circshift(arr, 5) == [1, 2, 3, 4, 5]

        # Shift more than length (wraps around)
        @test circshift(arr, 7) == [4, 5, 1, 2, 3]  # Same as shift by 2
    end

    @testset "circshift! (mutating)" begin
        # Shift right
        arr = [1, 2, 3, 4, 5]
        circshift!(arr, 1)
        @test arr == [5, 1, 2, 3, 4]

        # Shift left
        arr2 = [1, 2, 3, 4, 5]
        circshift!(arr2, -2)
        @test arr2 == [3, 4, 5, 1, 2]

        # Returns modified array
        arr3 = [1, 2, 3]
        result = circshift!(arr3, 1)
        @test result === arr3
    end
end

# ===== source: array/irrational_array_literal_promote_9511.jl =====

@testset "Irrational array literal promotion" begin
    float_pi = [0.0, pi]
    @test typeof(float_pi) == Vector{Float64}
    @test float_pi == [0.0, Float64(pi)]
    @test float_pi[2] == Float64(pi)

    int_pi = [1, pi]
    @test typeof(int_pi) == Vector{Float64}
    @test int_pi == [1.0, Float64(pi)]

    bool_pi = [true, pi]
    @test typeof(bool_pi) == Vector{Float64}
    @test bool_pi == [1.0, Float64(pi)]

    eu = ℯ
    mixed_irrationals = [pi, eu]
    @test typeof(mixed_irrationals) == Vector{Float64}
    @test mixed_irrationals == [Float64(pi), Float64(eu)]

    same_pi = [pi, pi]
    @test eltype(same_pi) == typeof(pi)
    @test typeof(same_pi) == Vector{typeof(pi)}
end

# ===== source: array/multi_comprehension.jl =====
# Multi-variable array comprehension (Issue #2143)
# Tests [expr for var1 in iter1, var2 in iter2, ...] syntax
# Julia uses column-major order: first index (i) varies fastest


@testset "Multi-variable comprehension" begin
    # Basic two-variable comprehension (column-major: i varies fastest)
    result = [i * j for i in 1:3, j in 1:4]
    @test length(result) == 12
    # Column-major order: (1,1),(2,1),(3,1),(1,2),(2,2),(3,2),(1,3),(2,3),(3,3),(1,4),(2,4),(3,4)
    @test result[1] == 1   # 1*1
    @test result[2] == 2   # 2*1
    @test result[3] == 3   # 3*1
    @test result[4] == 2   # 1*2
    @test result[5] == 4   # 2*2
    @test result[6] == 6   # 3*2
    @test result[9] == 9   # 3*3
    @test result[12] == 12 # 3*4

    # Two-variable comprehension with addition
    sums = [i + j for i in 1:2, j in 1:3]
    @test length(sums) == 6
    # Column-major: (1,1),(2,1),(1,2),(2,2),(1,3),(2,3)
    @test sums[1] == 2  # 1+1
    @test sums[2] == 3  # 2+1
    @test sums[3] == 3  # 1+2
    @test sums[4] == 4  # 2+2
    @test sums[5] == 4  # 1+3
    @test sums[6] == 5  # 2+3

    # Three-variable comprehension
    result3 = [i + j + k for i in 1:2, j in 1:2, k in 1:2]
    @test length(result3) == 8
    # Column-major: i fastest, then j, then k
    @test result3[1] == 3  # 1+1+1
    @test result3[2] == 4  # 2+1+1
    @test result3[3] == 4  # 1+2+1
    @test result3[4] == 5  # 2+2+1
    @test result3[5] == 4  # 1+1+2
    @test result3[6] == 5  # 2+1+2
    @test result3[7] == 5  # 1+2+2
    @test result3[8] == 6  # 2+2+2
end

# ===== source: array/nested_array_wrapper_typeinfo_prefix_6882.jl =====
# Issue #6882: a Vector whose elements are Memory-backed `Array{T,N}` *wrappers*
# (built via the typed `T[...]` / `T[]` forms) must print the bare `[...]` form
# when the inner element type is implicit (`Int64`/`Float64`/`Char`/`String`/
# `Symbol`), not a spurious `Array{T, N}[...]` typeinfo prefix. The native-array
# carrier and the wrapper representation must display identically.
#
# Verified against upstream Julia 1.12.6.


@testset "nested_array_wrapper_typeinfo_prefix_6882: typed-form inner arrays" begin
    @test string([Int[1], Int[2]]) == "[[1], [2]]"
    @test string([Int[], Int[]]) == "[Int64[], Int64[]]"
    @test string([Float64[1.0], Float64[2.0]]) == "[[1.0], [2.0]]"
    @test string([Int[1, 2], Int[3, 4]]) == "[[1, 2], [3, 4]]"
end

@testset "nested_array_wrapper_typeinfo_prefix_6882: mixed plain + typed" begin
    @test string([[1], Int[]]) == "[[1], Int64[]]"
    @test string([[1, 2], Int[]]) == "[[1, 2], Int64[]]"
end

@testset "nested_array_wrapper_typeinfo_prefix_6882: plain literals still bare" begin
    @test string([[1, 2], [3, 4]]) == "[[1, 2], [3, 4]]"
    @test string([[1.0, 2.0], [3.0, 4.0]]) == "[[1.0, 2.0], [3.0, 4.0]]"
end

# Note: a *non-implicit* inner eltype (e.g. `[Int8[1], Int8[2]]`) is left for a
# follow-up. After this fix the outer prefix is correct (`Vector{Int8}[...]`),
# but sjulia does not yet propagate the typeinfo context into nested element
# formatting, so the inner arrays still print `Int8[1]` instead of the bare `[1]`
# upstream emits under the propagated `Vector{Int8}` typeinfo. That nested
# typeinfo propagation is a separate, deeper formatter change.

# ===== source: array/repeat_type_preservation.jl =====

# Regression test for Issue #3587:
# `repeat(arr, n)` previously hard-coded `Vector{Float64}` output via
# `zeros(len * n)`, widening any non-Float input. Now uses push! onto an
# empty `[]` so the values are preserved (element type is Any until the
# deeper VM type-preservation infra in #3648 lands).

@testset "repeat preserves values for Int (#3587)" begin
    x = repeat([1, 2], 2)
    @test x == [1, 2, 1, 2]
    @test length(x) == 4
end

@testset "repeat preserves values for Bool" begin
    x = repeat([true, false], 2)
    @test x == [true, false, true, false]
end

@testset "repeat preserves values for String" begin
    x = repeat(["a", "b"], 3)
    @test x == ["a", "b", "a", "b", "a", "b"]
end

@testset "repeat regression for Float64" begin
    x = repeat([1.0, 2.0], 2)
    @test x == [1.0, 2.0, 1.0, 2.0]
end

@testset "repeat edge cases" begin
    @test repeat([1, 2], 0) == []
    @test repeat([1], 1) == [1]
    @test repeat(Int[], 5) == []
end

@testset "repeat(arr, m, n) preserves matrix element type (#3761)" begin
    vi = repeat([1, 2], 2, 3)
    @test typeof(vi) === Matrix{Int64}
    @test size(vi) == (4, 3)
    @test vi == [1 1 1; 2 2 2; 1 1 1; 2 2 2]

    vb = repeat([true, false], 2, 2)
    @test typeof(vb) === Matrix{Bool}
    @test size(vb) == (4, 2)

    mi = repeat([1 2; 3 4], 2, 2)
    @test typeof(mi) === Matrix{Int64}
    @test size(mi) == (4, 4)
    @test mi[1, 1] == 1
    @test mi[3, 3] == 1
    @test mi[4, 4] == 4
end

# ===== source: array/scalar_ndims_length.jl =====
# Test ndims and length for scalar Number types (Issue #2171)
# Julia: ndims(x::Number) = 0, length(x::Number) = 1


@testset "ndims for integers" begin
    @test ndims(42) == 0
    @test ndims(Int64(0)) == 0
    @test ndims(-10) == 0
end

@testset "ndims for floats" begin
    @test ndims(3.14) == 0
    @test ndims(0.0) == 0
    @test ndims(Float32(1.5)) == 0
end

@testset "ndims for Bool" begin
    @test ndims(true) == 0
    @test ndims(false) == 0
end

@testset "ndims for arrays (regression)" begin
    @test ndims([1, 2, 3]) == 1
    @test ndims([1 2; 3 4]) == 2
end

@testset "length for integers" begin
    @test length(42) == 1
    @test length(Int64(0)) == 1
    @test length(-10) == 1
end

@testset "length for floats" begin
    @test length(3.14) == 1
    @test length(0.0) == 1
    @test length(Float32(1.5)) == 1
end

@testset "length for Bool" begin
    @test length(true) == 1
    @test length(false) == 1
end

@testset "length for arrays (regression)" begin
    @test length([1, 2, 3]) == 3
    @test length([1 2; 3 4]) == 4
end

# ===== source: array/scalar_size.jl =====
# Test size(::Number) returns empty tuple for scalars (Issue #2179)
# Julia: size(::Number) = (), consistent with ndims(::Number) = 0


@testset "size(::Number) returns empty tuple" begin
    @test size(42) == ()
    @test size(3.14) == ()
    @test size(true) == ()
    @test size(Float32(1.0)) == ()
end

@testset "size(::Number) length is 0" begin
    @test length(size(42)) == 0
    @test length(size(3.14)) == 0
end

@testset "size for arrays (regression)" begin
    @test size([1, 2, 3]) == (3,)
    @test size([1 2; 3 4]) == (2, 2)
    @test size([1, 2, 3], 1) == 3
end

# ===== source: array/search_operations.jl =====

# Note: operator partial application syntax ==(x), >(x) is not yet supported (Issue #3119).
# Use explicit lambdas instead: x -> x == val, x -> x > val

@testset "Array search operations" begin
    @testset "findfirst" begin
        @test findfirst(x -> x == 3, [1, 2, 3, 4, 5]) == 3
        @test findfirst(x -> x == 3, [3, 2, 3]) == 1  # first occurrence
        @test findfirst(x -> x > 3, [1, 2, 3, 4, 5]) == 4
        @test isnothing(findfirst(x -> x == 99, [1, 2, 3]))
    end

    @testset "findlast" begin
        @test findlast(x -> x == 3, [1, 2, 3, 4, 3]) == 5  # last occurrence
        @test findlast(x -> x == 3, [3, 2, 1]) == 1
        @test isnothing(findlast(x -> x == 99, [1, 2, 3]))
    end

    @testset "findall" begin
        @test findall(x -> x == 2, [1, 2, 3, 2, 2]) == [2, 4, 5]
        @test findall(x -> x > 3, [1, 2, 3, 4, 5]) == [4, 5]
        @test isempty(findall(x -> x == 99, [1, 2, 3]))
    end

    @testset "count" begin
        @test count(x -> x == 2, [1, 2, 3, 2, 2]) == 3
        @test count(x -> x > 3, [1, 2, 3, 4, 5]) == 2
        @test count(x -> x == 99, [1, 2, 3]) == 0
    end

    @testset "filter" begin
        @test filter(x -> x > 3, [1, 2, 3, 4, 5]) == [4, 5]
        @test filter(iseven, [1, 2, 3, 4, 6]) == [2, 4, 6]
        @test isempty(filter(x -> x == 5, [1, 2, 3]))
    end
end

# ===== source: array/similar_array_type_dispatch.jl =====

@testset "similar Array type dispatch" begin
    v = similar(Array{Int64}, (3,))
    @test size(v) == (3,)
    @test length(v) == 3
    v[1] = 10
    v[2] = 20
    v[3] = 30
    @test v[2] == 20

    w = similar(Array{Int64}, 2)
    @test size(w) == (2,)
    w[1] = 7
    @test w[1] == 7

    m = similar(Array{Float64}, 2, 2)
    @test size(m) == (2, 2)
    m[2, 2] = 1
    @test m[2, 2] == 1.0
end

# ===== source: array/similar_array_type_preservation.jl =====

@testset "similar(Array{T}, dims...) preserves element type" begin
    ints = similar(Array{Int64}, 2)
    @test typeof(ints) == Vector{Int64}
    @test eltype(ints) == Int64
    @test length(ints) == 2

    f32s = similar(Array{Float32}, (2,))
    @test typeof(f32s) == Vector{Float32}
    @test eltype(f32s) == Float32
    @test length(f32s) == 2

    syms = similar(Array{Symbol}, 2)
    @test typeof(syms) == Vector{Symbol}
    @test eltype(syms) == Symbol
    @test length(syms) == 2

    mem = Memory{Int64}(undef, 2)
    wrapped = wrap(Array, mem, (2,))
    @test eltype(wrapped) == Int64
end

# ===== source: array/similar_dispatch_any_dims.jl =====
# Regression fixture for Issue #3777.
#
# The compile-time `similar` dispatch in compile/expr/call/mod.rs previously
# only routed to the builtin when the dim args inferred as a fixed integer
# width. Two common patterns inside a function body fell through to method
# dispatch and errored at runtime with "No method matching similar(...)":
#
#   1. Inline `similar(mat, size(mat, 1), size(mat, 2))` — `BuiltinOp::Size`
#      defaulted to F64 in the Builtin inference table.
#   2. `similar(arr, length(arr) * n)` where `n` is an Any-typed param —
#      `I64 * Any` infers as `Any`.
#
# Both were silently broken before the fix.


@testset "similar(mat, inline size(...), size(...))" begin
    function f(mat)
        similar(mat, size(mat, 1), size(mat, 2))
    end

    a = [1 2 3; 4 5 6]
    r = f(a)
    @test typeof(r) === Matrix{Int64}
    @test size(r) == (2, 3)

    b = [1.0 2.0; 3.0 4.0]
    s = f(b)
    @test typeof(s) === Matrix{Float64}
    @test size(s) == (2, 2)
end

@testset "similar(arr, length(arr) * n) — Any × Any arithmetic in dim" begin
    function g(arr, n)
        similar(arr, length(arr) * n)
    end

    r = g([1, 2, 3], 4)
    @test typeof(r) === Vector{Int64}
    @test length(r) == 12

    s = g([1.0, 2.0], 3)
    @test typeof(s) === Vector{Float64}
    @test length(s) == 6
end

@testset "similar(arr, total) — local Any value" begin
    function h(arr, n)
        len = length(arr)
        total = len * n
        similar(arr, total)
    end

    r = h([true, false], 5)
    @test typeof(r) === Vector{Bool}
    @test length(r) == 10
end

@testset "repeat(arr, n) round-trip type preservation" begin
    # Pure Julia `repeat` was migrated to similar(arr, total) once #3777 landed.
    @test typeof(repeat([1, 2], 3)) === Vector{Int64}
    @test repeat([1, 2], 3) == [1, 2, 1, 2, 1, 2]
    @test typeof(repeat([true, false], 2)) === Vector{Bool}
    @test typeof(repeat(["a"], 3)) === Vector{String}
end

# ===== source: array/similar_nothing_eltype_8387.jl =====

@testset "similar with Nothing element type preserves Vector{Nothing}" begin
    empty = similar(Float64[], Nothing)
    @test typeof(empty) == Vector{Nothing}
    @test empty isa Vector{Nothing}
    @test eltype(empty) == Nothing
    @test length(empty) == 0

    sized = similar(Float64[], Nothing, 2)
    @test typeof(sized) == Vector{Nothing}
    @test eltype(sized) == Nothing
    @test length(sized) == 2
    sized[1] = nothing
    @test sized[1] === nothing
end

# ===== source: array/stride_strides.jl =====
# stride and strides - Memory stride for column-major arrays (Issue #2157)


@testset "stride - 1D vector" begin
    v = [1.0, 2.0, 3.0, 4.0]
    @test stride(v, 1) == 1
    # Beyond ndims: stride equals total length
    @test stride(v, 2) == 4
end

@testset "stride - 2D matrix" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix
    @test stride(A, 1) == 1
    @test stride(A, 2) == 2   # size(A, 1) = 2
    # Beyond ndims: stride equals total elements
    @test stride(A, 3) == 6   # 2 * 3
end

@testset "stride - 3D array" begin
    B = zeros(2, 3, 4)
    @test stride(B, 1) == 1
    @test stride(B, 2) == 2   # size(B, 1) = 2
    @test stride(B, 3) == 6   # size(B, 1) * size(B, 2) = 2 * 3
end

@testset "strides - 1D vector" begin
    v = [1.0, 2.0, 3.0]
    s = strides(v)
    @test s[1] == 1
end

@testset "strides - 2D matrix" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix
    s = strides(A)
    @test s[1] == 1
    @test s[2] == 2
end

@testset "strides - 3D array" begin
    B = zeros(2, 3, 4)
    s = strides(B)
    @test s[1] == 1
    @test s[2] == 2
    @test s[3] == 6
end

# ===== source: array/string_array_ops.jl =====
# Test String array operations (Issue #811)
# - setindex! on String arrays
# - reverse on String arrays
# - reverse! on String arrays


@testset "String array setindex!" begin
    # Basic element assignment
    arr = ["a", "b", "c"]
    arr[1] = "x"
    # Use string() to ensure correct type comparison
    @test string(arr[1]) == "x"
    @test string(arr[2]) == "b"
    @test string(arr[3]) == "c"

    # Middle element assignment
    arr[2] = "y"
    @test string(arr[1]) == "x"
    @test string(arr[2]) == "y"
    @test string(arr[3]) == "c"

    # Last element assignment
    arr[3] = "z"
    @test string(arr[1]) == "x"
    @test string(arr[2]) == "y"
    @test string(arr[3]) == "z"
end

@testset "String array reverse" begin
    # reverse (non-mutating)
    arr = ["a", "b", "c"]
    rev = reverse(arr)
    @test string(rev[1]) == "c"
    @test string(rev[2]) == "b"
    @test string(rev[3]) == "a"
    # Original unchanged
    @test string(arr[1]) == "a"
    @test string(arr[2]) == "b"
    @test string(arr[3]) == "c"
end

@testset "String array reverse!" begin
    # reverse! (in-place)
    arr = ["a", "b", "c"]
    reverse!(arr)
    @test string(arr[1]) == "c"
    @test string(arr[2]) == "b"
    @test string(arr[3]) == "a"

    # Single element array
    single = ["only"]
    reverse!(single)
    @test string(single[1]) == "only"

    # Two element array
    two = ["first", "second"]
    reverse!(two)
    @test string(two[1]) == "second"
    @test string(two[2]) == "first"
end

# ===== source: array/string_array_type_preservation_4278.jl =====

@testset "similar preserves String element type for literal vectors (Issue #4278)" begin
    v = ["a", "b"]

    same_len = similar(v)
    @test typeof(same_len) === Vector{String}
    @test eltype(same_len) === String
    @test length(same_len) == 2

    resized = similar(v, 3)
    @test typeof(resized) === Vector{String}
    @test eltype(resized) === String
    @test length(resized) == 3
end

@testset "repeat preserves String element type for literal vectors (Issue #4278)" begin
    v = ["a", "b"]

    r = repeat(v, 2)
    @test typeof(r) === Vector{String}
    @test r == ["a", "b", "a", "b"]

    tiled = repeat(v, 2, 2)
    @test typeof(tiled) === Matrix{String}
    @test size(tiled) == (4, 2)
    @test tiled == ["a" "a"; "b" "b"; "a" "a"; "b" "b"]
end

@testset "repeat preserves String element type for literal matrices (Issue #4278)" begin
    m = ["a" "b"; "c" "d"]
    r = repeat(m, 2, 1)

    @test typeof(r) === Matrix{String}
    @test size(r) == (4, 2)
    @test r == ["a" "b"; "c" "d"; "a" "b"; "c" "d"]
end

@testset "permutedims preserves String element type for literal arrays (Issue #4278)" begin
    v = ["a", "b"]
    row = permutedims(v)
    @test typeof(row) === Matrix{String}
    @test size(row) == (1, 2)
    @test row == ["a" "b"]

    m = ["a" "b"; "c" "d"]
    transposed = permutedims(m)
    @test typeof(transposed) === Matrix{String}
    @test size(transposed) == (2, 2)
    @test transposed == ["a" "c"; "b" "d"]

    copied = permutedims(m, (1, 2))
    @test typeof(copied) === Matrix{String}
    @test size(copied) == (2, 2)
    @test copied == m
end

# ===== source: array/test_array_memory_creation.jl =====
# Test that Array creation functions use Memory internally (Issue #2762)
# These tests verify zeros/ones/similar produce correct results after
# the builtin migration from direct ArrayValue to Memory-based allocation.


@testset "Array creation via Memory pipeline" begin
    # zeros - F64 default
    z1 = zeros(3)
    @test length(z1) == 3
    @test z1[1] == 0.0
    @test z1[2] == 0.0
    @test z1[3] == 0.0
    @test eltype(z1) == Float64

    # zeros - 2D
    z2 = zeros(2, 3)
    @test size(z2) == (2, 3)
    @test z2[1, 1] == 0.0
    @test z2[2, 3] == 0.0

    # zeros - typed Int64
    zi = zeros(Int64, 4)
    @test length(zi) == 4
    @test zi[1] == 0
    @test eltype(zi) == Int64

    # ones - F64 default
    o1 = ones(3)
    @test length(o1) == 3
    @test o1[1] == 1.0
    @test o1[2] == 1.0
    @test o1[3] == 1.0
    @test eltype(o1) == Float64

    # ones - 2D
    o2 = ones(2, 3)
    @test size(o2) == (2, 3)
    @test o2[1, 1] == 1.0
    @test o2[2, 3] == 1.0

    # ones - typed Int64
    oi = ones(Int64, 4)
    @test length(oi) == 4
    @test oi[1] == 1
    @test eltype(oi) == Int64

    # similar - same shape
    a = [1.0, 2.0, 3.0]
    s = similar(a)
    @test length(s) == 3
    @test eltype(s) == Float64

    # similar - new length
    s2 = similar(a, 5)
    @test length(s2) == 5
    @test eltype(s2) == Float64

    # Array{T}(undef, n) - Float64
    uf = Array{Float64}(undef, 3)
    @test length(uf) == 3
    @test eltype(uf) == Float64

    # Array{T}(undef, n) - Int64
    ui = Array{Int64}(undef, 4)
    @test length(ui) == 4
    @test eltype(ui) == Int64

    # Array{T}(undef, n) - Bool
    ub = Array{Bool}(undef, 2)
    @test length(ub) == 2
    @test eltype(ub) == Bool

    # Mutability after creation
    z = zeros(3)
    z[1] = 42.0
    @test z[1] == 42.0
    @test z[2] == 0.0

    o = ones(3)
    push!(o, 4.0)
    @test length(o) == 4
    @test o[4] == 4.0
end

# ===== source: array/test_array_pure_julia.jl =====
# Test Array{T,N} as Pure Julia mutable struct wrapping MemoryRef{T} (Issues #2760/#6648)
# This tests the struct-based Array definition, not the compiler-intercepted path.
# The struct is constructed directly with MemoryRef{T} and a size tuple.
#
# This verifies both direct field access and Pure Julia wrapper methods that
# delegate shape/indexing/mutation to the backing Memory{T}.


@testset "Array{T} struct construction - Int64" begin
    # Create Memory{Int64} and construct Array struct
    mem = Memory{Int64}(undef, 3)
    mem[1] = 10
    mem[2] = 20
    mem[3] = 30
    a = Array{Int64,1}(memoryref(mem), (3,))

    # Verify field access
    @test a.size == (3,)
    @test memoryindex(a.ref) == 1
    @test size(a) == (3,)
    @test size(a, 1) == 3
    @test length(a) == 3
    @test ndims(a) == 1
    @test a[1] == 10
    @test a[2] == 20
    @test a[3] == 30

    # Verify memory field holds correct data
    m = parent(a.ref)
    @test m[1] == 10
    @test m[2] == 20
    @test m[3] == 30
end

@testset "Array{T} struct construction - Float64" begin
    mem = Memory{Float64}(undef, 4)
    mem[1] = 1.5
    mem[2] = 2.5
    mem[3] = 3.5
    mem[4] = 4.5
    a = Array{Float64,1}(memoryref(mem), (4,))

    @test a.size == (4,)
    m = parent(a.ref)
    @test m[1] == 1.5
    @test m[4] == 4.5
end

@testset "Array{T} struct mutability" begin
    mem = Memory{Int64}(undef, 3)
    mem[1] = 1
    mem[2] = 2
    mem[3] = 3
    a = Array{Int64,1}(memoryref(mem), (3,))

    # Mutable struct: can change size field.
    a.size = (1, 3)
    @test a.size == (1, 3)
    @test size(a) == (1, 3)
    @test ndims(a) == 2

    # Memory mutation through field access
    m = parent(a.ref)
    m[2] = 99
    @test parent(a.ref)[2] == 99
    @test a[2] == 99

    a[3] = 123
    @test parent(a.ref)[3] == 123
    @test a[3] == 123
end

@testset "Array{T} struct 2D" begin
    # 2D array: 2x3 matrix (6 elements in column-major Memory)
    mem = Memory{Float64}(undef, 6)
    mem[1] = 1.0
    mem[2] = 2.0
    mem[3] = 3.0
    mem[4] = 4.0
    mem[5] = 5.0
    mem[6] = 6.0
    a = Array{Float64,2}(memoryref(mem), (2, 3))

    @test a.size == (2, 3)
    @test size(a) == (2, 3)
    @test size(a, 1) == 2
    @test size(a, 2) == 3
    @test size(a, 3) == 1
    @test length(a) == 6
    @test ndims(a) == 2
    @test parent(a.ref)[1] == 1.0
    @test parent(a.ref)[6] == 6.0
    @test a[1, 1] == 1.0
    @test a[2, 1] == 2.0
    @test a[1, 2] == 3.0
    @test a[2, 3] == 6.0

    a[1, 3] = 9.5
    @test parent(a.ref)[5] == 9.5
    @test a[1, 3] == 9.5

    # Verify size tuple dimensions
    s = a.size
    @test s[1] == 2
    @test s[2] == 3
end

@testset "wrap Array from Memory" begin
    mem = Memory{Int64}(undef, 4)
    mem[1] = 10
    mem[2] = 20
    mem[3] = 30
    mem[4] = 40

    a = wrap(Array, mem, (2, 2))
    @test size(a) == (2, 2)
    @test length(a) == 4
    @test ndims(a) == 2
    @test a[1, 1] == 10
    @test a[2, 1] == 20
    @test a[1, 2] == 30
    @test a[2, 2] == 40

    a[1, 2] = 99
    @test mem[3] == 99

    mem[4] = 77
    @test a[2, 2] == 77

    v = wrap(Array, mem, 3)
    @test size(v) == (3,)
    @test size(v, 2) == 1
    @test length(v) == 3
    @test v[3] == 99

    full = wrap(Array, mem)
    @test size(full) == (4,)
    @test size(full, 2) == 1
    @test length(full) == 4
    @test full[4] == 77

    @test_throws DimensionMismatch wrap(Array, mem, (3, 2))
    @test_throws BoundsError v[4]
end

@testset "Array{T} struct 3D indexing" begin
    mem = Memory{Int64}(undef, 8)
    for i in 1:8
        mem[i] = i
    end

    a = wrap(Array, mem, (2, 2, 2))
    @test size(a) == (2, 2, 2)
    @test length(a) == 8
    @test ndims(a) == 3
    @test a[1, 1, 1] == 1
    @test a[2, 1, 1] == 2
    @test a[1, 2, 1] == 3
    @test a[2, 2, 1] == 4
    @test a[1, 1, 2] == 5
    @test a[2, 2, 2] == 8

    a[1, 2, 2] = 99
    @test mem[7] == 99
    @test a[1, 2, 2] == 99

    @test_throws BoundsError a[3, 1, 1]
    @test_throws BoundsError a[1, 1]
end

@testset "Array{T} struct reshape shares Memory" begin
    mem = Memory{Int64}(undef, 6)
    for i in 1:6
        mem[i] = i
    end

    a = wrap(Array, mem, (2, 3))
    v = reshape(a, 6)
    @test size(v) == (6,)
    @test length(v) == 6
    @test v[5] == 5

    v[6] = 99
    @test mem[6] == 99
    @test a[2, 3] == 99

    b = reshape(v, 3, 2)
    @test size(b) == (3, 2)
    @test b[3, 2] == 99

    r = wrap(Array, memoryref(mem, 2), (2, 2))
    rr = reshape(r, 4)
    @test size(rr) == (4,)
    @test size(rr, 2) == 1
    @test rr[1] == 2
    @test rr[4] == 5

    rr[4] = 77
    @test mem[5] == 77
    @test r[2, 2] == 77

    @test_throws DimensionMismatch reshape(a, 5)
end

@testset "Array{T} struct similar allocates Memory-backed wrapper" begin
    mem = Memory{Int64}(undef, 6)
    for i in 1:6
        mem[i] = i
    end

    a = wrap(Array, mem, (2, 3))
    b = similar(a)
    @test size(b) == (2, 3)
    @test length(b) == 6
    @test ndims(b) == 2

    b[2, 3] = 42
    @test b[2, 3] == 42
    @test mem[6] == 6

    v = similar(a, 4)
    @test size(v) == (4,)
    @test length(v) == 4
    v[4] = 77
    @test v[4] == 77

    m = similar(a, 3, 2)
    @test size(m) == (3, 2)
    @test length(m) == 6
    m[3, 2] = 99
    @test m[3, 2] == 99

    tf = similar(a, Float64, 2, 2)
    @test size(tf) == (2, 2)
    @test length(tf) == 4
    tf[2, 2] = 1.25
    @test tf[2, 2] == 1.25
    @test typeof(tf[2, 2]) == Float64

    tb = similar(a, Bool)
    @test size(tb) == (2, 3)
    tb[1, 1] = true
    @test tb[1, 1] == true
    @test typeof(tb[1, 1]) == Bool

    r = wrap(Array, memoryref(mem, 2), (2, 2))
    rr = similar(r)
    @test size(rr) == (2, 2)
    rr[2, 2] = 55
    @test rr[2, 2] == 55
    @test r[2, 2] == 5

    @test_throws DimensionMismatch similar(a, -1)
end

@testset "Array{Bool} struct" begin
    mem = Memory{Bool}(undef, 3)
    mem[1] = true
    mem[2] = false
    mem[3] = true
    a = Array{Bool,1}(memoryref(mem), (3,))

    @test a.size == (3,)
    m = parent(a.ref)
    @test m[1] == true
    @test m[2] == false
    @test m[3] == true
end

# ===== source: array/test_ndims.jl =====
# Test ndims function


@testset "ndims - number of dimensions" begin
    # 1D array (Vector)
    v = [1, 2, 3]
    @test ndims(v) == 1

    # 2D array (Matrix)
    m = [1 2 3; 4 5 6]
    @test ndims(m) == 2

    # Using zeros/ones
    @test ndims(zeros(5)) == 1
    @test ndims(zeros(3, 4)) == 2
    @test ndims(ones(2, 3)) == 2
end

# ===== source: array/type_preserving_alloc.jl =====
# Test type-preserving array allocation idioms inside function bodies
# (Issue #3648). Each of `similar(arr, n)`, `collect(arr)`, and
# `Vector{eltype(arr)}(undef, n)` must preserve the element type when
# called from inside a function with an Any-typed parameter.


@testset "similar(arr, n) preserves element type from inside a function" begin
    f(arr) = similar(arr, 2)
    @test typeof(f([1, 2, 3])) == Vector{Int64}
    @test typeof(f([1.0, 2.0, 3.0])) == Vector{Float64}
    @test typeof(f([true, false])) == Vector{Bool}
end

@testset "similar(arr) (no length) preserves element type from inside a function" begin
    g(arr) = similar(arr)
    @test typeof(g([1, 2, 3])) == Vector{Int64}
    @test typeof(g([1.0, 2.0])) == Vector{Float64}
    @test typeof(g([true, false])) == Vector{Bool}
end

@testset "collect(arr) preserves element type from inside a function" begin
    function rev(arr)
        n = length(arr)
        result = collect(arr)
        for i in 1:n
            result[i] = arr[n - i + 1]
        end
        return result
    end
    @test typeof(rev([1, 2, 3])) == Vector{Int64}
    @test rev([1, 2, 3]) == [3, 2, 1]
    @test typeof(rev([1.0, 2.0, 3.0])) == Vector{Float64}
    @test rev([1.0, 2.0, 3.0]) == [3.0, 2.0, 1.0]
    @test typeof(rev([true, false, true])) == Vector{Bool}
end

@testset "collect(arr) returns an independent shape-preserving copy" begin
    src = [1, 2, 3]
    dst = collect(src)
    dst[1] = 99
    @test src == [1, 2, 3]
    @test dst == [99, 2, 3]
    @test typeof(dst) == Vector{Int64}

    empty = collect(Int64[])
    @test typeof(empty) == Vector{Int64}
    @test length(empty) == 0

    mat = [1 2; 3 4]
    mat_copy = collect(mat)
    @test typeof(mat_copy) == Matrix{Int64}
    @test size(mat_copy) == (2, 2)
    @test mat_copy == mat
    mat_copy[1, 2] = 20
    @test mat == [1 2; 3 4]
    @test mat_copy == [1 20; 3 4]

    bools = [true false; false true]
    bool_copy = collect(bools)
    @test typeof(bool_copy) == Matrix{Bool}
    @test size(bool_copy) == (2, 2)
    @test bool_copy == bools
end

@testset "Vector{eltype(arr)}(undef, n) preserves element type" begin
    function f(arr)
        T = eltype(arr)
        result = Vector{T}(undef, 2)
        result[1] = arr[1]
        result[2] = arr[2]
        return result
    end
    @test typeof(f([1, 2, 3])) == Vector{Int64}
    @test f([1, 2, 3]) == [1, 2]
    @test typeof(f([1.0, 2.0, 3.0])) == Vector{Float64}
    @test typeof(f([true, false, true])) == Vector{Bool}
end

@testset "Vector{T}(undef, n) inside where T function" begin
    function fwhere(arr::Vector{T}) where T
        return Vector{T}(undef, 2)
    end
    @test typeof(fwhere([1, 2, 3])) == Vector{Int64}
    @test typeof(fwhere([1.0, 2.0, 3.0])) == Vector{Float64}
    @test typeof(fwhere([true, false])) == Vector{Bool}
end

@testset "reverse([1, 2, 3]) is type-preserving (#3648)" begin
    @test typeof(reverse([1, 2, 3])) == Vector{Int64}
    @test reverse([1, 2, 3]) == [3, 2, 1]
    @test typeof(reverse([1.0, 2.0])) == Vector{Float64}
    @test typeof(reverse([true, false])) == Vector{Bool}
end

@testset "broadcast still works (regression check for similar dispatch)" begin
    x = [1, 2, 3]
    y = x .+ 1
    @test typeof(y) == Vector{Int64}
    @test y == [2, 3, 4]

    a = [1.0, 2.0, 3.0]
    b = a .* 2
    @test typeof(b) == Vector{Float64}
end

# ===== source: array/typed_array_edge_cases.jl =====
# Typed array constructor edge cases
# Tests zero-length, single-element, and high-dimensional arrays.
# Related: Issue #1607


@testset "Zero-length arrays" begin
    v_f64 = Vector{Float64}(undef, 0)
    @test length(v_f64) == 0

    v_i64 = Vector{Int64}(undef, 0)
    @test length(v_i64) == 0

    v_bool = Vector{Bool}(undef, 0)
    @test length(v_bool) == 0
end

@testset "Single-element arrays" begin
    v_f64 = Vector{Float64}(undef, 1)
    @test length(v_f64) == 1
    v_f64[1] = 42.0
    @test v_f64[1] == 42.0

    v_i64 = Vector{Int64}(undef, 1)
    @test length(v_i64) == 1
    v_i64[1] = 99
    @test v_i64[1] == 99
end

@testset "2D array read-write" begin
    arr = Array{Float64}(undef, 3, 3)
    @test size(arr) == (3, 3)
    @test length(arr) == 9

    # Write to all elements
    for i in 1:3
        for j in 1:3
            arr[i, j] = Float64(i * 10 + j)
        end
    end

    # Read back
    @test arr[1, 1] == 11.0
    @test arr[2, 3] == 23.0
    @test arr[3, 3] == 33.0
end

@testset "3D array dimensions" begin
    arr = Array{Int64}(undef, 2, 3, 4)
    @test size(arr) == (2, 3, 4)
    @test length(arr) == 24
end

@testset "zeros and ones typed" begin
    # zeros with type
    z_f64 = zeros(Float64, 3)
    @test length(z_f64) == 3
    @test z_f64[1] == 0.0
    @test z_f64[2] == 0.0
    @test z_f64[3] == 0.0

    z_i64 = zeros(Int64, 4)
    @test length(z_i64) == 4
    @test z_i64[1] == 0

    # ones with type
    o_f64 = ones(Float64, 3)
    @test length(o_f64) == 3
    @test o_f64[1] == 1.0
    @test o_f64[2] == 1.0

    o_i64 = ones(Int64, 2)
    @test length(o_i64) == 2
    @test o_i64[1] == 1
end

@testset "Complex array undef length" begin
    v = Vector{Complex{Float64}}(undef, 3)
    @test length(v) == 3

    v_zero = Vector{Complex{Float64}}(undef, 0)
    @test length(v_zero) == 0
end

# ===== source: array/typed_container_no_widening_5073.jl =====
# Consolidated regression fixture for the typed-container "no widening" umbrella (Issue #5073).
# Pins the type-loss matrix enumerated in the umbrella so future widening regressions are caught.
# Sub-issues #5039 / #5040 / #5041 and the #4646 boxed-numeric cluster are all merged; this
# fixture locks their parity-with-upstream behavior in one place.


@testset "typed allocation keeps declared element type" begin
    @test typeof(zeros(2)) === Vector{Float64}
    @test typeof(zeros(Int8, 2)) === Vector{Int8}
    @test typeof(ones(2)) === Vector{Float64}
    @test typeof(ones(Int8, 3)) === Vector{Int8}
    @test typeof(fill(Int8(3), 2)) === Vector{Int8}
    @test typeof(fill(2.0f0, 3)) === Vector{Float32}
end

@testset "typed allocation (n-dimensional / parametric)" begin
    @test typeof(zeros(Int8, (2, 2))) === Matrix{Int8}
    @test typeof(zeros(Int8, 2, 2)) === Matrix{Int8}
    @test typeof(zeros(Complex{Float64}, 2)) === Vector{Complex{Float64}}
end

@testset "boxed numeric values keep their real type" begin
    @test typeof(Any[Int8(3)][1]) === Int8
    @test typeof(Real[Int8(3)][1]) === Int8
    a = Any[Int8(3)]
    push!(a, Int16(5))
    @test typeof(a[2]) === Int16
    @test eltype(Real[1 // 2, 3]) === Real
    @test typeof(Real[1 // 2, 3][1]) === Rational{Int64}
    @test typeof(Real[1 // 2, 3][2]) === Int64
end

@testset "typed comprehension T[expr ...] converts and keeps T" begin
    @test typeof(Float64[i for i in 1:3]) === Vector{Float64}
    @test typeof(Int8[i for i in 1:3]) === Vector{Int8}
    @test typeof(Bool[x > 0 for x in [-1, 0, 1]]) === Vector{Bool}
    @test Float64[i for i in 1:3] == [1.0, 2.0, 3.0]
end

@testset "Vector{T}(::Tuple) is a MethodError (matches upstream)" begin
    @test_throws MethodError Vector{Int64}((1, 2, 3))
end

# ===== source: array/wrap_memoryref.jl =====

@testset "wrap Array over MemoryRef offset storage" begin
    m = Memory{Int64}(undef, 5)
    for i in 1:5
        m[i] = 10 * i
    end

    r = memoryref(m, 3)
    a = wrap(Array, r, 3)
    @test size(a) == (3,)
    @test length(a) == 3
    @test a[1] == 30
    @test a[2] == 40
    a[2] = 99
    @test m[4] == 99

    r2 = memoryref(m, 2)
    b = wrap(Array, r2, (2, 2))
    @test size(b) == (2, 2)
    @test length(b) == 4
    @test b[1, 1] == 20
    @test b[1, 2] == 99
    b[2, 2] = 77
    @test m[5] == 77

    @test_throws DimensionMismatch wrap(Array, r, (4,))
end

true
