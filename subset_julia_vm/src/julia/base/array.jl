# =============================================================================
# Array{T} - Pure Julia mutable struct definition (Issue #2760)
# =============================================================================
# Based on Julia's base/array.jl (Julia 1.11+)
#
# In official Julia, Array{T,N} wraps Memory{T} for contiguous storage.
# This struct definition mirrors that structure using a mutable struct.
#
# Note: The N (dimension count) parameter is omitted because the current
# parametric type system doesn't support integer value parameters.
# Dimensions are stored in _size and derived at runtime.
#
# Note: The compiler currently intercepts Array{T}(undef, n) to create
# Rust Value::Array directly. This struct definition coexists with that path.
# Built-in patterns (undef constructor, empty, single-arg) still use the Rust path.
# Non-builtin patterns (e.g., Array{T}(mem, size)) use this struct.

mutable struct Array{T}
    _mem
    _size
end

# =============================================================================
# Array{T} method dispatch - TODO (Issue #2760)
# =============================================================================
# Methods like size, length, getindex, setindex! on the Pure Julia Array struct
# are currently blocked because the dispatch system can't match
# Struct("Array{Int64}") against the built-in ::Array type annotation.
# This needs type system changes to recognize user-defined Array{T} structs.
#
# Planned methods (to be enabled when dispatch is updated):
#   size(a::Array{T}) where T = a._size
#   size(a::Array{T}, d::Int64) where T = a._size[d]
#   length(a::Array{T}) where T = length(a._mem)
#   getindex(a::Array{T}, i::Int64) where T = a._mem[i]
#   setindex!(a::Array{T}, v, i::Int64) where T = setindex!(a._mem, v, i)
#   ndims(a::Array{T}) where T = length(a._size)
#   eltype(a::Array{T}) where T = T  (blocked by Any→DataType conversion)

# =============================================================================
# Array functions - Pure Julia implementations
# =============================================================================

# sum: compute the sum of all elements (preserves element type)
# With dims keyword: sum along specified dimension (dims=1: columns, dims=2: rows)
function sum(arr; dims=0)
    if dims == 0
        n = length(arr)
        if n == 0
            return zero(eltype(arr))
        end
        result = arr[1]
        for i in 2:n
            result += arr[i]
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = zeros(1, n)
        for j in 1:n
            s = 0.0
            for i in 1:m
                s = s + arr[i, j]
            end
            result[1, j] = s
        end
        return result
    elseif dims == 2
        result = zeros(m, 1)
        for i in 1:m
            s = 0.0
            for j in 1:n
                s = s + arr[i, j]
            end
            result[i, 1] = s
        end
        return result
    else
        error("sum: dims must be 1 or 2 for matrices")
    end
end

# sum(g::Generator): collect and sum
# Handles generator expressions like sum(x^2 for x in 1:10)
function sum(g::Generator)
    return sum(collect(g))
end

# prod: compute the product of all elements
# With dims keyword: product along specified dimension (dims=1: columns, dims=2: rows)
function prod(arr; dims=0)
    if dims == 0
        result = 1
        n = length(arr)
        for i in 1:n
            result *= arr[i]
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = ones(1, n)
        for j in 1:n
            p = 1.0
            for i in 1:m
                p = p * arr[i, j]
            end
            result[1, j] = p
        end
        return result
    elseif dims == 2
        result = ones(m, 1)
        for i in 1:m
            p = 1.0
            for j in 1:n
                p = p * arr[i, j]
            end
            result[i, 1] = p
        end
        return result
    else
        error("prod: dims must be 1 or 2 for matrices")
    end
end

# prod(f, arr) - product of f(x) for each element x
# Based on Julia's base/reduce.jl
function prod(f::Function, arr)
    n = length(arr)
    result = f(arr[1])
    for i in 2:n
        result = result * f(arr[i])
    end
    return result
end

# minimum: find the minimum element
# With dims keyword: minimum along specified dimension (dims=1: columns, dims=2: rows)
function minimum(arr; dims=0)
    if dims == 0
        result = arr[1]
        n = length(arr)
        for i in 2:n
            if arr[i] < result
                result = arr[i]
            end
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = zeros(1, n)
        for j in 1:n
            minval = arr[1, j]
            for i in 2:m
                if arr[i, j] < minval
                    minval = arr[i, j]
                end
            end
            result[1, j] = minval
        end
        return result
    elseif dims == 2
        result = zeros(m, 1)
        for i in 1:m
            minval = arr[i, 1]
            for j in 2:n
                if arr[i, j] < minval
                    minval = arr[i, j]
                end
            end
            result[i, 1] = minval
        end
        return result
    else
        error("minimum: dims must be 1 or 2 for matrices")
    end
end

# minimum(f, arr) - minimum of f(x) for each element x
# Based on Julia's base/reduce.jl:674
function minimum(f::Function, arr)
    return findmin(f, arr)[1]
end

# maximum: find the maximum element
# With dims keyword: maximum along specified dimension (dims=1: columns, dims=2: rows)
function maximum(arr; dims=0)
    if dims == 0
        result = arr[1]
        n = length(arr)
        for i in 2:n
            if arr[i] > result
                result = arr[i]
            end
        end
        return result
    end
    m = size(arr, 1)
    n = size(arr, 2)
    if dims == 1
        result = zeros(1, n)
        for j in 1:n
            maxval = arr[1, j]
            for i in 2:m
                if arr[i, j] > maxval
                    maxval = arr[i, j]
                end
            end
            result[1, j] = maxval
        end
        return result
    elseif dims == 2
        result = zeros(m, 1)
        for i in 1:m
            maxval = arr[i, 1]
            for j in 2:n
                if arr[i, j] > maxval
                    maxval = arr[i, j]
                end
            end
            result[i, 1] = maxval
        end
        return result
    else
        error("maximum: dims must be 1 or 2 for matrices")
    end
end

# maximum(f, arr) - maximum of f(x) for each element x
# Based on Julia's base/reduce.jl:647
function maximum(f::Function, arr)
    return findmax(f, arr)[1]
end

# =============================================================================
# In-place reduction functions
# =============================================================================
# Based on Julia's base/reducedim.jl
#
# These functions reduce A over the singleton dimensions of r,
# writing results into r. The shape of r determines which dimensions
# are reduced:
#   - r is a column vector (m×1): reduce along dim 2 (sum rows)
#   - r is a row vector (1×n): reduce along dim 1 (sum columns)

# sum!: sum elements of A over singleton dimensions of r, write to r
function sum!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        # r is vector of length m → reduce along dim 2
        m = sa[1]
        n = sa[2]
        for i in 1:m
            s = 0.0
            for j in 1:n
                s = s + A[i, j]
            end
            r[i] = s
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            # r is 1×n → reduce along dim 1
            for j in 1:n
                s = 0.0
                for i in 1:m
                    s = s + A[i, j]
                end
                r[1, j] = s
            end
        elseif sr[1] == m && sr[2] == 1
            # r is m×1 → reduce along dim 2
            for i in 1:m
                s = 0.0
                for j in 1:n
                    s = s + A[i, j]
                end
                r[i, 1] = s
            end
        else
            error("sum!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("sum!: unsupported array dimensions")
    end
    return r
end

# prod!: product of elements of A over singleton dimensions of r, write to r
function prod!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        m = sa[1]
        n = sa[2]
        for i in 1:m
            p = 1.0
            for j in 1:n
                p = p * A[i, j]
            end
            r[i] = p
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            for j in 1:n
                p = 1.0
                for i in 1:m
                    p = p * A[i, j]
                end
                r[1, j] = p
            end
        elseif sr[1] == m && sr[2] == 1
            for i in 1:m
                p = 1.0
                for j in 1:n
                    p = p * A[i, j]
                end
                r[i, 1] = p
            end
        else
            error("prod!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("prod!: unsupported array dimensions")
    end
    return r
end

# maximum!: maximum of A over singleton dimensions of r, write to r
function maximum!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        m = sa[1]
        n = sa[2]
        for i in 1:m
            maxval = A[i, 1]
            for j in 2:n
                if A[i, j] > maxval
                    maxval = A[i, j]
                end
            end
            r[i] = maxval
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            for j in 1:n
                maxval = A[1, j]
                for i in 2:m
                    if A[i, j] > maxval
                        maxval = A[i, j]
                    end
                end
                r[1, j] = maxval
            end
        elseif sr[1] == m && sr[2] == 1
            for i in 1:m
                maxval = A[i, 1]
                for j in 2:n
                    if A[i, j] > maxval
                        maxval = A[i, j]
                    end
                end
                r[i, 1] = maxval
            end
        else
            error("maximum!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("maximum!: unsupported array dimensions")
    end
    return r
end

# minimum!: minimum of A over singleton dimensions of r, write to r
function minimum!(r, A)
    sr = size(r)
    sa = size(A)
    ndr = length(sr)
    nda = length(sa)
    if ndr == 1 && nda == 2
        m = sa[1]
        n = sa[2]
        for i in 1:m
            minval = A[i, 1]
            for j in 2:n
                if A[i, j] < minval
                    minval = A[i, j]
                end
            end
            r[i] = minval
        end
    elseif ndr == 2 && nda == 2
        m = sa[1]
        n = sa[2]
        if sr[1] == 1 && sr[2] == n
            for j in 1:n
                minval = A[1, j]
                for i in 2:m
                    if A[i, j] < minval
                        minval = A[i, j]
                    end
                end
                r[1, j] = minval
            end
        elseif sr[1] == m && sr[2] == 1
            for i in 1:m
                minval = A[i, 1]
                for j in 2:n
                    if A[i, j] < minval
                        minval = A[i, j]
                    end
                end
                r[i, 1] = minval
            end
        else
            error("minimum!: output dimensions must match input along non-reduced dimensions")
        end
    else
        error("minimum!: unsupported array dimensions")
    end
    return r
end

# argmin: find index of minimum element
function argmin(arr)
    idx = 1
    val = arr[1]
    n = length(arr)
    for i in 2:n
        if arr[i] < val
            val = arr[i]
            idx = i
        end
    end
    return idx
end

# argmax: find index of maximum element
function argmax(arr)
    idx = 1
    val = arr[1]
    n = length(arr)
    for i in 2:n
        if arr[i] > val
            val = arr[i]
            idx = i
        end
    end
    return idx
end

# reverse: reverse an array (type-preserving)
function reverse(arr)
    n = length(arr)
    result = collect(arr)  # Create type-preserving copy
    for i in 1:n
        result[i] = arr[n - i + 1]
    end
    return result
end

# Note: count(f, arr) is implemented as a builtin HOF
# because the VM doesn't yet support calling function parameters

# issorted: check if array is sorted in ascending order
function issorted(arr)
    n = length(arr)
    nm1 = n - 1
    for i in 1:nm1
        if arr[i] > arr[i+1]
            return false
        end
    end
    return true
end

# =============================================================================
# Array manipulation functions
# =============================================================================

# circshift: circular shift array by k positions (type-preserving)
# Positive k shifts right, negative k shifts left
function circshift(arr, k)
    n = length(arr)
    if n == 0
        return collect(arr)  # Return type-preserving empty array
    end
    # Normalize k to be in range [0, n)
    k = mod(k, n)
    if k == 0
        return collect(arr)  # Return type-preserving copy
    end
    result = collect(arr)  # Create type-preserving copy
    for i in 1:n
        # New position after shifting right by k
        new_i = mod(i - 1 + k, n) + 1
        result[new_i] = arr[i]
    end
    return result
end

# circshift!: circular shift array in place
# Based on Julia's base/abstractarray.jl:3655
# Uses the "block swap" algorithm with three reverses
function circshift!(arr, shift)
    n = length(arr)
    if n == 0
        return arr
    end
    shift = mod(shift, n)
    if shift == 0
        return arr
    end
    # Block swap algorithm:
    # 1. Reverse first part [1, n-shift]
    # 2. Reverse second part [n-shift+1, n]
    # 3. Reverse entire array
    reverse!(arr, 1, n - shift)
    reverse!(arr, n - shift + 1, n)
    reverse!(arr)
    return arr
end

# repeat: repeat array n times
# Julia-style multiple dispatch: this handles arrays, string repeat is handled by builtin
# Type annotation ensures this only matches Array, not String
function repeat(arr::Array, n::Int)
    len = length(arr)
    result = zeros(len * n)
    idx = 1
    for _ in 1:n
        for i in 1:len
            result[idx] = arr[i]
            idx = idx + 1
        end
    end
    return result
end

# rotl90: rotate matrix 90 degrees counter-clockwise
# For matrix with size (m, n), result has size (n, m)
function rotl90(mat)
    m = size(mat, 1)
    n = size(mat, 2)
    result = zeros(n, m)
    for i in 1:m
        for j in 1:n
            result[n - j + 1, i] = mat[i, j]
        end
    end
    return result
end

# rotr90: rotate matrix 90 degrees clockwise
# For matrix with size (m, n), result has size (n, m)
function rotr90(mat)
    m = size(mat, 1)
    n = size(mat, 2)
    result = zeros(n, m)
    for i in 1:m
        for j in 1:n
            result[j, m - i + 1] = mat[i, j]
        end
    end
    return result
end

# rot180: rotate matrix 180 degrees
function rot180(mat)
    m = size(mat, 1)
    n = size(mat, 2)
    result = zeros(m, n)
    for i in 1:m
        for j in 1:n
            result[m - i + 1, n - j + 1] = mat[i, j]
        end
    end
    return result
end

# copy: create a shallow copy of an array (type-preserving)
function copy(arr)
    return collect(arr)
end

# Note: mean is in Statistics, not Base. Use `using Statistics` to get mean.
# See: subset_julia_vm/src/julia/stdlib/Statistics/src/Statistics.jl

# =============================================================================
# Array concatenation functions
# =============================================================================

# vcat: vertical concatenation of 1D arrays (type-preserving)
# For 1D arrays, concatenates elements sequentially
# Based on Julia's base/abstractarray.jl:1695
function vcat(a, b)
    na = length(a)
    nb = length(b)
    # Create result array by collecting 'a' and extending
    result = collect(a)
    for i in 1:nb
        push!(result, b[i])
    end
    return result
end

# vcat: varargs version for 3+ arguments
# Based on Julia's base/abstractarray.jl:1966
function vcat(args...)
    n = length(args)
    if n == 0
        return Int64[]
    end
    if n == 1
        return collect(args[1])
    end
    # Fold left: vcat(a, b, c, ...) = vcat(vcat(a, b), c, ...)
    result = collect(args[1])
    for i in 2:n
        arr_i = args[i]
        ni = length(arr_i)
        for j in 1:ni
            push!(result, arr_i[j])
        end
    end
    return result
end

# hcat: horizontal concatenation (treats 1D arrays as column vectors)
# Returns a matrix from 1D arrays of same length
# Based on Julia's base/abstractarray.jl:1728
function hcat(a, b)
    na = length(a)
    nb = length(b)
    if na != nb
        error("hcat: arrays must have same length")
    end
    result = zeros(na, 2)
    for i in 1:na
        result[i, 1] = a[i]
        result[i, 2] = b[i]
    end
    return result
end

# hcat: varargs version for 3+ arguments
# Based on Julia's base/abstractarray.jl:2016
function hcat(args...)
    n = length(args)
    if n == 0
        error("hcat requires at least one argument")
    end
    if n == 1
        nrows = length(args[1])
        result = zeros(nrows, 1)
        for i in 1:nrows
            result[i, 1] = args[1][i]
        end
        return result
    end
    nrows = length(args[1])
    # Verify all arrays have same length
    for j in 2:n
        if length(args[j]) != nrows
            error("hcat: arrays must have same length")
        end
    end
    result = zeros(nrows, n)
    for j in 1:n
        for i in 1:nrows
            result[i, j] = args[j][i]
        end
    end
    return result
end

# vec: flatten array to 1D vector (type-preserving)
function vec(arr)
    return collect(arr)
end

# =============================================================================
# stack: combine arrays into a higher-dimensional array
# =============================================================================
# Based on Julia's Base.stack (Julia 1.9+)

# stack(arrays): stack 1D arrays as columns of a matrix
# Each element of arrays should be a 1D array of the same length.
# Returns a matrix where column j is arrays[j].
function stack(arrays)
    n = length(arrays)
    if n == 0
        return zeros(0, 0)
    end
    first_arr = arrays[1]
    m = length(first_arr)
    result = zeros(m, n)
    for j in 1:n
        arr = arrays[j]
        for i in 1:m
            result[i, j] = arr[i]
        end
    end
    return result
end

# =============================================================================
# selectdim: select a slice along a specific dimension
# =============================================================================
# Based on Julia's Base.selectdim

# selectdim(A, d, i): return slice of A along dimension d at index i
# For 2D matrices:
#   selectdim(A, 1, i) returns row i as a 1D vector
#   selectdim(A, 2, j) returns column j as a 1D vector
function selectdim(A, d, i)
    m = size(A, 1)
    n = size(A, 2)
    if d == 1
        # Select row i
        result = zeros(n)
        for j in 1:n
            result[j] = A[i, j]
        end
        return result
    elseif d == 2
        # Select column j
        result = zeros(m)
        for k in 1:m
            result[k] = A[k, i]
        end
        return result
    else
        error("selectdim: dimension must be 1 or 2 for matrices")
    end
end

# =============================================================================
# dropdims: remove singleton dimensions from array
# =============================================================================
# Based on Julia's Base.dropdims
# Simplified: supports removing a single dimension from 2D arrays

# dropdims(A; dims): remove singleton dimension
# For a 2D array with one singleton dimension, returns a 1D vector
function dropdims(A; dims)
    m = size(A, 1)
    n = size(A, 2)
    if dims == 1
        if m != 1
            error("dropdims: dimension 1 has size $m, must be 1")
        end
        result = zeros(n)
        for j in 1:n
            result[j] = A[1, j]
        end
        return result
    elseif dims == 2
        if n != 1
            error("dropdims: dimension 2 has size $n, must be 1")
        end
        result = zeros(m)
        for i in 1:m
            result[i] = A[i, 1]
        end
        return result
    else
        error("dropdims: dimension must be 1 or 2 for matrices")
    end
end

# insertdims(A; dims): insert singleton dimension at specified position
# Inverse of dropdims. Based on Julia's base/abstractarraymath.jl (Julia 1.12).
# For a 1D vector: dims=1 -> 1×n row, dims=2 -> n×1 column
# For a 2D matrix: dims=3 -> m×n×1 array
function insertdims(A; dims)
    # deepcopy A first because reshape modifies shape in-place in SubsetJuliaVM
    B = deepcopy(A)
    nd = ndims(B)
    if nd == 1
        n = length(B)
        if dims == 1
            return reshape(B, 1, n)
        elseif dims == 2
            return reshape(B, n, 1)
        else
            error("insertdims: dims must be between 1 and $(nd + 1) for $(nd)D arrays")
        end
    elseif nd == 2
        m = size(B, 1)
        n = size(B, 2)
        if dims == 1
            return reshape(B, 1, m, n)
        elseif dims == 2
            return reshape(B, m, 1, n)
        elseif dims == 3
            return reshape(B, m, n, 1)
        else
            error("insertdims: dims must be between 1 and $(nd + 1) for $(nd)D arrays")
        end
    else
        error("insertdims: only 1D and 2D arrays are supported")
    end
end

# =============================================================================
# Array search functions
# =============================================================================

# findfirst: find first index where predicate is satisfied
# Returns the index of first occurrence, or nothing if not found
# Note: String search is handled by builtin findfirst(pattern::String, s::String)
function findfirst(f::Function, arr::Array)
    n = length(arr)
    for i in 1:n
        if f(arr[i])
            return i
        end
    end
    return nothing
end

# findfirst: find first index where value appears in array
function findfirst(value, arr::Array)
    n = length(arr)
    for i in 1:n
        if arr[i] == value
            return i
        end
    end
    return nothing
end

# findlast: find last index where predicate is satisfied
# Returns the index of last occurrence, or nothing if not found
# Note: String search is handled by builtin findlast(pattern::String, s::String)
function findlast(f::Function, arr::Array)
    n = length(arr)
    for i in n:-1:1
        if f(arr[i])
            return i
        end
    end
    return nothing
end

# findlast: find last index where value appears in array
function findlast(value, arr::Array)
    n = length(arr)
    for i in n:-1:1
        if arr[i] == value
            return i
        end
    end
    return nothing
end

# Note: findall(f, arr) is implemented as a builtin higher-order function
# Returns a Vector{Int64} of 1-based indices where predicate f returns true
# See: src/compile/expr/builtin_hof.rs

# findall(A): Single-argument form for boolean/truthy arrays
# Based on Julia's base/array.jl:2812
# Returns Vector{Int64} of indices where A[i] is truthy
function findall(A::Array)
    result = Int64[]
    n = length(A)
    for i in 1:n
        # Direct truthiness check works for Bool values
        if A[i]
            push!(result, i)
        end
    end
    return result
end

# findall(x::Bool): Scalar boolean - returns [1] if true, empty array if false
# Based on Julia's base/array.jl:2825
function findall(x::Bool)
    if x
        return Int64[1]
    else
        return Int64[]
    end
end

# =============================================================================
# Array manipulation functions (additional)
# =============================================================================

# fill!: fill array with a value (mutating)
function fill!(arr, value)
    n = length(arr)
    for i in 1:n
        arr[i] = value
    end
    return arr
end

# copyto!: copy elements from src to dest (mutating)
function copyto!(dest, src)
    n = length(src)
    for i in 1:n
        dest[i] = src[i]
    end
    return dest
end

# copyto!(dest, dstart, src): copy all of src to dest starting at dest[dstart]
# Based on Julia's base/abstractarray.jl:1126
function copyto!(dest::Array, dstart::Int64, src::Array)
    n = length(src)
    for i in 1:n
        dest[dstart + i - 1] = src[i]
    end
    return dest
end

# copyto!(dest, dstart, src, sstart): copy from src[sstart:end] to dest[dstart:end]
# Based on Julia's base/abstractarray.jl:1130
function copyto!(dest::Array, dstart::Int64, src::Array, sstart::Int64)
    n = length(src) - sstart + 1
    for i in 1:n
        dest[dstart + i - 1] = src[sstart + i - 1]
    end
    return dest
end

# copyto!(dest, dstart, src, sstart, n): copy n elements from src[sstart] to dest[dstart]
# Based on Julia's base/abstractarray.jl:1136
function copyto!(dest::Array, dstart::Int64, src::Array, sstart::Int64, n::Int64)
    if n == 0
        return dest
    end
    for i in 1:n
        dest[dstart + i - 1] = src[sstart + i - 1]
    end
    return dest
end

# copy!: copy elements from src to dest (mutating), resizing dest if needed
# Based on Julia's base/abstractarray.jl:924
# For vectors/1D arrays: resizes dest to match src length, then copies
function copy!(dest::Array, src::Array)
    if length(dest) != length(src)
        resize!(dest, length(src))
    end
    copyto!(dest, src)
end

# =============================================================================
# Array dimension and iteration functions
# =============================================================================

# ndims: return the number of dimensions of an array
# Based on Julia's base/abstractarray.jl
#   ndims(::AbstractArray{T,N}) where {T,N} = N::Int
# Note: In Julia, ndims extracts N from the type parameter.
# Here we compute it from the size tuple.
function ndims(arr::AbstractArray)
    return length(size(arr))
end

# axes: return tuple of index ranges for each dimension
# Supports up to 16 dimensions (covers virtually all practical use cases)
function axes(arr)
    s = size(arr)
    n = length(s)
    if n == 1
        return (1:s[1],)
    elseif n == 2
        return (1:s[1], 1:s[2])
    elseif n == 3
        return (1:s[1], 1:s[2], 1:s[3])
    elseif n == 4
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4])
    elseif n == 5
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5])
    elseif n == 6
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6])
    elseif n == 7
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7])
    elseif n == 8
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8])
    elseif n == 9
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9])
    elseif n == 10
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10])
    elseif n == 11
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10], 1:s[11])
    elseif n == 12
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10], 1:s[11], 1:s[12])
    elseif n == 13
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10], 1:s[11], 1:s[12], 1:s[13])
    elseif n == 14
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10], 1:s[11], 1:s[12], 1:s[13], 1:s[14])
    elseif n == 15
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10], 1:s[11], 1:s[12], 1:s[13], 1:s[14], 1:s[15])
    elseif n == 16
        return (1:s[1], 1:s[2], 1:s[3], 1:s[4], 1:s[5], 1:s[6], 1:s[7], 1:s[8], 1:s[9], 1:s[10], 1:s[11], 1:s[12], 1:s[13], 1:s[14], 1:s[15], 1:s[16])
    else
        # For dimensions > 16, throw an error
        error("axes: arrays with more than 16 dimensions are not supported")
    end
end

# axes with dimension argument: return range for specific dimension
function axes(arr, d::Int)
    s = size(arr)
    if d > length(s)
        return 1:1
    end
    return 1:s[d]
end

# Note: enumerate and zip are not yet fully implemented
# The Pure Julia iterator protocol (iterate function) is not supported
# These functions are exported but will cause compilation errors if used
# TODO: Implement when ArrayData::Tuple or iterator protocol is supported

# Placeholder functions - will error if called with unsupported patterns
# enumerate(arr) - works with eachindex pattern: for i in eachindex(arr)
# zip(a, b) - use manual indexing: for i in 1:min(length(a), length(b))

# =============================================================================
# Dimension permutation functions
# =============================================================================

# permutedims for 1D vector: converts to 1×N row vector
# permutedims for 2D matrix: transpose (swap rows and columns)
# Type-preserving: Int64 input -> Int64 output
function permutedims(arr)
    s = size(arr)
    nd = length(s)
    T = eltype(arr)
    if nd == 1
        # 1D vector -> 1×N row vector
        n = s[1]
        # Type-preserving array creation
        if T == Int64
            result = zeros(Int64, 1, n)
        else
            result = zeros(1, n)
        end
        for i in 1:n
            result[1, i] = arr[i]
        end
        return result
    elseif nd == 2
        # 2D matrix -> transpose
        m = s[1]
        n = s[2]
        # Type-preserving array creation
        if T == Int64
            result = zeros(Int64, n, m)
        else
            result = zeros(n, m)
        end
        for i in 1:m
            for j in 1:n
                result[j, i] = arr[i, j]
            end
        end
        return result
    else
        error("permutedims without perm argument only supports 1D and 2D arrays")
    end
end

# permutedims with explicit permutation tuple
# Supports up to 4-dimensional arrays
function permutedims(arr, perm)
    s = size(arr)
    nd = length(s)

    # Validate permutation length matches array dimensions
    perm_len = length(perm)
    if perm_len != nd
        error("permutedims: permutation length must match array dimensions")
    end

    if nd == 2
        p1 = perm[1]
        p2 = perm[2]
        if p1 == 1 && p2 == 2
            # Identity permutation - copy
            m = s[1]
            n = s[2]
            result = zeros(m, n)
            for i in 1:m
                for j in 1:n
                    result[i, j] = arr[i, j]
                end
            end
            return result
        elseif p1 == 2 && p2 == 1
            # Transpose
            return permutedims(arr)
        else
            error("permutedims: invalid permutation indices")
        end
    elseif nd == 3
        # 3D array permutation
        p1 = Int64(perm[1])
        p2 = Int64(perm[2])
        p3 = Int64(perm[3])
        # New shape: (s[p1], s[p2], s[p3])
        ns1 = s[p1]
        ns2 = s[p2]
        ns3 = s[p3]
        result = zeros(ns1, ns2, ns3)
        # For each output index (i1, i2, i3), compute input index
        for i1 in 1:ns1
            for i2 in 1:ns2
                for i3 in 1:ns3
                    # Compute input indices using inverse permutation
                    # Output index (i1, i2, i3) maps to input at position where
                    # inp[pk] = ik for each dimension k
                    in1 = (p1 == 1 ? i1 : (p2 == 1 ? i2 : i3))
                    in2 = (p1 == 2 ? i1 : (p2 == 2 ? i2 : i3))
                    in3 = (p1 == 3 ? i1 : (p2 == 3 ? i2 : i3))
                    result[i1, i2, i3] = arr[in1, in2, in3]
                end
            end
        end
        return result
    elseif nd == 4
        # 4D array permutation
        p1 = Int64(perm[1])
        p2 = Int64(perm[2])
        p3 = Int64(perm[3])
        p4 = Int64(perm[4])
        # New shape
        ns1 = s[p1]
        ns2 = s[p2]
        ns3 = s[p3]
        ns4 = s[p4]
        result = zeros(ns1, ns2, ns3, ns4)
        for i1 in 1:ns1
            for i2 in 1:ns2
                for i3 in 1:ns3
                    for i4 in 1:ns4
                        # Compute input indices
                        in1 = (p1 == 1 ? i1 : (p2 == 1 ? i2 : (p3 == 1 ? i3 : i4)))
                        in2 = (p1 == 2 ? i1 : (p2 == 2 ? i2 : (p3 == 2 ? i3 : i4)))
                        in3 = (p1 == 3 ? i1 : (p2 == 3 ? i2 : (p3 == 3 ? i3 : i4)))
                        in4 = (p1 == 4 ? i1 : (p2 == 4 ? i2 : (p3 == 4 ? i3 : i4)))
                        result[i1, i2, i3, i4] = arr[in1, in2, in3, in4]
                    end
                end
            end
        end
        return result
    else
        error("permutedims: only supports arrays up to 4 dimensions")
    end
end

# permutedims!: permute dimensions of src and store result in dest
# Based on Julia's base/permuteddimsarray.jl
function permutedims!(dest, src, perm)
    s = size(src)
    nd = length(s)
    perm_len = length(perm)
    if perm_len != nd
        error("permutedims!: permutation length must match array dimensions")
    end
    if nd == 2
        p1 = perm[1]
        p2 = perm[2]
        m = s[1]
        n = s[2]
        if p1 == 1 && p2 == 2
            # Identity permutation - copy
            for i in 1:m
                for j in 1:n
                    dest[i, j] = src[i, j]
                end
            end
        elseif p1 == 2 && p2 == 1
            # Transpose
            for i in 1:m
                for j in 1:n
                    dest[j, i] = src[i, j]
                end
            end
        else
            error("permutedims!: invalid permutation indices")
        end
    elseif nd == 3
        p1 = Int64(perm[1])
        p2 = Int64(perm[2])
        p3 = Int64(perm[3])
        ns1 = s[p1]
        ns2 = s[p2]
        ns3 = s[p3]
        for i1 in 1:ns1
            for i2 in 1:ns2
                for i3 in 1:ns3
                    in1 = (p1 == 1 ? i1 : (p2 == 1 ? i2 : i3))
                    in2 = (p1 == 2 ? i1 : (p2 == 2 ? i2 : i3))
                    in3 = (p1 == 3 ? i1 : (p2 == 3 ? i2 : i3))
                    dest[i1, i2, i3] = src[in1, in2, in3]
                end
            end
        end
    else
        error("permutedims!: only supports 2D and 3D arrays")
    end
    return dest
end

# =============================================================================
# transpose and adjoint for arrays
# =============================================================================
# Based on Julia's LinearAlgebra module
# transpose(A) = permutedims(A) for 1D and 2D arrays
# adjoint(A) = conjugate transpose (conj applied element-wise, then transpose)

# transpose for arrays - pure permutation without conjugation
function transpose(arr::Array)
    return permutedims(arr)
end

# adjoint for arrays - conjugate transpose
# For real arrays, this is identical to transpose
# For complex arrays, each element is conjugated
# Type-preserving: Int64 input -> Int64 output
function adjoint(arr::Array)
    s = size(arr)
    nd = length(s)
    T = eltype(arr)
    if nd == 1
        # 1D vector -> 1×N row vector (conjugated)
        n = s[1]
        # Type-preserving array creation
        if T == Int64
            result = zeros(Int64, 1, n)
        else
            result = zeros(1, n)
        end
        for i in 1:n
            result[1, i] = conj(arr[i])
        end
        return result
    elseif nd == 2
        # 2D matrix -> conjugate transpose (n×m)
        m = s[1]
        n = s[2]
        # Type-preserving array creation
        if T == Int64
            result = zeros(Int64, n, m)
        else
            result = zeros(n, m)
        end
        for i in 1:m
            for j in 1:n
                result[j, i] = conj(arr[i, j])
            end
        end
        return result
    else
        error("adjoint only supports 1D and 2D arrays")
    end
end

# =============================================================================
# adjoint for range types - convert to row vector
# =============================================================================
# Based on Julia's LinearAlgebra/src/adjtrans.jl
# Ranges are collected to arrays before applying adjoint

# adjoint for UnitRange - converts to 1×N row vector
function adjoint(r::UnitRange)
    return adjoint(collect(r))
end

# adjoint for LinRange - converts to 1×N row vector
function adjoint(r::LinRange)
    return adjoint(collect(r))
end

# adjoint for StepRangeLen - converts to 1×N row vector
function adjoint(r::StepRangeLen)
    return adjoint(collect(r))
end

# adjoint for OneTo - converts to 1×N row vector
function adjoint(r::OneTo)
    return adjoint(collect(r))
end

# =============================================================================
# transpose for range types - convert to row vector (no conjugation)
# =============================================================================

# transpose for UnitRange - converts to 1×N row vector
function transpose(r::UnitRange)
    return transpose(collect(r))
end

# transpose for LinRange - converts to 1×N row vector
function transpose(r::LinRange)
    return transpose(collect(r))
end

# transpose for StepRangeLen - converts to 1×N row vector
function transpose(r::StepRangeLen)
    return transpose(collect(r))
end

# transpose for OneTo - converts to 1×N row vector
function transpose(r::OneTo)
    return transpose(collect(r))
end

# =============================================================================
# Array repetition functions
# =============================================================================

# Note: repeat(arr::Array, n::Int) is already defined above (line 134)
# String repeat is handled by Pure Julia in base/strings/basic.jl

# repeat(v, m, n) for 1D vector - create m×n matrix by tiling
# Example: repeat([1, 2], 3, 2) => 6×2 matrix
#   1  1
#   2  2
#   1  1
#   2  2
#   1  1
#   2  2
function repeat(arr::Array, m::Int64, n::Int64)
    # Check dimensionality using length(size(arr))
    # 1D: size(arr) = (n,), length = 1
    # 2D: size(arr) = (m, n), length = 2
    dims = length(size(arr))
    if dims == 1
        # 1D vector: repeat vertically m times, horizontally n times
        len = length(arr)
        result = zeros(len * m, n)
        for j in 1:n
            row = 1
            for _ in 1:m
                for i in 1:len
                    result[row, j] = arr[i]
                    row = row + 1
                end
            end
        end
        return result
    else
        # 2D matrix: repeat m times vertically, n times horizontally
        rows = size(arr, 1)
        cols = size(arr, 2)
        result = zeros(rows * m, cols * n)
        for block_j in 0:(n-1)
            for block_i in 0:(m-1)
                for j in 1:cols
                    for i in 1:rows
                        result[block_i * rows + i, block_j * cols + j] = arr[i, j]
                    end
                end
            end
        end
        return result
    end
end

# =============================================================================
# empty - create empty collection of same type
# =============================================================================
# Based on Julia's base/abstractarray.jl and base/abstractdict.jl
#
# empty(a) creates an empty collection of the same type as a

# empty for arrays - create empty array of same element type
function empty(arr::Array)
    return zeros(0)
end

# empty(a, T) creates empty array with element type T
function empty(arr::Array, T)
    # Workaround: returns Float64 array (typed empty array not supported)
    return zeros(0)
end

# empty for Dict - create empty Dict with same key/value types
# Note: In SubsetJuliaVM, Dict type information is limited
# This is a simplified implementation
function empty(dict::Dict)
    return Dict()
end

# empty for Dict with value type specified
function empty(dict::Dict, V)
    return Dict()
end

# empty for Set (arrays used as sets) - create empty array
function empty(set::Array)
    return zeros(0)
end

# empty for Tuple - return empty tuple
function empty(tup::Tuple)
    return ()
end

# =============================================================================
# Bounds checking
# =============================================================================
# Based on Julia's base/abstractarray.jl

# checkbounds(Bool, A, i) - return true if index i is valid for array A
function checkbounds(::Type{Bool}, A, i::Int64)
    return 1 <= i && i <= length(A)
end

# checkbounds(Bool, A, i) - fallback for non-Int64 indices
function checkbounds(::Type{Bool}, A, i)
    return checkbounds(Bool, A, Int64(i))
end

# checkbounds(A, i) - throw BoundsError if index i is not valid for array A
function checkbounds(A, i::Int64)
    if !(1 <= i <= length(A))
        throw(BoundsError(A, i))
    end
    return nothing
end

# checkbounds(A, i) - fallback for non-Int64 indices
function checkbounds(A, i)
    return checkbounds(A, Int64(i))
end

# checkindex(Bool, inds, i) - check if index i is within range inds
function checkindex(::Type{Bool}, inds, i::Int64)
    # For UnitRange (1:n style), use firstindex/lastindex
    first_idx = inds[1]
    last_idx = inds[length(inds)]
    return first_idx <= i && i <= last_idx
end

# checkindex(Bool, inds, i) - fallback for non-Int64 indices
function checkindex(::Type{Bool}, inds, i)
    return checkindex(Bool, inds, Int64(i))
end

# =============================================================================
# isassigned - check if array index has an assigned value (Issue #1836)
# =============================================================================
# Based on Julia's base/essentials.jl:1007-1038 and base/array.jl:229-242
#
# In SubsetJuliaVM, all array elements for isbits types (Int64, Float64, Bool,
# etc.) are always assigned, so isassigned simplifies to a bounds check.

function isassigned(a, i::Int64)
    return 1 <= i && i <= length(a)
end

function isassigned(a, i::Integer)
    return isassigned(a, Int64(i))
end

# =============================================================================
# popat! - remove and return element at index
# =============================================================================
# Based on Julia's base/array.jl:1710-1725
#
# popat!(a, i) removes and returns the element at index i
# popat!(a, i, default) returns default if index is out of bounds

function popat!(a, i::Int64)
    x = a[i]
    deleteat!(a, i)
    return x
end

function popat!(a, i::Int64, default)
    n = length(a)
    if 1 <= i && i <= n
        x = a[i]
        deleteat!(a, i)
        return x
    else
        return default
    end
end

# =============================================================================
# Boolean array construction functions
# =============================================================================
# Based on Julia's base/bitarray.jl:393-416
#
# In Julia, trues/falses return BitArray (compact boolean storage).
# In SubsetJuliaVM, we return Vector{Bool} for simplicity.

# trues(n) - create 1D array of true values
# trues(dims...) - create multi-dimensional array of true values
function trues(dims...)
    n = length(dims)
    if n == 1
        # 1D array
        len = dims[1]
        result = Bool[]
        i = 1
        while i <= len
            push!(result, true)
            i = i + 1
        end
        return result
    elseif n == 2
        # 2D array - use reshape pattern
        rows = dims[1]
        cols = dims[2]
        result = Bool[]
        total = rows * cols
        i = 1
        while i <= total
            push!(result, true)
            i = i + 1
        end
        return reshape(result, rows, cols)
    elseif n == 3
        # 3D array
        d1 = dims[1]
        d2 = dims[2]
        d3 = dims[3]
        result = Bool[]
        total = d1 * d2 * d3
        i = 1
        while i <= total
            push!(result, true)
            i = i + 1
        end
        return reshape(result, d1, d2, d3)
    else
        error("trues: only supports up to 3 dimensions")
    end
end

# falses(n) - create 1D array of false values
# falses(dims...) - create multi-dimensional array of false values
function falses(dims...)
    n = length(dims)
    if n == 1
        # 1D array
        len = dims[1]
        result = Bool[]
        i = 1
        while i <= len
            push!(result, false)
            i = i + 1
        end
        return result
    elseif n == 2
        # 2D array
        rows = dims[1]
        cols = dims[2]
        result = Bool[]
        total = rows * cols
        i = 1
        while i <= total
            push!(result, false)
            i = i + 1
        end
        return reshape(result, rows, cols)
    elseif n == 3
        # 3D array
        d1 = dims[1]
        d2 = dims[2]
        d3 = dims[3]
        result = Bool[]
        total = d1 * d2 * d3
        i = 1
        while i <= total
            push!(result, false)
            i = i + 1
        end
        return reshape(result, d1, d2, d3)
    else
        error("falses: only supports up to 3 dimensions")
    end
end

# =============================================================================
# fill - create array filled with a value
# =============================================================================
# Based on Julia's base/array.jl
#
# fill(value, dims...) creates an array filled with the given value.
# The element type is determined by the value's type.

function fill(value, dims...)
    n = length(dims)
    if n == 1
        # 1D array
        len = dims[1]
        # Determine result type based on value type
        if isa(value, Bool)
            result = Bool[]
            i = 1
            while i <= len
                push!(result, value)
                i = i + 1
            end
            return result
        elseif isa(value, Int64)
            result = Int64[]
            i = 1
            while i <= len
                push!(result, value)
                i = i + 1
            end
            return result
        elseif isa(value, String)
            # String arrays (Issue #2177)
            result = String[]
            i = 1
            while i <= len
                push!(result, value)
                i = i + 1
            end
            return result
        else
            # Default to Float64 for numeric types
            result = Float64[]
            i = 1
            while i <= len
                push!(result, Float64(value))
                i = i + 1
            end
            return result
        end
    elseif n == 2
        # 2D array
        rows = dims[1]
        cols = dims[2]
        total = rows * cols
        if isa(value, Bool)
            result = Bool[]
            i = 1
            while i <= total
                push!(result, value)
                i = i + 1
            end
            return reshape(result, rows, cols)
        elseif isa(value, Int64)
            result = Int64[]
            i = 1
            while i <= total
                push!(result, value)
                i = i + 1
            end
            return reshape(result, rows, cols)
        elseif isa(value, String)
            # String arrays (Issue #2177)
            result = String[]
            i = 1
            while i <= total
                push!(result, value)
                i = i + 1
            end
            return reshape(result, rows, cols)
        else
            result = Float64[]
            i = 1
            while i <= total
                push!(result, Float64(value))
                i = i + 1
            end
            return reshape(result, rows, cols)
        end
    elseif n == 3
        # 3D array
        d1 = dims[1]
        d2 = dims[2]
        d3 = dims[3]
        total = d1 * d2 * d3
        if isa(value, Bool)
            result = Bool[]
            i = 1
            while i <= total
                push!(result, value)
                i = i + 1
            end
            return reshape(result, d1, d2, d3)
        elseif isa(value, Int64)
            result = Int64[]
            i = 1
            while i <= total
                push!(result, value)
                i = i + 1
            end
            return reshape(result, d1, d2, d3)
        elseif isa(value, String)
            # String arrays (Issue #2177)
            result = String[]
            i = 1
            while i <= total
                push!(result, value)
                i = i + 1
            end
            return reshape(result, d1, d2, d3)
        else
            result = Float64[]
            i = 1
            while i <= total
                push!(result, Float64(value))
                i = i + 1
            end
            return reshape(result, d1, d2, d3)
        end
    else
        error("fill: only supports up to 3 dimensions")
    end
end

# =============================================================================
# resize! - Resize vector to new length
# =============================================================================
# Based on Julia's base/array.jl:1533
#
# resize!(a, n) resizes collection a to contain n elements.
# If n is smaller than the current size, the first n elements are retained.
# If n is larger, the collection is extended with uninitialized values
# (zeros for numeric types, false for Bool).

function resize!(a::Array, n::Int64)
    current = length(a)
    if n < 0
        error("resize!: new length must be ≥ 0")
    end
    if n > current
        # Grow: push default values (zeros)
        # Get element type and push appropriate default
        et = eltype(a)
        if et == Bool
            default = false
        elseif et == Int64
            default = Int64(0)
        else
            default = 0.0
        end
        i = current
        while i < n
            push!(a, default)
            i = i + 1
        end
    elseif n < current
        # Shrink: pop elements from end
        i = current
        while i > n
            pop!(a)
            i = i - 1
        end
    end
    return a
end

# =============================================================================
# keepat! - Keep only elements at specified indices
# =============================================================================
# Based on Julia's base/array.jl:3078
#
# keepat!(a, inds) removes items at all indices NOT in inds, and returns
# the modified array. Items are shifted to fill gaps.
# inds must be sorted and unique integer indices.
# keepat!(a, m::Vector{Bool}) keeps elements where m[i] is true.

function keepat!(a::Array, inds)
    # Check if inds is a boolean mask
    if length(inds) > 0 && eltype(inds) == Bool
        # Boolean mask version
        if length(inds) != length(a)
            error("keepat!: mask length must match array length")
        end

        j = 1
        for i in 1:length(a)
            # Direct truthiness check works for Bool values
            if inds[i]
                if j != i
                    a[j] = a[i]
                end
                j = j + 1
            end
        end

        # Remove remaining elements
        resize!(a, j - 1)
        return a
    else
        # Integer indices version
        # Validate that indices are sorted and unique
        n_keep = length(inds)
        if n_keep > 0
            prev = inds[1]
            for i in 2:n_keep
                curr = inds[i]
                if curr <= prev
                    error("keepat!: indices must be unique and sorted")
                end
                prev = curr
            end
        end

        # Move elements to keep to the front
        j = 1
        for k in inds
            if k < 1 || k > length(a)
                error("keepat!: index out of bounds")
            end
            if j != k
                a[j] = a[k]
            end
            j = j + 1
        end

        # Remove remaining elements from the end
        resize!(a, n_keep)
        return a
    end
end

# =============================================================================
# pushfirst! - Insert element at beginning of array
# =============================================================================
# Based on Julia's base/array.jl:1746
#
# pushfirst!(a, item) inserts item at the beginning of a and returns a.
# All existing elements are shifted to the right by one position.

function pushfirst!(a::Array, item)
    n = length(a)
    # Extend array by one element
    resize!(a, n + 1)
    # Shift all elements to the right
    i = n + 1
    while i > 1
        a[i] = a[i - 1]
        i = i - 1
    end
    # Insert item at position 1
    a[1] = item
    return a
end

# =============================================================================
# popfirst! - Remove and return first element
# =============================================================================
# Based on Julia's base/array.jl:1805
#
# popfirst!(a) removes the first element from a and returns it.
# Throws an error if array is empty.

function popfirst!(a::Array)
    n = length(a)
    if n == 0
        error("popfirst!: array must be non-empty")
    end
    # Get first element
    item = a[1]
    # Shift all elements to the left
    i = 1
    while i < n
        a[i] = a[i + 1]
        i = i + 1
    end
    # Shrink array
    resize!(a, n - 1)
    return item
end

# =============================================================================
# insert! - Insert element at specific index
# =============================================================================
# Based on Julia's base/array.jl:1830
#
# insert!(a, index, item) inserts item into a at the given index.
# index is the index of item in the resulting array.

function insert!(a::Array, index::Int64, item)
    n = length(a)
    if index < 1 || index > n + 1
        error("insert!: index out of bounds")
    end
    # Extend array by one element
    resize!(a, n + 1)
    # Shift elements from index to end to the right
    i = n + 1
    while i > index
        a[i] = a[i - 1]
        i = i - 1
    end
    # Insert item at index
    a[index] = item
    return a
end

# =============================================================================
# append! - Append elements from collection to array
# =============================================================================
# Based on Julia's base/array.jl:1408
#
# append!(a, items) adds all items from collection to the end of a.
# Returns the modified array.

function append!(a::Array, items)
    for item in items
        push!(a, item)
    end
    return a
end

# =============================================================================
# prepend! - Prepend elements from collection to array
# =============================================================================
# Based on Julia's base/array.jl:1428
#
# prepend!(a, items) adds all items from collection to the beginning of a.
# Returns the modified array.

function prepend!(a::Array, items)
    # Collect items first to get length (since items may be an iterator)
    items_arr = collect(items)
    m = length(items_arr)
    if m == 0
        return a
    end
    n = length(a)
    # Extend array
    resize!(a, n + m)
    # Shift existing elements to the right by m positions
    i = n + m
    while i > m
        a[i] = a[i - m]
        i = i - 1
    end
    # Copy items to the beginning
    for i in 1:m
        a[i] = items_arr[i]
    end
    return a
end

# =============================================================================
# deleteat! - Delete element at index
# =============================================================================
# Based on Julia's base/array.jl:1880
#
# deleteat!(a, i) removes the element at index i from a.
# Returns the modified array.

function deleteat!(a::Array, i::Int64)
    n = length(a)
    if i < 1 || i > n
        error("deleteat!: index out of bounds")
    end
    # Shift elements to the left
    j = i
    while j < n
        a[j] = a[j + 1]
        j = j + 1
    end
    # Shrink array
    resize!(a, n - 1)
    return a
end

# =============================================================================
# indexin - Find indices of first collection in second
# =============================================================================
# Based on Julia's base/array.jl:2861
#
# indexin(a, b) returns an array containing the first index in b for each
# value in a that is a member of b. Returns nothing for elements not found.

function indexin(a, b)
    # Build a dictionary for O(1) lookup
    bdict = Dict{Any, Int64}()
    for i in 1:length(b)
        val = b[i]
        if !haskey(bdict, val)
            bdict[val] = i
        end
    end

    # Look up each element of a
    result = []
    for val in a
        if haskey(bdict, val)
            push!(result, bdict[val])
        else
            push!(result, nothing)
        end
    end
    return result
end

# =============================================================================
# Permutation functions
# =============================================================================
# Based on Julia's base/combinatorics.jl

# isperm(p) - Check if p is a valid permutation of 1:n
# A valid permutation contains each integer from 1 to n exactly once
function isperm(p)
    n = length(p)
    if n == 0
        return true
    end

    # Use a boolean array to track which values we've seen
    seen = falses(n)
    for val in p
        # Check if val is a valid integer in range 1:n
        if !isa(val, Int64) && !isa(val, Int)
            return false
        end
        idx = Int64(val)
        if idx < 1 || idx > n
            return false
        end
        # Check if we've already seen this value
        if seen[idx]  # Direct truthiness check
            return false
        end
        seen[idx] = true
    end
    return true
end

# invperm(p) - Compute the inverse permutation
# If p is a permutation, then invperm(p)[p[i]] == i for all i
function invperm(p)
    n = length(p)
    if !isperm(p)
        error("invperm: argument is not a permutation")
    end

    # Create result array
    result = zeros(Int64, n)
    for i in 1:n
        result[p[i]] = i
    end
    return result
end

# =============================================================================
# empty! - Remove all elements from a collection
# =============================================================================
# Based on Julia's base/array.jl:2124
#
# empty!(a) removes all elements from a and returns the modified collection.

function empty!(a::Array)
    resize!(a, 0)
    return a
end

# =============================================================================
# filter! - Filter array elements in place
# =============================================================================
# Based on Julia's base/array.jl:3035
#
# filter!(f, a) removes elements from a for which f returns false.
# Returns the modified array.

function filter!(f, a::Array)
    j = 1
    for i in 1:length(a)
        ai = a[i]
        if f(ai)
            if j != i
                a[j] = ai
            end
            j = j + 1
        end
    end
    # Resize to keep only the elements that passed the filter
    resize!(a, j - 1)
    return a
end

# =============================================================================
# splice! - Remove and optionally replace elements
# =============================================================================
# Based on Julia's base/array.jl:2039
#
# splice!(a, i) removes element at index i and returns it.
# splice!(a, i, v) replaces element at index i with v and returns old element.
# splice!(a, r) removes elements in range r and returns them.
# splice!(a, r, ins) removes elements in range r, inserts ins, returns old elements.

# splice!(a, i) - Remove and return element at index i
function splice!(a::Array, i::Int64)
    n = length(a)
    if i < 1 || i > n
        error("splice!: index out of bounds")
    end
    # Save the element to return
    v = a[i]
    # Shift elements left
    j = i
    while j < n
        a[j] = a[j + 1]
        j = j + 1
    end
    # Shrink array
    resize!(a, n - 1)
    return v
end

# splice!(a, i, replacement) - Replace element at index i with replacement
# Note: replacement can be a single value or an array of values
function splice!(a::Array, i::Int64, replacement)
    n = length(a)
    if i < 1 || i > n
        error("splice!: index out of bounds")
    end

    # Save the old element
    v = a[i]

    # Check if replacement is an array
    if isa(replacement, Array)
        items_arr = replacement
        m = length(items_arr)
        if m == 0
            # Remove element without replacement
            j = i
            while j < n
                a[j] = a[j + 1]
                j = j + 1
            end
            resize!(a, n - 1)
        elseif m == 1
            # Single element replacement
            a[i] = items_arr[1]
        else
            # Multiple element replacement: need to grow array
            resize!(a, n + m - 1)
            # Shift elements right
            j = n + m - 1
            while j > i + m - 1
                a[j] = a[j - m + 1]
                j = j - 1
            end
            # Insert new elements
            for k in 1:m
                a[i + k - 1] = items_arr[k]
            end
        end
    else
        # Single value replacement
        a[i] = replacement
    end

    return v
end

# =============================================================================
# map! - Apply function to array elements in-place
# =============================================================================
# Based on Julia's base/abstractarray.jl:3381
#
# map!(f, dest, A) applies f to each element of A and stores result in dest.
# map!(f, A) applies f to each element of A in-place (modifies A).

# map!(f, A) - Apply f to A in-place (simple 2-arg version)
function map!(f, a::Array)
    for i in 1:length(a)
        ai = a[i]
        a[i] = f(ai)
    end
    return a
end

# map!(f, dest, A) - Apply f to A and store in dest (3-arg version)
# Note: In Julia, map! processes min(length(dest), length(src)) elements
# and doesn't resize the destination array
function map!(f, dest::Array, src::Array)
    n = min(length(dest), length(src))
    for i in 1:n
        ai = src[i]
        dest[i] = f(ai)
    end
    return dest
end

# NOTE: map!(f, dest, A, B) 4-arg version not implemented
# The VM currently doesn't support calling function parameters with multiple arguments
# (i.e., f(ai, bi) doesn't compile correctly when f is a parameter)

# =============================================================================
# clamp! - Clamp array values in place
# =============================================================================
# Based on Julia's base/math.jl clamp! function
#
# clamp!(a, lo, hi) restricts each element of a to the interval [lo, hi].
# Values less than lo are set to lo, values greater than hi are set to hi.

"""
    clamp!(a, lo, hi)

Restrict values in array `a` to the interval [`lo`, `hi`], in-place.
For each element, values less than `lo` will become `lo` and values greater
than `hi` will become `hi`.

Returns the modified array `a`.

# Examples
```julia
julia> a = [1.0, 5.0, 10.0, 15.0];
julia> clamp!(a, 3, 12);
julia> a
4-element Vector{Float64}:
  3.0
  5.0
 10.0
 12.0
```
"""
function clamp!(a, lo, hi)
    n = length(a)
    for i in 1:n
        x = a[i]
        if x < lo
            a[i] = lo
        elseif x > hi
            a[i] = hi
        end
    end
    return a
end

# =============================================================================
# cat: general array concatenation along a specified dimension
# =============================================================================
# Based on Julia's Base.cat

# cat(A, B; dims): concatenate two arrays along dimension dims
# dims=1: vertical concatenation (like vcat for matrices)
# dims=2: horizontal concatenation (like hcat for matrices)
function cat(A, B; dims)
    sA = size(A)
    sB = size(B)
    ndA = length(sA)
    ndB = length(sB)
    if dims == 1
        if ndA == 1 && ndB == 1
            # Both 1D: concatenate elements
            na = length(A)
            nb = length(B)
            result = zeros(na + nb)
            for i in 1:na
                result[i] = A[i]
            end
            for i in 1:nb
                result[na + i] = B[i]
            end
            return result
        else
            # 2D: vertical concatenation
            mA = size(A, 1)
            nA = size(A, 2)
            mB = size(B, 1)
            nB = size(B, 2)
            if nA != nB
                error("cat: dimension mismatch along dim 2: $nA vs $nB")
            end
            result = zeros(mA + mB, nA)
            for i in 1:mA
                for j in 1:nA
                    result[i, j] = A[i, j]
                end
            end
            for i in 1:mB
                for j in 1:nB
                    result[mA + i, j] = B[i, j]
                end
            end
            return result
        end
    elseif dims == 2
        if ndA == 1 && ndB == 1
            # Both 1D: treat as column vectors, produce matrix
            na = length(A)
            nb = length(B)
            if na != nb
                error("cat: dimension mismatch along dim 1: $na vs $nb")
            end
            result = zeros(na, 2)
            for i in 1:na
                result[i, 1] = A[i]
                result[i, 2] = B[i]
            end
            return result
        else
            # 2D: horizontal concatenation
            mA = size(A, 1)
            nA = size(A, 2)
            mB = size(B, 1)
            nB = size(B, 2)
            if mA != mB
                error("cat: dimension mismatch along dim 1: $mA vs $mB")
            end
            result = zeros(mA, nA + nB)
            for i in 1:mA
                for j in 1:nA
                    result[i, j] = A[i, j]
                end
            end
            for i in 1:mB
                for j in 1:nB
                    result[i, nA + j] = B[i, j]
                end
            end
            return result
        end
    else
        error("cat: dims must be 1 or 2 for 2D arrays")
    end
end

# =============================================================================
# mapslices: apply a function to slices of an array along a dimension
# =============================================================================
# Based on Julia's Base.mapslices

# mapslices(f, A; dims): apply f to each slice of A along dimension dims
# For 2D matrices:
#   dims=1: apply f to each column (slices along rows)
#   dims=2: apply f to each row (slices along columns)
function mapslices(f, A; dims)
    m = size(A, 1)
    n = size(A, 2)
    if dims == 1
        # Apply f to each column
        results = zeros(n)
        for j in 1:n
            col = zeros(m)
            for i in 1:m
                col[i] = A[i, j]
            end
            results[j] = f(col)
        end
        return results
    elseif dims == 2
        # Apply f to each row
        results = zeros(m)
        for i in 1:m
            row = zeros(n)
            for j in 1:n
                row[j] = A[i, j]
            end
            results[i] = f(row)
        end
        return results
    else
        error("mapslices: dims must be 1 or 2 for 2D arrays")
    end
end

# sortslices: sort slices of an array along a dimension
# dims=1: sort rows by comparing row vectors lexicographically
# dims=2: sort columns by comparing column vectors lexicographically
function sortslices(A; dims)
    m = size(A, 1)
    n = size(A, 2)
    if dims == 1
        # Sort rows: compare row i and row j lexicographically
        # Create index array and sort it
        idx = collect(1:m)
        # Insertion sort on indices
        for i in 2:m
            key = idx[i]
            j = i - 1
            while j >= 1
                # Compare row idx[j] vs row key lexicographically
                should_swap = false
                for col in 1:n
                    if A[idx[j], col] > A[key, col]
                        should_swap = true
                        break
                    elseif A[idx[j], col] < A[key, col]
                        break
                    end
                end
                if !should_swap
                    break
                end
                idx[j + 1] = idx[j]
                j = j - 1
            end
            idx[j + 1] = key
        end
        # Build result matrix with sorted rows
        result = zeros(m, n)
        for i in 1:m
            for j in 1:n
                result[i, j] = A[idx[i], j]
            end
        end
        return result
    elseif dims == 2
        # Sort columns: compare column i and column j lexicographically
        idx = collect(1:n)
        for i in 2:n
            key = idx[i]
            j = i - 1
            while j >= 1
                should_swap = false
                for row in 1:m
                    if A[row, idx[j]] > A[row, key]
                        should_swap = true
                        break
                    elseif A[row, idx[j]] < A[row, key]
                        break
                    end
                end
                if !should_swap
                    break
                end
                idx[j + 1] = idx[j]
                j = j - 1
            end
            idx[j + 1] = key
        end
        # Build result matrix with sorted columns
        result = zeros(m, n)
        for i in 1:m
            for j in 1:n
                result[i, j] = A[i, idx[j]]
            end
        end
        return result
    else
        error("sortslices: dims must be 1 or 2 for 2D arrays")
    end
end

# =============================================================================
# findnext / findprev with predicate function (Issue #2109)
# =============================================================================
# Based on Julia's base/array.jl
# findnext(testf, A, start) - find next index >= start where testf(A[i]) is true
# findprev(testf, A, start) - find prev index <= start where testf(A[i]) is true

function findnext(testf::Function, A, start::Int64)
    n = length(A)
    i = start
    while i <= n
        if testf(A[i])
            return i
        end
        i = i + 1
    end
    return nothing
end

function findprev(testf::Function, A, start::Int64)
    i = start
    while i >= 1
        if testf(A[i])
            return i
        end
        i = i - 1
    end
    return nothing
end
