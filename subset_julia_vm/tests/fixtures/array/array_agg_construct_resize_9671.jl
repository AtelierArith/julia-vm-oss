# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 pilot).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: array/deleteat_return_type.jl =====
# Test that deleteat! returns the mutated array, not the removed element (Issue #3468)
# Julia: typeof(deleteat!([1,2,3], 2)) == Vector{Int64}


@testset "array_deleteat_return_type: deleteat! returns mutated array" begin
    a = [1, 2, 3]
    result = deleteat!(a, 2)
    @test typeof(result) == Vector{Int64}
    @test result === a
    @test length(result) == 2
    @test result[1] == 1
    @test result[2] == 3
end

# ===== source: array/fill_non_numeric.jl =====
# Test fill() with non-numeric types (Issue #2177)
# Julia: fill("hello", 3) returns ["hello", "hello", "hello"]


@testset "fill with String" begin
    a = fill("hello", 3)
    @test length(a) == 3
    @test a[1] == "hello"
    @test a[2] == "hello"
    @test a[3] == "hello"
end

@testset "fill with String - different values" begin
    a = fill("world", 4)
    @test length(a) == 4
    @test a[1] == "world"
    @test a[4] == "world"
end

@testset "fill with Bool" begin
    a = fill(true, 3)
    @test length(a) == 3
    @test a[1] == true
    @test a[2] == true
    @test a[3] == true
end

@testset "fill with Int64 (regression)" begin
    a = fill(42, 3)
    @test length(a) == 3
    @test a[1] == 42
    @test a[2] == 42
    @test a[3] == 42
end

@testset "fill with Float64 (regression)" begin
    a = fill(3.14, 3)
    @test length(a) == 3
    @test a[1] == 3.14
    @test a[2] == 3.14
end

# ===== source: array/fill_type_preservation.jl =====

@testset "fill preserves value type" begin
    @test convert(Symbol, :x) == :x

    f32s = fill(Float32(1.5), 3)
    @test eltype(f32s) == Float32
    @test length(f32s) == 3
    @test f32s[1] == Float32(1.5)
    @test f32s[3] == Float32(1.5)

    ints = fill(7, (2, 2))
    @test eltype(ints) == Int64
    @test size(ints) == (2, 2)
    @test ints[2, 2] == 7

    syms = fill(:x, 3)
    @test typeof(syms) == Vector{Symbol}
    @test eltype(syms) == Symbol
    @test syms[1] == :x
    @test syms[3] == :x

    symmat = fill(:y, (2, 2))
    @test typeof(symmat) == Matrix{Symbol}
    @test eltype(symmat) == Symbol
    @test size(symmat) == (2, 2)
    @test symmat[2, 2] == :y

    simsyms = similar(Array{Symbol}, 2)
    simsyms[1] = :a
    @test typeof(simsyms) == Vector{Symbol}
    @test eltype(simsyms) == Symbol
    @test simsyms[1] == :a
end

# ===== source: array/insertdims.jl =====
# insertdims - Insert singleton dimensions (Issue #2153)
# Inverse of dropdims. Based on Julia's base/abstractarraymath.jl


@testset "insertdims - 1D vector" begin
    v = [1.0, 2.0, 3.0]

    # dims=1: vector -> 1×3 row matrix
    row = insertdims(v; dims=1)
    @test size(row, 1) == 1
    @test size(row, 2) == 3
    @test row[1, 1] == 1.0
    @test row[1, 2] == 2.0
    @test row[1, 3] == 3.0

    # dims=2: vector -> 3×1 column matrix
    col = insertdims(v; dims=2)
    @test size(col, 1) == 3
    @test size(col, 2) == 1
    @test col[1, 1] == 1.0
    @test col[2, 1] == 2.0
    @test col[3, 1] == 3.0
end

@testset "insertdims - 2D matrix" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2×3 matrix

    # dims=3: 2×3 -> 2×3×1 array
    B = insertdims(A; dims=3)
    @test ndims(B) == 3
    @test size(B, 1) == 2
    @test size(B, 2) == 3
    @test size(B, 3) == 1
end

@testset "insertdims - roundtrip with dropdims" begin
    v = [10.0, 20.0, 30.0]

    # insertdims then dropdims should give back the original
    row = insertdims(v; dims=1)
    v_back = dropdims(row; dims=1)
    @test length(v_back) == 3
    @test v_back[1] == 10.0
    @test v_back[2] == 20.0
    @test v_back[3] == 30.0

    col = insertdims(v; dims=2)
    v_back2 = dropdims(col; dims=2)
    @test length(v_back2) == 3
    @test v_back2[1] == 10.0
    @test v_back2[2] == 20.0
    @test v_back2[3] == 30.0
end

# ===== source: array/matrix_vector_undef_constructor_7890.jl =====
# Regression for Matrix{T}(undef,...) and Vector{T}(undef,...) constructors
# (Issue #7890)


@testset "Matrix and Vector undef constructors" begin
    # Matrix{T}(undef, m, n)
    m_f64 = Matrix{Float64}(undef, 2, 3)
    @test typeof(m_f64) == Matrix{Float64}
    @test eltype(m_f64) == Float64
    @test size(m_f64) == (2, 3)
    @test length(m_f64) == 6

    # Matrix{T}(undef, m, n) with integer element type
    m_i64 = Matrix{Int64}(undef, 3, 2)
    @test typeof(m_i64) == Matrix{Int64}
    @test size(m_i64) == (3, 2)

    # Matrix with Bool element type
    m_bool = Matrix{Bool}(undef, 2, 2)
    @test typeof(m_bool) == Matrix{Bool}
    @test size(m_bool) == (2, 2)

    # Matrix with Complex element type
    m_complex = Matrix{Complex{Float64}}(undef, 2, 2)
    @test typeof(m_complex) == Matrix{Complex{Float64}}
    @test size(m_complex) == (2, 2)

    # Vector{T}(undef, n) (already supported, keep parity)
    v_f64 = Vector{Float64}(undef, 4)
    @test typeof(v_f64) == Vector{Float64}
    @test length(v_f64) == 4

    # Can write to undef Matrix
    m_f64[1, 1] = 1.0
    m_f64[2, 3] = 6.0
    @test m_f64[1, 1] == 1.0
    @test m_f64[2, 3] == 6.0
end

# ===== source: array/splice_range.jl =====
# Test splice! with range indices (Issue #3481)


@testset "splice! with range indices" begin
    # splice!(a, r) - remove and return elements in range
    a1 = [1, 2, 3, 4, 5]
    removed1 = splice!(a1, 2:4)
    @test removed1 == [2, 3, 4]
    @test length(a1) == 2
    @test a1[1] == 1
    @test a1[2] == 5

    # splice!(a, r) - remove first element via range
    a2 = [10, 20, 30, 40]
    removed2 = splice!(a2, 1:2)
    @test removed2 == [10, 20]
    @test length(a2) == 2
    @test a2[1] == 30
    @test a2[2] == 40

    # splice!(a, r, ins) - remove range and insert replacement
    a3 = [1, 2, 3, 4, 5]
    removed3 = splice!(a3, 2:3, [20, 30, 40])
    @test removed3 == [2, 3]
    @test length(a3) == 6
    @test a3[1] == 1
    @test a3[2] == 20
    @test a3[3] == 30
    @test a3[4] == 40
    @test a3[5] == 4
    @test a3[6] == 5

    # splice!(a, r, ins) - replace with fewer elements (shrinks)
    a4 = [1, 2, 3, 4, 5]
    removed4 = splice!(a4, 2:4, [99])
    @test removed4 == [2, 3, 4]
    @test length(a4) == 3
    @test a4[1] == 1
    @test a4[2] == 99
    @test a4[3] == 5
end

# ===== source: array/typed_array_undef_constructor.jl =====
# Test Vector{T}(undef, n) and Array{T}(undef, dims...) constructors
# (Issue #1586, Issue #4047)


@testset "Typed array undef constructor" begin
    # Vector{Float64}(undef, n)
    v_f64 = Vector{Float64}(undef, 5)
    @test length(v_f64) == 5

    # Vector{Int64}(undef, n)
    v_i64 = Vector{Int64}(undef, 3)
    @test length(v_i64) == 3

    # Vector{Bool}(undef, n)
    v_bool = Vector{Bool}(undef, 4)
    @test length(v_bool) == 4

    # Vector{Complex{Float64}}(undef, n)
    v_complex = Vector{Complex{Float64}}(undef, 2)
    @test length(v_complex) == 2

    # Array{Float64}(undef, m, n) - 2D array
    arr_2d = Array{Float64}(undef, 3, 4)
    @test size(arr_2d) == (3, 4)
    @test length(arr_2d) == 12

    # Array{Int64}(undef, m, n, k) - 3D array
    arr_3d = Array{Int64}(undef, 2, 3, 4)
    @test size(arr_3d) == (2, 3, 4)
    @test length(arr_3d) == 24

    # Array{T}(undef, dims::Tuple) mirrors Julia's boot.jl tuple constructor.
    arr_tuple = Array{Float64}(undef, (2, 3))
    @test typeof(arr_tuple) == Matrix{Float64}
    @test eltype(arr_tuple) == Float64
    @test size(arr_tuple) == (2, 3)
    @test length(arr_tuple) == 6

    # Explicit-rank Array{T,N}(undef, dims::Tuple) unpacks dims as d...
    arr_rank_tuple = Array{Float32,2}(undef, (2, 2))
    @test typeof(arr_rank_tuple) == Matrix{Float32}
    @test eltype(arr_rank_tuple) == Float32
    @test ndims(arr_rank_tuple) == 2
    @test size(arr_rank_tuple) == (2, 2)

    function make_rank_tuple(T, N)
        Array{T,N}(undef, (2, 2))
    end

    arr_runtime_rank = make_rank_tuple(Float32, 2)
    @test typeof(arr_runtime_rank) == Matrix{Float32}
    @test eltype(arr_runtime_rank) == Float32
    @test ndims(arr_runtime_rank) == 2
    @test size(arr_runtime_rank) == (2, 2)

    function make_vec_tuple(T)
        Array{T,1}(undef, (2,))
    end

    arr_runtime_vec = make_vec_tuple(Complex{Float64})
    @test typeof(arr_runtime_vec) == Vector{Complex{Float64}}
    @test eltype(arr_runtime_vec) == Complex{Float64}
    @test ndims(arr_runtime_vec) == 1
    @test length(arr_runtime_vec) == 2

    dims_from_var = (2, 2)
    arr_dims_var = Array{Int64,2}(undef, dims_from_var)
    @test typeof(arr_dims_var) == Matrix{Int64}
    @test size(arr_dims_var) == (2, 2)

    # Can write to undef arrays
    v_f64[1] = 1.5
    v_f64[2] = 2.5
    @test v_f64[1] == 1.5
    @test v_f64[2] == 2.5

    v_i64[1] = 10
    v_i64[2] = 20
    @test v_i64[1] == 10
    @test v_i64[2] == 20
end

# ===== source: array/typed_undef_roundtrip.jl =====
# Write-read round-trip tests for all typed undef array constructors (Issue #1804)
# Pattern: construct -> write -> read -> verify for each element type


@testset "Float64 undef array round-trip" begin
    v = Vector{Float64}(undef, 3)
    @test length(v) == 3

    v[1] = 1.5
    v[2] = 2.5
    v[3] = 3.5

    @test v[1] == 1.5
    @test v[2] == 2.5
    @test v[3] == 3.5

    # Overwrite and verify
    v[2] = 99.0
    @test v[2] == 99.0
    @test v[1] == 1.5  # unchanged
end

@testset "Int64 undef array round-trip" begin
    v = Vector{Int64}(undef, 3)
    @test length(v) == 3

    v[1] = 10
    v[2] = 20
    v[3] = 30

    @test v[1] == 10
    @test v[2] == 20
    @test v[3] == 30

    # Overwrite and verify
    v[1] = 999
    @test v[1] == 999
    @test v[3] == 30  # unchanged
end

@testset "Bool undef array allocation" begin
    v = Vector{Bool}(undef, 4)
    @test length(v) == 4
    # Bool undef arrays are initialized to false
    @test v[1] == false
    @test v[4] == false
end

@testset "Complex{Float64} undef array round-trip" begin
    v = Vector{Complex{Float64}}(undef, 3)
    @test length(v) == 3

    v[1] = Complex(1.0, 2.0)
    v[2] = Complex(3.0, 4.0)
    v[3] = Complex(5.0, 6.0)

    @test real(v[1]) == 1.0
    @test imag(v[1]) == 2.0
    @test real(v[2]) == 3.0
    @test imag(v[2]) == 4.0
    @test real(v[3]) == 5.0
    @test imag(v[3]) == 6.0

    # Overwrite and verify
    v[2] = Complex(30.0, 40.0)
    @test real(v[2]) == 30.0
    @test imag(v[2]) == 40.0
    @test real(v[1]) == 1.0  # unchanged
    @test imag(v[3]) == 6.0  # unchanged
end

@testset "2D Float64 undef array round-trip" begin
    arr = Array{Float64}(undef, 2, 3)
    @test size(arr) == (2, 3)

    arr[1, 1] = 1.0
    arr[2, 1] = 2.0
    arr[1, 2] = 3.0
    arr[2, 2] = 4.0
    arr[1, 3] = 5.0
    arr[2, 3] = 6.0

    @test arr[1, 1] == 1.0
    @test arr[2, 1] == 2.0
    @test arr[1, 2] == 3.0
    @test arr[2, 2] == 4.0
    @test arr[1, 3] == 5.0
    @test arr[2, 3] == 6.0
end

# ===== source: array/vcat_hcat_varargs.jl =====
# Test vcat/hcat with 3+ arguments (Issue #2169)
# Julia supports varargs: vcat(a, b, c, ...) and hcat(a, b, c, ...)


@testset "vcat with 3 arguments" begin
    r = vcat([1, 2], [3, 4], [5, 6])
    @test length(r) == 6
    @test r[1] == 1
    @test r[3] == 3
    @test r[5] == 5
    @test r[6] == 6
end

@testset "vcat with 4 arguments" begin
    r = vcat([1], [2], [3], [4])
    @test length(r) == 4
    @test r[1] == 1
    @test r[4] == 4
end

@testset "vcat with 2 arguments (regression)" begin
    r = vcat([1, 2], [3, 4])
    @test length(r) == 4
    @test r[1] == 1
    @test r[4] == 4
end

@testset "hcat with 3 arguments" begin
    r = hcat([1, 2], [3, 4], [5, 6])
    @test size(r) == (2, 3)
    @test r[1, 1] == 1.0
    @test r[2, 1] == 2.0
    @test r[1, 3] == 5.0
    @test r[2, 3] == 6.0
end

@testset "hcat with 4 arguments" begin
    r = hcat([1, 2], [3, 4], [5, 6], [7, 8])
    @test size(r) == (2, 4)
    @test r[1, 1] == 1.0
    @test r[1, 4] == 7.0
    @test r[2, 4] == 8.0
end

@testset "hcat with 2 arguments (regression)" begin
    r = hcat([1, 2], [3, 4])
    @test size(r) == (2, 2)
    @test r[1, 1] == 1.0
    @test r[2, 2] == 4.0
end

true
