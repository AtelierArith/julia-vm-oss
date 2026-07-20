# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 pilot).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: array/cat_eachslice_mapslices.jl =====
# cat, eachslice, mapslices (Issue #1952)


@testset "cat dims=1 (vertical)" begin
    # 2D matrices
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]
    C = cat(A, B; dims=1)
    @test size(C, 1) == 4
    @test size(C, 2) == 2
    @test abs(C[1, 1] - 1.0) < 1e-10
    @test abs(C[2, 1] - 3.0) < 1e-10
    @test abs(C[3, 1] - 5.0) < 1e-10
    @test abs(C[4, 1] - 7.0) < 1e-10
    @test abs(C[3, 2] - 6.0) < 1e-10
    @test abs(C[4, 2] - 8.0) < 1e-10

    # 1D arrays
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0]
    z = cat(x, y; dims=1)
    @test length(z) == 5
    @test abs(z[1] - 1.0) < 1e-10
    @test abs(z[4] - 4.0) < 1e-10
    @test abs(z[5] - 5.0) < 1e-10
end

@testset "cat dims=2 (horizontal)" begin
    # 2D matrices
    A = [1.0 2.0; 3.0 4.0]
    B = [5.0 6.0; 7.0 8.0]
    D = cat(A, B; dims=2)
    @test size(D, 1) == 2
    @test size(D, 2) == 4
    @test abs(D[1, 1] - 1.0) < 1e-10
    @test abs(D[1, 3] - 5.0) < 1e-10
    @test abs(D[2, 4] - 8.0) < 1e-10

    # 1D arrays as columns
    x = [1.0, 2.0, 3.0]
    y = [4.0, 5.0, 6.0]
    M = cat(x, y; dims=2)
    @test size(M, 1) == 3
    @test size(M, 2) == 2
    @test abs(M[1, 1] - 1.0) < 1e-10
    @test abs(M[1, 2] - 4.0) < 1e-10
    @test abs(M[3, 2] - 6.0) < 1e-10
end

@testset "eachslice dims=1 (rows)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    row_sums = Float64[]
    for row in eachslice(A; dims=1)
        push!(row_sums, sum(row))
    end
    @test length(row_sums) == 2
    @test abs(row_sums[1] - 6.0) < 1e-10   # 1+2+3
    @test abs(row_sums[2] - 15.0) < 1e-10  # 4+5+6
end

@testset "eachslice dims=2 (columns)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    col_sums = Float64[]
    for col in eachslice(A; dims=2)
        push!(col_sums, sum(col))
    end
    @test length(col_sums) == 3
    @test abs(col_sums[1] - 5.0) < 1e-10   # 1+4
    @test abs(col_sums[2] - 7.0) < 1e-10   # 2+5
    @test abs(col_sums[3] - 9.0) < 1e-10   # 3+6
end

@testset "mapslices dims=1 (columns)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    # Sum each column
    col_sums = mapslices(sum, A; dims=1)
    @test length(col_sums) == 3
    @test abs(col_sums[1] - 5.0) < 1e-10   # 1+4
    @test abs(col_sums[2] - 7.0) < 1e-10   # 2+5
    @test abs(col_sums[3] - 9.0) < 1e-10   # 3+6
end

@testset "mapslices dims=2 (rows)" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]
    # Sum each row
    row_sums = mapslices(sum, A; dims=2)
    @test length(row_sums) == 2
    @test abs(row_sums[1] - 6.0) < 1e-10   # 1+2+3
    @test abs(row_sums[2] - 15.0) < 1e-10  # 4+5+6
end

# ===== source: array/findfirst_predicate.jl =====
# findfirst(f, A) - find first index where predicate returns true (Issue #1996)


@testset "findfirst with predicate" begin
    @test findfirst(x -> x > 3, [1, 2, 5, 4]) == 3
end

# ===== source: array/findlast_predicate.jl =====
# findlast(f, A) - find last index where predicate returns true (Issue #1996)


@testset "findlast with predicate" begin
    # Lambda function - basic
    @test findlast(x -> x > 3, [1, 2, 5, 4]) == 4
    @test findlast(x -> x < 0, [-1, 2, -3]) == 3

    # Lambda - first element matches only
    @test findlast(x -> x > 5, [6, 2, 4, 1]) == 1

    # Lambda - last element matches
    @test findlast(x -> x > 6, [2, 4, 6, 7]) == 4

    # No match returns nothing
    @test findlast(x -> x > 10, [2, 4, 6, 8]) === nothing

    # Single element array - match
    @test findlast(x -> x > 0, [3]) == 1

    # Single element array - no match
    @test findlast(x -> x > 5, [3]) === nothing

    # isodd/iseven via lambda
    @test findlast(x -> x % 2 != 0, [2, 4, 3, 6]) == 3
    @test findlast(x -> x % 2 == 0, [1, 3, 4, 5, 6]) == 5

    # Multiple matches - returns last
    @test findlast(x -> x > 2, [1, 3, 5, 2, 4]) == 5
end

# ===== source: array/first_last_n.jl =====
# Test first(arr, n) and last(arr, n) multi-element variants (Issue #1887)


@testset "first(arr, n) basic" begin
    arr = [10, 20, 30, 40, 50]
    @test first(arr, 3) == [10, 20, 30]
    @test first(arr, 1) == [10]
    @test first(arr, 5) == [10, 20, 30, 40, 50]
end

@testset "first(arr, n) edge cases" begin
    arr = [10, 20, 30]
    # n > length returns all elements
    @test first(arr, 10) == [10, 20, 30]
    # n == 0 returns empty
    @test length(first(arr, 0)) == 0
end

@testset "first(arr, n) single element" begin
    @test first([42], 1) == [42]
    @test length(first([42], 0)) == 0
end

@testset "first(arr, n) float" begin
    arr = [1.5, 2.5, 3.5, 4.5]
    @test first(arr, 2) == [1.5, 2.5]
end

@testset "last(arr, n) basic" begin
    arr = [10, 20, 30, 40, 50]
    @test last(arr, 3) == [30, 40, 50]
    @test last(arr, 1) == [50]
    @test last(arr, 5) == [10, 20, 30, 40, 50]
end

@testset "last(arr, n) edge cases" begin
    arr = [10, 20, 30]
    # n > length returns all elements
    @test last(arr, 10) == [10, 20, 30]
    # n == 0 returns empty
    @test length(last(arr, 0)) == 0
end

@testset "last(arr, n) single element" begin
    @test last([42], 1) == [42]
    @test length(last([42], 0)) == 0
end

@testset "last(arr, n) float" begin
    arr = [1.5, 2.5, 3.5, 4.5]
    @test last(arr, 2) == [3.5, 4.5]
end

# ===== source: array/isassigned_basic.jl =====
# Test isassigned(array, i) for index assignment check (Issue #1836)
# In SubsetJuliaVM, all isbits array elements are always assigned,
# so isassigned is effectively a bounds check.


@testset "isassigned basic" begin
    arr = [10, 20, 30, 40, 50]

    # Valid indices return true
    @test isassigned(arr, 1) == true
    @test isassigned(arr, 3) == true
    @test isassigned(arr, 5) == true

    # Out of bounds return false
    @test isassigned(arr, 0) == false
    @test isassigned(arr, 6) == false
    @test isassigned(arr, -1) == false
end

@testset "isassigned Float64 array" begin
    arr = [1.0, 2.0, 3.0]

    @test isassigned(arr, 1) == true
    @test isassigned(arr, 3) == true
    @test isassigned(arr, 4) == false
end

@testset "isassigned single element" begin
    arr = [42]
    @test isassigned(arr, 1) == true
    @test isassigned(arr, 2) == false
end

@testset "isassigned empty array" begin
    arr = Int64[]
    @test isassigned(arr, 1) == false
end

# ===== source: array/literal_memory_first.jl =====
# Array literal behavior covered while VM builder storage is Memory-first.


@testset "Array literal Memory-first builder behavior" begin
    v = [1, 2, 3]
    @test typeof(v) == Vector{Int64}
    @test eltype(v) == Int64
    @test size(v) == (3,)
    @test v[2] == 2

    v[2] = 20
    @test v[2] == 20
    @test typeof(v) == Vector{Int64}

    m = [1.0 2.0; 3.0 4.0]
    @test typeof(m) == Array{Float64, 2}
    @test eltype(m) == Float64
    @test size(m) == (2, 2)
    @test m[1, 2] == 2.0

    m[2, 1] = 30.0
    @test m[2, 1] == 30.0
    @test m[3] == 2.0

    b = [true, false, true]
    @test typeof(b) == Vector{Bool}
    @test eltype(b) == Bool
    b[2] = true
    @test b[1] && b[2] && b[3]
end

# ===== source: array/setindex_return_type.jl =====
# Test that setindex! returns the mutated collection, not the value (Issue #3477)


@testset "array_setindex_return_type: setindex! returns mutated collection" begin
    a = [1, 2, 3]
    result = setindex!(a, 9, 1)
    @test typeof(result) == Vector{Int64}
    @test result === a
    @test result[1] == 9
end

# ===== source: array/slice_2d_dimension.jl =====
# Test 2D array slicing dimension handling
# Issue #1562: A[:, i] should return 1D vector, not 2D array


@testset "2D slice dimension handling" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Test column slice: A[:, 1] should return 1D vector
    col1 = A[:, 1]
    @test length(col1) == 2
    @test ndims(col1) == 1  # Should be 1D, not 2D
    @test col1[1] == 1.0
    @test col1[2] == 4.0

    # Test row slice: A[1, :] should return 1D vector
    row1 = A[1, :]
    @test length(row1) == 3
    @test ndims(row1) == 1  # Should be 1D, not 2D
    @test row1[1] == 1.0
    @test row1[2] == 2.0
    @test row1[3] == 3.0

    # Test full slice: A[:, :] should return 2D matrix
    full = A[:, :]
    @test size(full) == (2, 3)
    @test ndims(full) == 2

    # Test range slice: A[:, 1:2] should return 2D matrix
    cols12 = A[:, 1:2]
    @test size(cols12) == (2, 2)
    @test ndims(cols12) == 2
end

# ===== source: array/slice_2d_explicit_shape.jl =====
# Explicit shape verification for 2D slicing
# This test will fail if shape is wrong


@testset "2D slice explicit shape check" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Column slice
    col1 = A[:, 1]

    # Get size as tuple
    s = size(col1)

    # If shape is (2,), then:
    #   - length(s) == 1 (1-tuple)
    #   - s[1] == 2
    # If shape is (2, 1), then:
    #   - length(s) == 2 (2-tuple)
    #   - s[1] == 2, s[2] == 1

    # This test explicitly checks the tuple length
    tuple_len = length(s)
    @test tuple_len == 1  # Should be 1-tuple (2,) not 2-tuple (2, 1)

    # Also check ndims directly
    @test ndims(col1) == 1

    # Check the element values are correct
    @test col1[1] == 1.0
    @test col1[2] == 4.0
end

# ===== source: array/slice_2d_shape_debug.jl =====
# Debug test for 2D slice shape
# Check the actual shape returned by slicing


@testset "2D slice shape debug" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Column slice
    col1 = A[:, 1]
    col_shape = size(col1)
    col_ndims = ndims(col1)
    col_len = length(col1)

    # Check what we actually get
    @test col_len == 2

    # In Julia: size(A[:, 1]) should be (2,) not (2, 1)
    # ndims(A[:, 1]) should be 1 not 2
    @test col_ndims == 1

    # Row slice
    row1 = A[1, :]
    row_shape = size(row1)
    row_ndims = ndims(row1)
    row_len = length(row1)

    @test row_len == 3
    @test row_ndims == 1
end

# ===== source: array/slice_2d_size_check.jl =====
# Explicit size check for 2D slicing
# This test will fail if slicing returns wrong shape


@testset "2D slice size check" begin
    A = [1.0 2.0 3.0; 4.0 5.0 6.0]  # 2x3 matrix

    # Check original matrix
    @test size(A) == (2, 3)
    @test ndims(A) == 2

    # Column slice - should be 1D
    col1 = A[:, 1]
    s = size(col1)

    # In Julia, size of 1D array returns (n,) which is a 1-tuple
    # length(size(col1)) should be 1
    @test length(s) == 1
    @test s[1] == 2  # 2 elements

    # If this fails, the slice returned a 2D array
    # with shape (2, 1) instead of (2,)
end

# ===== source: array/slice_3d_basic.jl =====
# Test 3D array slicing
# Issue #1564: N-D array slicing (N > 2) returns empty array


@testset "3D array slicing" begin
    # Create a 3x4x2 array
    # Using manual construction since reshape might have issues
    A = zeros(3, 4, 2)

    # Fill with sequential values for testing
    val = 1.0
    for k in 1:2
        for j in 1:4
            for i in 1:3
                A[i, j, k] = val
                val += 1.0
            end
        end
    end

    # Test basic indexing
    @test A[1, 1, 1] == 1.0
    @test A[3, 4, 2] == 24.0

    # Test slice with scalar first dimension: A[1, :, :]
    # Should return 4x2 matrix
    slice1 = A[1, :, :]
    @test ndims(slice1) == 2
    @test size(slice1) == (4, 2)
    @test slice1[1, 1] == 1.0   # A[1,1,1]
    @test slice1[2, 1] == 4.0   # A[1,2,1]

    # Test slice with scalar second dimension: A[:, 2, :]
    # Should return 3x2 matrix
    slice2 = A[:, 2, :]
    @test ndims(slice2) == 2
    @test size(slice2) == (3, 2)
    @test slice2[1, 1] == 4.0   # A[1,2,1]
    @test slice2[2, 1] == 5.0   # A[2,2,1]

    # Test slice with scalar third dimension: A[:, :, 1]
    # Should return 3x4 matrix
    slice3 = A[:, :, 1]
    @test ndims(slice3) == 2
    @test size(slice3) == (3, 4)
    @test slice3[1, 1] == 1.0   # A[1,1,1]
    @test slice3[3, 4] == 12.0  # A[3,4,1]

    # Test full slice: A[:, :, :]
    # Should return 3x4x2 array
    full = A[:, :, :]
    @test ndims(full) == 3
    @test size(full) == (3, 4, 2)
    @test full[1, 1, 1] == 1.0
end

# ===== source: array/slice_bool_2d.jl =====
# Test slicing 2D Bool arrays preserves element type
# Issue #1565: Array slicing only handles F64/I64, other types become 0.0


@testset "2D Bool array slicing" begin
    # Create a 2D Bool array (3x3 matrix)
    arr = [true false true; false true false; true true false]

    # Test column slice: should return 1D vector
    col1 = arr[:, 1]
    @test length(col1) == 3
    @test ndims(col1) == 1
    @test col1[1] == true
    @test col1[2] == false
    @test col1[3] == true
    @test eltype(col1) == Bool

    # Test row slice: should return 1D vector
    row1 = arr[1, :]
    @test length(row1) == 3
    @test ndims(row1) == 1
    @test row1[1] == true
    @test row1[2] == false
    @test row1[3] == true
    @test eltype(row1) == Bool

    # Test full slice: should return 2D matrix
    full = arr[:, :]
    @test size(full) == (3, 3)
    @test ndims(full) == 2
    @test full[1, 1] == true
    @test full[2, 2] == true
    @test eltype(full) == Bool
end

# ===== source: array/slice_bool_array.jl =====
# Test slicing Bool arrays preserves element type
# Issue #1565: Array slicing only handles F64/I64, other types become 0.0


@testset "Bool array slicing" begin
    # Create a Bool array
    arr = [true, false, true, false, true]

    # Test 1D slicing
    slice1 = arr[2:4]
    @test length(slice1) == 3
    @test slice1[1] == false
    @test slice1[2] == true
    @test slice1[3] == false

    # Test that the element type is preserved (Bool, not F64)
    @test eltype(slice1) == Bool

    # Test slicing with :
    slice_all = arr[:]
    @test length(slice_all) == 5
    @test slice_all[1] == true
    @test slice_all[5] == true
    @test eltype(slice_all) == Bool
end

# ===== source: array/slice_producers_wrapper_6807.jl =====
# Issue #6807: the array-slice producers in `exec/array_index_slice.rs`
# (`a[range]`, `a[indexvec]`, `m[rows, cols]`, n-dim slices) now emit the
# MemoryRef-backed `Array{T,N}` wrapper instead of the legacy native carrier.
#
# A slice is a fresh array (sjulia materializes a copy, not a view), so it must
# be independently mutable and must not alias the parent. Verified against
# upstream Julia 1.12.6.


@testset "slice_producers_wrapper_6807: 1-D slices" begin
    a = [10, 20, 30, 40, 50]
    @test a[2:4] == [20, 30, 40]
    @test a[[1, 3, 5]] == [10, 30, 50]
    @test a[1:2:5] == [10, 30, 50]
    @test typeof(a[2:4]) == Vector{Int64}
    @test length(a[2:4]) == 3
end

@testset "slice_producers_wrapper_6807: 2-D slices" begin
    m = [1 2 3; 4 5 6; 7 8 9]
    @test m[1:2, 2:3] == [2 3; 5 6]
    @test m[:, 2] == [2, 5, 8]
    @test m[2, :] == [4, 5, 6]
    @test m[[1, 3], [1, 3]] == [1 3; 7 9]
    @test size(m[1:2, 2:3]) == (2, 2)
end

@testset "slice_producers_wrapper_6807: slice is a fresh mutable array" begin
    a = [10, 20, 30, 40, 50]
    s = a[2:4]
    push!(s, 99)
    @test s == [20, 30, 40, 99]
    @test a == [10, 20, 30, 40, 50]      # parent unchanged

    s[1] = 0
    @test s == [0, 30, 40, 99]
    @test a == [10, 20, 30, 40, 50]      # parent still unchanged

    col = [1 2; 3 4][:, 1]
    push!(col, 5)
    @test col == [1, 3, 5]
end

@testset "slice_producers_wrapper_6807: float + slice-of-slice" begin
    a = [1.0, 2.0, 3.0, 4.0]
    @test a[2:3] == [2.0, 3.0]
    @test a[2:4][1:2] == [2.0, 3.0]
    @test eltype(a[2:3]) == Float64
end

# ===== source: array/slice_string_array.jl =====
# Test slicing String arrays preserves element type
# Issue #1565: Array slicing only handles F64/I64, other types become 0.0


@testset "String array slicing" begin
    # Create a String array
    arr = ["apple", "banana", "cherry", "date", "elderberry"]

    # Test 1D slicing
    slice1 = arr[2:4]
    @test length(slice1) == 3
    @test slice1[1] == "banana"
    @test slice1[2] == "cherry"
    @test slice1[3] == "date"

    # Test that the element type is preserved (String, not F64)
    @test eltype(slice1) == String

    # Test slicing with :
    slice_all = arr[:]
    @test length(slice_all) == 5
    @test slice_all[1] == "apple"
    @test slice_all[5] == "elderberry"
    @test eltype(slice_all) == String
end

# ===== source: array/typed_array_indexstore_all_types.jl =====
# Comprehensive typed array IndexStore test for all numeric element types (Issue #2218)
# Verifies that Vector{T}(undef, n) followed by indexed assignment works for every
# supported numeric type. Prevents regressions where new types are added but
# IndexStore/IndexLoad handlers are not updated.


@testset "Typed array IndexStore: Int64 and Float64" begin
    # Int64
    v = Vector{Int64}(undef, 2)
    v[1] = Int64(1)
    v[2] = Int64(-1)
    @test v[1] == Int64(1)
    @test v[2] == Int64(-1)
    @test typeof(v) == Vector{Int64}

    # Float64
    v = Vector{Float64}(undef, 2)
    v[1] = 3.14
    v[2] = -1.0
    @test v[1] == 3.14
    @test v[2] == -1.0
    @test typeof(v) == Vector{Float64}
end

@testset "Typed array IndexStore: small integer types" begin
    # Int8
    v = Vector{Int8}(undef, 2)
    v[1] = Int8(42)
    v[2] = Int8(-1)
    @test v[1] == Int8(42)
    @test v[2] == Int8(-1)
    @test typeof(v) == Vector{Int8}

    # Int16
    v = Vector{Int16}(undef, 2)
    v[1] = Int16(1000)
    v[2] = Int16(-500)
    @test v[1] == Int16(1000)
    @test v[2] == Int16(-500)
    @test typeof(v) == Vector{Int16}

    # Int32
    v = Vector{Int32}(undef, 2)
    v[1] = Int32(100000)
    v[2] = Int32(-50000)
    @test v[1] == Int32(100000)
    @test v[2] == Int32(-50000)
    @test typeof(v) == Vector{Int32}
end

@testset "Typed array IndexStore: unsigned integer types" begin
    # UInt8
    v = Vector{UInt8}(undef, 2)
    v[1] = UInt8(255)
    v[2] = UInt8(0)
    @test v[1] == UInt8(255)
    @test v[2] == UInt8(0)
    @test typeof(v) == Vector{UInt8}

    # UInt16
    v = Vector{UInt16}(undef, 2)
    v[1] = UInt16(65535)
    v[2] = UInt16(0)
    @test v[1] == UInt16(65535)
    @test v[2] == UInt16(0)
    @test typeof(v) == Vector{UInt16}

    # UInt32
    v = Vector{UInt32}(undef, 2)
    v[1] = UInt32(100000)
    v[2] = UInt32(0)
    @test v[1] == UInt32(100000)
    @test v[2] == UInt32(0)
    @test typeof(v) == Vector{UInt32}

    # UInt64
    v = Vector{UInt64}(undef, 2)
    v[1] = UInt64(1)
    v[2] = UInt64(0)
    @test v[1] == UInt64(1)
    @test v[2] == UInt64(0)
    @test typeof(v) == Vector{UInt64}
end

@testset "Typed array IndexStore: float types" begin
    # Float32
    v = Vector{Float32}(undef, 2)
    v[1] = Float32(3.14)
    v[2] = Float32(-1.0)
    @test v[1] == Float32(3.14)
    @test v[2] == Float32(-1.0)
    @test typeof(v) == Vector{Float32}
end

@testset "Typed array IndexStore: Bool" begin
    v = Vector{Bool}(undef, 2)
    v[1] = true
    v[2] = false
    @test v[1] == true
    @test v[2] == false
    @test typeof(v) == Vector{Bool}
end

@testset "Typed array IndexStore: overwrite" begin
    v = Vector{Int32}(undef, 1)
    v[1] = Int32(10)
    @test v[1] == Int32(10)
    v[1] = Int32(20)
    @test v[1] == Int32(20)

    v = Vector{Float32}(undef, 1)
    v[1] = Float32(1.0)
    @test v[1] == Float32(1.0)
    v[1] = Float32(2.0)
    @test v[1] == Float32(2.0)
end

# ===== source: array/vector_bool_indexstore.jl =====

# Test writing to Vector{Bool}(undef, n) via indexed assignment (Issue #2207)
# The IndexStore instruction must handle Bool values when writing to Bool arrays.

@testset "Vector{Bool} indexed assignment" begin
    # Basic write and read
    v = Vector{Bool}(undef, 4)
    v[1] = true
    v[2] = false
    v[3] = true
    v[4] = false
    @test v[1] == true
    @test v[2] == false
    @test v[3] == true
    @test v[4] == false

    # Overwrite values
    v[1] = false
    v[2] = true
    @test v[1] == false
    @test v[2] == true

    # typeof check
    @test typeof(v) == Vector{Bool}

    # length check
    @test length(v) == 4
end

# ===== source: array/wrap_range_indexing.jl =====

@testset "wrap Array range and colon indexing" begin
    m = Memory{Int64}(undef, 5)
    for i in 1:5
        m[i] = 10 * i
    end

    a = wrap(Array, m, 5)
    r = a[2:4]
    @test size(r) == (3,)
    @test r[1] == 20
    @test r[2] == 30
    @test r[3] == 40

    r[2] = 999
    @test a[3] == 30
    @test m[3] == 30

    c = a[:]
    @test size(c) == (5,)
    @test c[1] == 10
    @test c[5] == 50

    a[1] = 7.0
    @test a[1] == 7
end

true
